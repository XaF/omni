use std::collections::HashSet;

use globset::Glob;
use itertools::Itertools;
use lazy_static::lazy_static;
use regex::escape as regex_escape;
use regex::Regex;
use rusqlite::params;
use rusqlite::Row;
use serde::Deserialize;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::config::up::utils::VersionParser;
use crate::internal::env::now as omni_now;
use crate::internal::self_updater::compatible_release_arch;
use crate::internal::self_updater::compatible_release_os;

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
        compatible_release_arch().into_iter().flatten().join("|")
    )) {
        Ok(arch_re) => arch_re,
        Err(err) => panic!("failed to create architecture regex: {}", err),
    };
    static ref ARCH_REGEX_PER_LEVEL: Vec<Regex> = compatible_release_arch()
        .into_iter()
        .map(|arch| {
            match Regex::new(&format!(r"(?i)(\b|_)({})(\b|_)", arch.join("|"))) {
                Ok(arch_re) => arch_re,
                Err(err) => panic!("failed to create architecture regex: {}", err),
            }
        })
        .collect();
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
pub struct GithubReleaseOperationCache {}

impl GithubReleaseOperationCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn add_releases(
        &self,
        repository: &str,
        releases: &GithubReleases,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/github_release_operation_add_releases.sql"),
            params![repository, serde_json::to_string(&releases.releases)?],
        )?;
        Ok(inserted > 0)
    }

    pub fn get_releases(&self, repository: &str) -> Option<GithubReleases> {
        let db = CacheManager::get();
        let releases: Option<GithubReleases> = db
            .query_one(
                include_str!("database/sql/github_release_operation_get_releases.sql"),
                params![repository],
            )
            .ok();
        releases
    }

    pub fn add_installed(
        &self,
        repository: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/github_release_operation_add_install.sql"),
            params![repository, version],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_required_by(
        &self,
        env_version_id: &str,
        repository: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/github_release_operation_add_install_required_by.sql"),
            params![repository, version, env_version_id],
        )?;
        Ok(inserted > 0)
    }

    pub fn list_installed(&self) -> Result<Vec<GithubReleaseInstalled>, CacheManagerError> {
        let db = CacheManager::get();
        let installed: Vec<GithubReleaseInstalled> = db.query_as(
            include_str!("database/sql/github_release_operation_list_installed.sql"),
            params![],
        )?;
        Ok(installed)
    }

    pub fn cleanup(&self) -> Result<(), CacheManagerError> {
        let config = global_config();
        let db = CacheManager::get();

        db.execute(
            include_str!("database/sql/github_release_operation_cleanup_installed.sql"),
            params![&config.cache.github_release.cleanup_after],
        )?;

        db.execute(
            include_str!("database/sql/github_release_operation_cleanup_versions.sql"),
            params![&config.cache.github_release.versions_retention],
        )?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleaseInstalled {
    pub repository: String,
    pub version: String,
}

impl FromRow for GithubReleaseInstalled {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        Ok(Self {
            repository: row.get("repository")?,
            version: row.get("version")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GithubReleasesSelector {
    pub version: String,
    pub prerelease: bool,
    pub build: bool,
    pub binary: bool,
    pub asset_name: Option<String>,
    pub skip_arch_matching: bool,
    pub skip_os_matching: bool,
    pub checksum_lookup: bool,
    pub checksum_algorithm: Option<String>,
    pub checksum_asset_name: Option<String>,
}

impl GithubReleasesSelector {
    pub fn new(version: &str) -> Self {
        Self {
            version: version.to_string(),
            ..Self::default()
        }
    }

    pub fn prerelease(mut self, prerelease: bool) -> Self {
        self.prerelease = prerelease;
        self
    }

    pub fn build(mut self, build: bool) -> Self {
        self.build = build;
        self
    }

    pub fn binary(mut self, binary: bool) -> Self {
        self.binary = binary;
        self
    }

    pub fn asset_name(mut self, asset_name: Option<String>) -> Self {
        self.asset_name = asset_name;
        self
    }

    pub fn skip_arch_matching(mut self, skip_arch_matching: bool) -> Self {
        self.skip_arch_matching = skip_arch_matching;
        self
    }

    pub fn skip_os_matching(mut self, skip_os_matching: bool) -> Self {
        self.skip_os_matching = skip_os_matching;
        self
    }

    pub fn checksum_lookup(mut self, checksum_lookup: bool) -> Self {
        self.checksum_lookup = checksum_lookup;
        self
    }

    pub fn checksum_algorithm(mut self, checksum_algorithm: Option<String>) -> Self {
        self.checksum_algorithm = checksum_algorithm;
        self
    }

    pub fn checksum_asset_name(mut self, checksum_asset_name: Option<String>) -> Self {
        self.checksum_asset_name = checksum_asset_name;
        self
    }

    // Use a tiny int for the matching: -1 means no matching,
    // 0+ means matching and the lowest value is the best match
    fn asset_matches(&self, asset: &GithubReleaseAsset) -> i32 {
        if let Some((asset_type, _)) = asset.file_type() {
            if asset_type.is_binary() && !self.binary {
                return -1;
            }
        } else {
            return -1;
        }

        if let Some(ref patterns) = self.asset_name {
            if !Self::matches_glob_patterns(patterns, &asset.name) {
                return -1;
            }
        }

        let asset_name = asset.name.to_lowercase();

        if !self.skip_os_matching && !OS_REGEX.is_match(&asset_name) {
            return -1;
        }

        if !self.skip_arch_matching {
            for (idx, arch_level_reg) in ARCH_REGEX_PER_LEVEL.iter().enumerate() {
                if arch_level_reg.is_match(&asset_name) {
                    return idx as i32;
                }
            }
            return -1;
        }

        0
    }

    fn matches_glob_patterns(patterns: &str, value: &str) -> bool {
        let patterns = patterns.split('\n').collect::<Vec<&str>>();

        let mut has_positive_pattern = false;
        let mut matched = false;

        for pattern in patterns {
            if pattern.is_empty() {
                continue;
            }

            let (should_match, pattern) = if let Some(pattern) = pattern.strip_prefix('!') {
                (false, pattern)
            } else {
                has_positive_pattern = true;
                (true, pattern)
            };

            let glob = match Glob::new(pattern) {
                Ok(glob) => glob.compile_matcher(),
                Err(_) => continue,
            };

            if glob.is_match(value) {
                if should_match {
                    matched = true;
                    break;
                } else {
                    return false;
                }
            }
        }

        // Fail right away if we have any positive pattern
        // and none of them matched
        if !matched && has_positive_pattern {
            return false;
        }

        true
    }

    fn assets_with_checksums(&self, assets: &[GithubReleaseAsset]) -> Vec<GithubReleaseAsset> {
        // This will prioritize an exact-arch matching over an extended-arch matching
        let asset_with_matching = assets.iter().filter_map(|asset| {
            let matching_level = self.asset_matches(asset);
            if matching_level < 0 {
                return None;
            }
            Some((asset, matching_level))
        });

        let highest_matching_level = asset_with_matching
            .clone()
            .map(|(_, level)| level)
            .min()
            .unwrap_or(-1);

        if highest_matching_level < 0 {
            return vec![];
        }

        let mut matching_assets = asset_with_matching
            .filter(|(_, level)| *level == highest_matching_level)
            .map(|(asset, _)| asset.clone())
            .collect::<Vec<GithubReleaseAsset>>();

        if !self.checksum_lookup {
            return matching_assets;
        }

        let search_assets = assets
            .iter()
            .filter(|asset| {
                (asset.content_type == "application/octet-stream"
                    || asset.content_type == "text/plain"
                    || asset.content_type.starts_with("text/plain;"))
                    && !matching_assets.iter().any(|a| a.name == asset.name)
            })
            .cloned()
            .collect::<Vec<GithubReleaseAsset>>();

        let (search_assets, guessing) =
            if let Some(ref checksum_asset_name) = self.checksum_asset_name {
                // If there is a pattern, we will only look for that pattern
                let search_assets = search_assets
                    .iter()
                    .filter(|asset| Self::matches_glob_patterns(checksum_asset_name, &asset.name))
                    .cloned()
                    .collect::<Vec<GithubReleaseAsset>>();

                (search_assets, false)
            } else {
                (search_assets, true)
            };

        for asset in &mut matching_assets {
            let asset_without_ext = match asset.file_type() {
                Some((_, prefix)) => prefix,
                None => asset.name.clone(),
            };

            if !guessing {
                if search_assets.len() == 1 {
                    asset.checksum_asset = Some(Box::new(search_assets.first().cloned().unwrap()));
                    continue;
                }

                let with_asset_name = search_assets
                    .iter()
                    .filter(|a| a.name.starts_with(&asset.name))
                    .cloned()
                    .collect::<Vec<GithubReleaseAsset>>();
                if with_asset_name.len() == 1 {
                    asset.checksum_asset =
                        Some(Box::new(with_asset_name.first().cloned().unwrap()));
                    continue;
                }

                let with_asset_name_without_ext = search_assets
                    .iter()
                    .filter(|a| a.name.starts_with(&asset_without_ext))
                    .cloned()
                    .collect::<Vec<GithubReleaseAsset>>();
                if with_asset_name_without_ext.len() == 1 {
                    asset.checksum_asset = Some(Box::new(
                        with_asset_name_without_ext.first().cloned().unwrap(),
                    ));
                    continue;
                }

                // Not found with the provided parameter, let's go to the next asset
                continue;
            }

            // If no pattern was provided, we will look at potential usual filename patterns
            // for checksum files; such as:
            // - <asset_name>.<algorithm>
            // - <asset_name>.<algorithm>.txt
            // - <asset_name>.<algorithm>.sum
            // - <asset_name>_checksum.<algorithm>
            // - <asset_name>_checksum.txt
            // - <asset_name>-<algorithm>.txt
            // - <algorithm>.txt
            // - <algorithm>sum.txt

            let algorithms = if let Some(ref checksum_algorithm) = self.checksum_algorithm {
                vec![checksum_algorithm.as_str()]
            } else {
                vec!["md5", "sha1", "sha256", "sha384", "sha512"]
            };

            let regex_name = format!(
                r"(\b|_)({}|{})(\b|_)",
                regex_escape(&asset_without_ext),
                regex_escape(&asset.name),
            );
            let regex_algorithm = format!(
                r"(\b|_)({}|check)(sums?)?(\b|_)",
                algorithms.iter().map(|a| regex_escape(a)).join("|"),
            );

            if let (Ok(regex_name), Ok(regex_algorithm)) =
                (Regex::new(&regex_name), Regex::new(&regex_algorithm))
            {
                if let Some(checksum_asset) = search_assets
                    .iter()
                    .find(|a| regex_name.is_match(&a.name) && regex_algorithm.is_match(&a.name))
                {
                    asset.checksum_asset = Some(Box::new(checksum_asset.clone()));
                    continue;
                }
            }

            // Now try to find checksum files that are not named after the asset
            // but might contain checksums for multiple files

            let regex_checksums = format!(
                r"^(({0})(sums?)(\.txt)?|checksums?\.(txt|{0}))$",
                algorithms.iter().map(|a| regex_escape(a)).join("|"),
            );

            if let Ok(regex_checksums) = Regex::new(&regex_checksums) {
                if let Some(checksum_asset) = search_assets
                    .iter()
                    .find(|a| regex_checksums.is_match(&a.name))
                {
                    asset.checksum_asset = Some(Box::new(checksum_asset.clone()));
                    continue;
                }
            }

            // If we get here, we didn't find any checksum file for the current asset
        }

        // Return the assets
        matching_assets
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleases {
    pub releases: Vec<GithubReleaseVersion>,
    #[serde(default = "OffsetDateTime::now_utc", with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

impl GithubReleases {
    pub fn new() -> Self {
        Self {
            releases: vec![],
            fetched_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn add_json(&mut self, json: &str) -> Result<bool, String> {
        let releases: Vec<GithubReleaseVersion> = match serde_json::from_str(json) {
            Ok(releases) => releases,
            Err(err) => return Err(format!("failed to parse releases: {}", err)),
        };

        let existing_tag_names: HashSet<String> =
            self.releases.iter().map(|r| r.tag_name.clone()).collect();

        let mut dup = false;
        for release in releases {
            if existing_tag_names.contains(&release.tag_name) {
                dup = true;
                break;
            }

            self.releases.push(release);
        }

        self.fetched_at = OffsetDateTime::now_utc();

        Ok(!dup)
    }

    pub fn is_fresh(&self) -> bool {
        self.fetched_at >= omni_now()
    }

    pub fn is_stale(&self, ttl: u64) -> bool {
        let duration = time::Duration::seconds(ttl as i64);
        self.fetched_at + duration < OffsetDateTime::now_utc()
    }

    pub fn get(&self, selector: GithubReleasesSelector) -> Option<(String, GithubReleaseVersion)> {
        let mut matcher = VersionMatcher::new(&selector.version);
        matcher.prerelease(selector.prerelease);
        matcher.build(selector.build);
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
                if !selector.prerelease && release.prerelease {
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
                // and find its checksum file if available

                // Try and find all the checksum files for the current release
                // depending on the checksums configuration
                let assets = selector.assets_with_checksums(&release.assets);
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
            .max_by(|(version_a, _), (version_b, _)| VersionParser::compare(version_a, version_b))
    }
}

impl FromRow for GithubReleases {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        let releases_str: String = row.get("releases")?;
        let releases: Vec<GithubReleaseVersion> = serde_json::from_str(&releases_str)?;

        let fetched_at_str: String = row.get("fetched_at")?;
        let fetched_at: OffsetDateTime = OffsetDateTime::parse(&fetched_at_str, &Rfc3339)?;

        Ok(Self {
            releases,
            fetched_at,
        })
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_asset: Option<Box<GithubReleaseAsset>>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::cache::database::get_conn;
    use crate::internal::testutils::run_with_env;

    mod github_release_operation_cache {
        use super::*;
        use time::OffsetDateTime;

        #[test]
        fn test_add_and_get_releases() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();
                let repository = "test/repo";

                // Create test releases
                let releases = GithubReleases {
                    releases: vec![GithubReleaseVersion {
                        tag_name: "v1.0.0".to_string(),
                        name: "Release 1.0.0".to_string(),
                        draft: false,
                        prerelease: false,
                        assets: vec![],
                    }],
                    fetched_at: OffsetDateTime::now_utc(),
                };

                // Test adding releases
                assert!(cache
                    .add_releases(repository, &releases)
                    .expect("Failed to add releases"));

                // Test retrieving releases
                let retrieved = cache
                    .get_releases(repository)
                    .expect("Failed to get releases");
                assert_eq!(retrieved.releases.len(), 1);
                assert_eq!(retrieved.releases[0].tag_name, "v1.0.0");

                // Test retrieving non-existent repository
                let non_existent = cache.get_releases("non/existent");
                assert!(non_existent.is_none());
            });
        }

        #[test]
        fn test_add_and_list_installed() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();
                let repository = "test/repo";
                let version = "v1.0.0";

                // Test adding installed version
                assert!(cache
                    .add_installed(repository, version)
                    .expect("Failed to add installed version"));

                // Test listing installed versions
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), 1);
                assert_eq!(installed[0].repository, repository);
                assert_eq!(installed[0].version, version);

                // Test adding duplicate installed version
                assert!(cache
                    .add_installed(repository, version)
                    .expect("Failed to add duplicate installed version"));

                // Verify no duplicates in list
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), 1);
            });
        }

        #[test]
        fn test_add_required_by() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();
                let repository = "test/repo";
                let version = "v1.0.0";
                let env_version_id = "test-env-id";

                // Add environment version first for foreign key constraint
                let conn = get_conn();
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![env_version_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");

                // Try adding required_by without installed - should fail
                let result = cache.add_required_by(env_version_id, repository, version);
                assert!(result.is_err(), "Should fail without installed version");

                // Add installed version
                cache
                    .add_installed(repository, version)
                    .expect("Failed to add installed version");

                // Now add required_by - should succeed
                assert!(cache
                    .add_required_by(env_version_id, repository, version)
                    .expect("Failed to add required by relationship"));

                // Verify the relationship exists
                let required_exists: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM github_release_required_by WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3)",
                        params![repository, version, env_version_id],
                        |row| row.get(0),
                    )
                    .expect("Failed to query required by relationship");
                assert!(required_exists);
            });
        }

        #[test]
        fn test_multiple_required_by() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();
                let repository = "test/repo";
                let version = "v1.0.0";
                let env_version_ids = vec!["env-1", "env-2", "env-3"];

                // Add installed version first
                cache
                    .add_installed(repository, version)
                    .expect("Failed to add installed version");

                // Add environments
                let conn = get_conn();
                for env_id in &env_version_ids {
                    conn.execute(
                        include_str!("database/sql/up_environments_insert_env_version.sql"),
                        params![env_id, "{}", "[]", "[]", "{}", "hash"],
                    )
                    .expect("Failed to add environment version");
                }

                // Add requirements for each environment
                for env_id in &env_version_ids {
                    assert!(cache
                        .add_required_by(env_id, repository, version)
                        .expect("Failed to add requirement"));
                }

                // Verify requirements
                for env_id in &env_version_ids {
                    let required: bool = conn
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM github_release_required_by WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3)",
                            params![repository, version, env_id],
                            |row| row.get(0),
                        )
                        .expect("Failed to query requirement");
                    assert!(required, "Requirement for {} should exist", env_id);
                }

                // Verify installed version still exists
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), 1);
                assert_eq!(installed[0].repository, repository);
                assert_eq!(installed[0].version, version);
            });
        }

        #[test]
        fn test_cleanup() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();

                // Add two repositories
                let repo1 = "test/repo1";
                let repo2 = "test/repo2";
                let version = "v1.0.0";

                // Add installations
                cache
                    .add_installed(repo1, version)
                    .expect("Failed to add repo1 installation");
                cache
                    .add_installed(repo2, version)
                    .expect("Failed to add repo2 installation");

                let conn = get_conn();

                // Set repo1's last_required_at to old date (should be cleaned up)
                conn.execute(
                    "UPDATE github_release_installed SET last_required_at = '1970-01-01T00:00:00.000Z' WHERE repository = ?1",
                    params![repo1],
                )
                .expect("Failed to update last_required_at for repo1");

                // Keep repo2's last_required_at recent (should not be cleaned up)
                conn.execute(
                    "UPDATE github_release_installed SET last_required_at = datetime('now') WHERE repository = ?1",
                    params![repo2],
                )
                .expect("Failed to update last_required_at for repo2");

                // Run cleanup
                cache.cleanup().expect("Failed to cleanup");

                // Verify repo1 was cleaned up
                let repo1_exists: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM github_release_installed WHERE repository = ?1)",
                        params![repo1],
                        |row| row.get(0),
                    )
                    .expect("Failed to query repo1");
                assert!(
                    !repo1_exists,
                    "Old installation should have been cleaned up"
                );

                // Verify repo2 still exists
                let repo2_exists: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM github_release_installed WHERE repository = ?1)",
                        params![repo2],
                        |row| row.get(0),
                    )
                    .expect("Failed to query repo2");
                assert!(
                    repo2_exists,
                    "Recent installation should not have been cleaned up"
                );

                // Verify through list_installed
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), 1);
                assert_eq!(installed[0].repository, repo2);
            });
        }

        #[test]
        fn test_update_releases() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();
                let repository = "test/repo";

                // Create initial releases
                let releases1 = GithubReleases {
                    releases: vec![GithubReleaseVersion {
                        tag_name: "v1.0.0".to_string(),
                        name: "Release 1.0.0".to_string(),
                        draft: false,
                        prerelease: false,
                        assets: vec![],
                    }],
                    fetched_at: OffsetDateTime::now_utc(),
                };

                // Add initial releases
                assert!(cache
                    .add_releases(repository, &releases1)
                    .expect("Failed to add initial releases"));

                // Create updated releases
                let releases2 = GithubReleases {
                    releases: vec![
                        GithubReleaseVersion {
                            tag_name: "v1.0.0".to_string(),
                            name: "Release 1.0.0".to_string(),
                            draft: false,
                            prerelease: false,
                            assets: vec![],
                        },
                        GithubReleaseVersion {
                            tag_name: "v1.1.0".to_string(),
                            name: "Release 1.1.0".to_string(),
                            draft: false,
                            prerelease: false,
                            assets: vec![],
                        },
                    ],
                    fetched_at: OffsetDateTime::now_utc(),
                };

                // Update releases
                assert!(cache
                    .add_releases(repository, &releases2)
                    .expect("Failed to update releases"));

                // Verify updated releases
                let retrieved = cache
                    .get_releases(repository)
                    .expect("Failed to get releases");
                assert_eq!(retrieved.releases.len(), 2);
                assert!(retrieved.releases.iter().any(|r| r.tag_name == "v1.1.0"));
            });
        }

        #[test]
        fn test_multiple_versions_same_repository() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();
                let repository = "test/repo";
                let versions = vec!["v1.0.0", "v1.1.0", "v2.0.0"];

                // Add multiple versions
                for version in &versions {
                    assert!(cache
                        .add_installed(repository, version)
                        .expect("Failed to add installed version"));
                }

                // Verify all versions are listed
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), versions.len());

                for version in versions {
                    assert!(
                        installed
                            .iter()
                            .any(|i| i.repository == repository && i.version == version),
                        "Version {} should be in installed list",
                        version
                    );
                }
            });
        }

        #[test]
        fn test_required_by_multiple_versions() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();
                let repository = "test/repo";
                let versions = vec!["v1.0.0", "v1.1.0"];
                let env_id = "test-env";

                // Add environment
                let conn = get_conn();
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![env_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");

                // Add installations and requirements
                for version in &versions {
                    // Add installation
                    assert!(cache
                        .add_installed(repository, version)
                        .expect("Failed to add installed version"));

                    // Add requirement
                    assert!(cache
                        .add_required_by(env_id, repository, version)
                        .expect("Failed to add requirement"));
                }

                // Verify all requirements exist
                for version in versions {
                    let required: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM github_release_required_by WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3)",
                    params![repository, version, env_id],
                    |row| row.get(0),
                )
                .expect("Failed to query requirement");
                    assert!(required, "Requirement for version {} should exist", version);
                }
            });
        }

        #[test]
        fn test_cleanup_cascade() {
            run_with_env(&[], || {
                let cache = GithubReleaseOperationCache::get();

                struct TestCase {
                    repository: &'static str,
                    version: &'static str,
                    env_id: &'static str,
                    remove_env: bool,
                    remove_install: bool,
                }

                let tests = vec![
                    TestCase {
                        repository: "test/repo1",
                        version: "v1.0.0",
                        env_id: "test-env1",
                        remove_env: false,
                        remove_install: true,
                    },
                    TestCase {
                        repository: "test/repo2",
                        version: "v1.0.0",
                        env_id: "test-env2",
                        remove_env: true,
                        remove_install: false,
                    },
                ];

                let conn = get_conn();

                for test in &tests {
                    // Add environment
                    conn.execute(
                        include_str!("database/sql/up_environments_insert_env_version.sql"),
                        params![test.env_id, "{}", "[]", "[]", "{}", "hash"],
                    )
                    .expect("Failed to add environment version");

                    // Add installation
                    cache
                        .add_installed(test.repository, test.version)
                        .unwrap_or_else(|_| {
                            panic!("Failed to add installed version for {}", test.repository)
                        });

                    // Add requirement
                    cache
                        .add_required_by(test.env_id, test.repository, test.version)
                        .unwrap_or_else(|_| {
                            panic!("Failed to add requirement for {}", test.repository)
                        });

                    // Check that the requirement exists
                    let required: bool = conn
                        .query_row(
                            concat!(
                                "SELECT EXISTS(",
                                "  SELECT 1 FROM github_release_required_by ",
                                "  WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3",
                                ")",
                            ),
                            params![test.repository, test.version, test.env_id],
                            |row| row.get(0),
                        )
                        .expect("Failed to query requirement");
                    assert!(required, "Requirement for {} should exist", test.repository);

                    if test.remove_env {
                        // Remove environment
                        conn.execute(
                            "DELETE FROM env_versions WHERE env_version_id = ?1",
                            params![test.env_id],
                        )
                        .expect("Failed to remove environment");
                    }

                    if test.remove_install {
                        // Remove installation
                        conn.execute(
                            "DELETE FROM github_release_installed WHERE repository = ?1 AND version = ?2",
                            params![test.repository, test.version],
                        ).expect("Failed to remove installation");
                    }

                    // Verify that the requirement has been cleaned up
                    let requirement_exists: bool = conn
                        .query_row(
                            concat!(
                                "SELECT EXISTS(",
                                "  SELECT 1 FROM github_release_required_by ",
                                "  WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3",
                                ")",
                            ),
                            params![test.repository, test.version, test.env_id],
                            |row| row.get(0),
                        )
                        .expect("Failed to query requirement");
                    assert!(
                        !requirement_exists,
                        "Requirement should be cleaned up via cascade"
                    );
                }
            });
        }
    }
}
