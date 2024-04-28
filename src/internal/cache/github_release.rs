use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io;

use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_github_release_operation_cache;
use crate::internal::cache::loaders::set_github_release_operation_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::env::now as omni_now;
use crate::internal::self_updater::compatible_release_arch;
use crate::internal::self_updater::compatible_release_os;

const GITHUB_RELEASE_CACHE_NAME: &str = "github_release_operation";

lazy_static! {
    static ref OS_REGEX: Regex = match Regex::new(&format!(
        r"(?i)(\b|_)({})(\b|_)",
        compatible_release_os().join("|")
    )) {
        Ok(os_re) => os_re,
        Err(err) => panic!("failed to create OS regex: {}", err),
    };
    static ref ARCH_REGEX: Regex = match Regex::new(&format!(
        r"(?i)(\b|_)({})(\b|_)",
        compatible_release_arch().join("|")
    )) {
        Ok(arch_re) => arch_re,
        Err(err) => panic!("failed to create architecture regex: {}", err),
    };
    static ref SEPARATOR_MID_REGEX: Regex = match Regex::new(r"([-_]{2,})") {
        Ok(separator_re) => separator_re,
        Err(err) => panic!("failed to create separator regex: {}", err),
    };
    static ref SEPARATOR_END_REGEX: Regex = match Regex::new(r"(^[-_]+|[-_]+$)") {
        Ok(separator_re) => separator_re,
        Err(err) => panic!("failed to create separator regex: {}", err),
    };
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleaseOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<GithubReleaseInstalled>,
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub releases: BTreeMap<String, GithubReleases>,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

impl GithubReleaseOperationCache {
    pub fn updated(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn add_releases(&mut self, repository: &str, releases: &GithubReleases) -> bool {
        self.releases
            .insert(repository.to_string(), releases.clone());
        self.updated();
        true
    }

    pub fn get_releases(&self, repository: &str) -> Option<&GithubReleases> {
        self.releases.get(repository)
    }

    pub fn add_installed(&mut self, workdir_id: &str, repository: &str, version: &str) -> bool {
        let inserted = if let Some(install) = self
            .installed
            .iter_mut()
            .find(|i| i.repository == repository && i.version == version)
        {
            if install.required_by.insert(workdir_id.to_string())
                || install.last_required_at < omni_now()
            {
                install.last_required_at = omni_now();
                true
            } else {
                false
            }
        } else {
            let install = GithubReleaseInstalled {
                repository: repository.to_string(),
                version: version.to_string(),
                required_by: [workdir_id.to_string()].iter().cloned().collect(),
                last_required_at: omni_now(),
            };
            self.installed.push(install);
            true
        };

        if inserted {
            self.updated();
        }

        inserted
    }
}

impl Empty for GithubReleaseOperationCache {
    fn is_empty(&self) -> bool {
        self.installed.is_empty() && self.releases.is_empty()
    }
}

impl CacheObject for GithubReleaseOperationCache {
    fn new_empty() -> Self {
        Self {
            installed: Vec::new(),
            releases: BTreeMap::new(),
            updated_at: utils::origin_of_time(),
        }
    }

    fn get() -> Self {
        get_github_release_operation_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(GITHUB_RELEASE_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(
            GITHUB_RELEASE_CACHE_NAME,
            processing_fn,
            set_github_release_operation_cache,
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleaseInstalled {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_by: BTreeSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

impl GithubReleaseInstalled {
    pub fn stale(&self) -> bool {
        self.last_required_at < omni_now()
    }

    pub fn removable(&self) -> bool {
        if !self.required_by.is_empty() {
            return false;
        }

        let config = global_config();
        let grace_period = config.cache.github_release.cleanup_after;
        let grace_period = time::Duration::seconds(grace_period as i64);

        (self.last_required_at + grace_period) < omni_now()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleases {
    pub releases: Vec<GithubReleaseVersion>,
    #[serde(default = "OffsetDateTime::now_utc", with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

impl GithubReleases {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let releases: Vec<GithubReleaseVersion> = match serde_json::from_str(json) {
            Ok(releases) => releases,
            Err(err) => return Err(format!("failed to parse releases: {}", err)),
        };

        Ok(Self {
            releases,
            fetched_at: OffsetDateTime::now_utc(),
        })
    }

    pub fn is_fresh(&self) -> bool {
        self.fetched_at >= omni_now()
    }

    pub fn is_stale(&self, ttl: u64) -> bool {
        let duration = time::Duration::seconds(ttl as i64);
        self.fetched_at + duration < OffsetDateTime::now_utc()
    }

    pub fn get(
        &self,
        version: &str,
        prerelease: bool,
        build: bool,
        binary: bool,
    ) -> Option<(String, GithubReleaseVersion)> {
        let mut matcher = VersionMatcher::new(version);
        matcher.prerelease(prerelease);
        matcher.build(build);
        // We also always authorize `prefix` because we don't know what
        // the prefix is going to be, `v` or anything else
        matcher.prefix(true);

        self.releases
            .iter()
            .filter_map(|release| {
                // Discard drafts as they are not considered releases
                if release.draft {
                    return None;
                }

                // Discard pre-releases if needed
                if !prerelease && release.prerelease {
                    return None;
                }

                // Parse the version
                let release_version = release.version();

                // Make sure the version fits the requested version
                if !matcher.matches(&release_version) {
                    return None;
                }

                // Check that we have one matching asset for the current
                // platform and architecture, that is either a .zip or .tar.gz
                let assets = release
                    .assets
                    .iter()
                    .filter(|asset| {
                        if let Some((asset_type, _)) = asset.file_type() {
                            if asset_type.is_binary() && !binary {
                                return false;
                            }
                        } else {
                            return false;
                        }

                        let asset_name = asset.name.to_lowercase();

                        let compatible_release_os = compatible_release_os();
                        if compatible_release_os
                            .iter()
                            .all(|os| !asset_name.contains(os))
                        {
                            return false;
                        }

                        let compatible_release_arch = compatible_release_arch();
                        if compatible_release_arch
                            .iter()
                            .all(|arch| !asset_name.contains(arch))
                        {
                            return false;
                        }

                        true
                    })
                    .cloned()
                    .collect::<Vec<GithubReleaseAsset>>();

                if assets.is_empty() {
                    return None;
                }

                let release = GithubReleaseVersion {
                    tag_name: release.tag_name.clone(),
                    name: release.name.clone(),
                    draft: release.draft,
                    prerelease: release.prerelease,
                    assets,
                };

                Some((release_version, release))
            })
            .max_by(|(version_a, _), (version_b, _)| version_a.cmp(version_b))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleaseVersion {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<GithubReleaseAsset>,
}

impl GithubReleaseVersion {
    pub fn version(&self) -> String {
        self.tag_name.clone()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleaseAsset {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub browser_download_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_type: String,
    #[serde(default)]
    pub size: u64,
}

impl GithubReleaseAsset {
    const TAR_GZ_EXTS: [&'static str; 2] = [".tar.gz", ".tgz"];
    const ZIP_EXTS: [&'static str; 1] = [".zip"];

    pub fn file_type(&self) -> Option<(GithubReleaseAssetType, String)> {
        for ext in Self::TAR_GZ_EXTS.iter() {
            if let Some(prefix) = self.name.strip_suffix(ext) {
                return Some((GithubReleaseAssetType::TarGz, prefix.to_string()));
            }
        }

        for ext in Self::ZIP_EXTS.iter() {
            if let Some(prefix) = self.name.strip_suffix(ext) {
                return Some((GithubReleaseAssetType::Zip, prefix.to_string()));
            }
        }

        if self.name.ends_with(".exe") {
            return Some((GithubReleaseAssetType::Binary, self.name.clone()));
        }

        if !self.name.contains('.') {
            return Some((GithubReleaseAssetType::Binary, self.name.clone()));
        }

        None
    }

    pub fn clean_name(&self, version: &str) -> String {
        let name = self.name.clone();
        let name = OS_REGEX.replace_all(&name, "$1$3");
        let name = ARCH_REGEX.replace_all(&name, "$1$3");
        let name = name.replace(version, "");
        let name = SEPARATOR_MID_REGEX.replace_all(&name, "-");
        let name = SEPARATOR_END_REGEX.replace_all(&name, "");
        name.to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum GithubReleaseAssetType {
    TarGz,
    Zip,
    Binary,
}

impl GithubReleaseAssetType {
    pub fn is_zip(&self) -> bool {
        matches!(self, Self::Zip)
    }

    pub fn is_tgz(&self) -> bool {
        matches!(self, Self::TarGz)
    }

    pub fn is_binary(&self) -> bool {
        matches!(self, Self::Binary)
    }
}
