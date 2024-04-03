use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io;
use std::str::FromStr;

use lazy_static::lazy_static;
use node_semver::Version as semverVersion;
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
use crate::internal::config::up::asdf_base::version_match;
use crate::internal::self_updater::compatible_release_arch;
use crate::internal::self_updater::compatible_release_os;

const GITHUB_RELEASE_CACHE_NAME: &str = "github_release_operation";

lazy_static! {
    static ref GITHUB_RELEASE_OPERATION_NOW: OffsetDateTime = OffsetDateTime::now_utc();
}

// TODO: merge this with homebrew_operation_now, maybe up_operation_now?
fn github_release_operation_now() -> OffsetDateTime {
    *GITHUB_RELEASE_OPERATION_NOW
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
                || install.last_required_at < github_release_operation_now()
            {
                install.last_required_at = github_release_operation_now();
                true
            } else {
                false
            }
        } else {
            let install = GithubReleaseInstalled {
                repository: repository.to_string(),
                version: version.to_string(),
                required_by: [workdir_id.to_string()].iter().cloned().collect(),
                last_required_at: github_release_operation_now(),
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
        self.last_required_at < github_release_operation_now()
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

    pub fn is_stale(&self, ttl: u64) -> bool {
        let duration = time::Duration::seconds(ttl as i64);
        self.fetched_at + duration < OffsetDateTime::now_utc()
    }

    pub fn get(
        &self,
        version: &str,
        prerelease: bool,
    ) -> Option<(semverVersion, GithubReleaseVersion)> {
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
                let release_version = match release.version() {
                    Ok(release_version) => release_version,
                    Err(_) => {
                        return None;
                    }
                };

                // Make sure the version fits the requested version
                if !version_match(version, &release_version.to_string(), prerelease) {
                    return None;
                }

                // Check that we have one matching asset for the current
                // platform and architecture, that is either a .zip or .tar.gz
                let assets = release
                    .assets
                    .iter()
                    .filter(|asset| {
                        let is_tgz =
                            asset.name.ends_with(".tar.gz") || asset.name.ends_with(".tgz");
                        let is_zip = asset.name.ends_with(".zip");

                        if !is_tgz && !is_zip {
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
    pub tag_name: String,
    pub name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<GithubReleaseAsset>,
}

impl GithubReleaseVersion {
    pub fn version(&self) -> Result<semverVersion, String> {
        // Get the version as the tag name but ideally without the first v
        let version = match self.tag_name.strip_prefix('v') {
            Some(version) => version,
            None => &self.tag_name,
        };

        semverVersion::from_str(version).map_err(|err| format!("failed to parse version: {}", err))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub state: String,
    pub content_type: String,
    pub size: u64,
}
