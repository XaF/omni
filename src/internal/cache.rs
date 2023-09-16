use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use fs4::FileExt;
use lazy_static::lazy_static;
use serde::Deserialize;
use serde::Serialize;
use serde_json;
use time::OffsetDateTime;

use crate::internal::config;

lazy_static! {
    static ref CACHE: Mutex<Cache> = Mutex::new(Cache::new());
}

pub fn get_cache() -> Cache {
    let cache = CACHE.lock().unwrap();
    cache.clone()
}

fn set_cache(cache_set: Cache) {
    let mut cache = CACHE.lock().unwrap();
    *cache = cache_set;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cache {
    #[serde(skip_serializing_if = "entry_empty_option")]
    pub asdf_operation: Option<AsdfOperation>,
    #[serde(skip_serializing_if = "entry_empty_option")]
    pub homebrew_operation: Option<HomebrewOperation>,
    #[serde(skip_serializing_if = "entry_expired_option")]
    pub omni_path_updates: Option<OmniPathUpdates>,
    #[serde(skip_serializing_if = "entry_empty_option")]
    pub trusted_repositories: Option<TrustedRepositories>,
    #[serde(skip_serializing_if = "entry_empty_option")]
    pub up_environments: Option<UpEnvironments>,
}

impl Cache {
    pub fn new() -> Self {
        if let Ok(cache) = Self::shared() {
            return cache;
        }

        Self::new_empty()
    }

    pub fn new_empty() -> Self {
        Self {
            asdf_operation: None,
            homebrew_operation: None,
            omni_path_updates: None,
            trusted_repositories: None,
            up_environments: None,
        }
    }

    pub fn omni_path_updated() -> bool {
        if let Some(omni_path_updates) = get_cache().omni_path_updates {
            return omni_path_updates.updated();
        }
        false
    }

    pub fn shared() -> io::Result<Self> {
        let file = File::open(config(".").cache.path.clone())?;
        let _file_lock = file.lock_shared();
        let cache: Cache = serde_json::from_reader(file)?;
        Ok(cache)
    }

    pub fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        // Check if the directory of the cache file exists, otherwise create it recursively
        let cache_path = PathBuf::from(config(".").cache.path.clone());
        if let Some(parent) = cache_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Open the cache file
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(config(".").cache.path.clone())?;

        // Take the exclusive lock on the file, it will be release when `_file_lock` goes out of scope
        let _file_lock = file.lock_exclusive();

        // Read the content of the file, and parse it as JSON
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let load_cache: Result<Cache, _> = serde_json::from_str(&content);
        let mut cache = if let Ok(_) = load_cache {
            load_cache.unwrap().clone()
        } else {
            Cache::new_empty()
        };

        // Call the provided closure, passing the cache reference, and check if there is a request
        // to update the cache with the new data
        if processing_fn(&mut cache) {
            let serialized = serde_json::to_string(&cache).unwrap();

            // Replace entirely the content of the file with the new JSON
            file.set_len(0)?;
            file.seek(io::SeekFrom::Start(0))?;
            file.write_all(serialized.as_bytes())?;

            // Update the CACHE global variable with the new data
            set_cache(cache.clone());
        }

        // Return the cache as modified by the closure, no matter if the file was updated or not
        Ok(cache)
    }

    pub fn set_asdf_operation_installed(&mut self, installed: Vec<AsdfInstalled>) {
        if let None = self.asdf_operation {
            self.asdf_operation = Some(AsdfOperation::new());
        }
        self.asdf_operation
            .as_mut()
            .unwrap()
            .set_installed(installed);
    }

    pub fn should_update_asdf(&self) -> bool {
        if let Some(asdf_operation) = &self.asdf_operation {
            return asdf_operation.should_update_asdf();
        }
        true
    }

    pub fn should_update_asdf_plugin(&self, plugin: &str) -> bool {
        if let Some(asdf_operation) = &self.asdf_operation {
            return asdf_operation.should_update_asdf_plugin(plugin);
        }
        true
    }

    pub fn get_asdf_plugin_versions(&self, plugin: &str) -> Option<Vec<String>> {
        if let Some(asdf_operation) = &self.asdf_operation {
            return asdf_operation.get_asdf_plugin_versions(plugin);
        }
        None
    }

    pub fn updated_asdf(&mut self) {
        if let None = self.asdf_operation {
            self.asdf_operation = Some(AsdfOperation::new());
        }
        self.asdf_operation.as_mut().unwrap().updated_asdf();
    }

    pub fn updated_asdf_plugin(&mut self, plugin: &str) {
        if let None = self.asdf_operation {
            self.asdf_operation = Some(AsdfOperation::new());
        }
        self.asdf_operation
            .as_mut()
            .unwrap()
            .updated_asdf_plugin(plugin);
    }

    pub fn set_asdf_plugin_versions(&mut self, plugin: &str, versions: Vec<String>) {
        if let None = self.asdf_operation {
            self.asdf_operation = Some(AsdfOperation::new());
        }
        self.asdf_operation
            .as_mut()
            .unwrap()
            .set_asdf_plugin_versions(plugin, versions);
    }
}

trait Expires {
    fn expired(&self) -> bool;
}

trait Empty {
    fn is_empty(&self) -> bool;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperation {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<HomebrewInstalled>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub tapped: Vec<HomebrewTapped>,
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl Empty for HomebrewOperation {
    fn is_empty(&self) -> bool {
        self.installed.is_empty() && self.tapped.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewInstalled {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default = "set_false", skip_serializing_if = "is_false")]
    pub cask: bool,
    #[serde(default = "set_false", skip_serializing_if = "is_false")]
    pub installed: bool,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub required_by: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewTapped {
    pub name: String,
    #[serde(default = "set_false", skip_serializing_if = "is_false")]
    pub tapped: bool,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub required_by: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfOperation {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<AsdfInstalled>,
    #[serde(
        default = "AsdfOperationUpdateCache::new",
        skip_serializing_if = "AsdfOperationUpdateCache::is_empty"
    )]
    pub update_cache: AsdfOperationUpdateCache,
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl AsdfOperation {
    pub fn new() -> Self {
        Self {
            installed: Vec::new(),
            update_cache: AsdfOperationUpdateCache::new(),
            updated_at: set_origin_of_time(),
        }
    }

    pub fn updated(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn set_installed(&mut self, installed: Vec<AsdfInstalled>) {
        self.installed = installed;
        self.updated();
    }

    pub fn updated_asdf(&mut self) {
        self.update_cache.updated_asdf();
        self.updated();
    }

    pub fn updated_asdf_plugin(&mut self, plugin: &str) {
        self.update_cache.updated_asdf_plugin(plugin);
        self.updated();
    }

    pub fn set_asdf_plugin_versions(&mut self, plugin: &str, versions: Vec<String>) {
        self.update_cache.set_asdf_plugin_versions(plugin, versions);
        self.updated();
    }

    pub fn should_update_asdf(&self) -> bool {
        self.update_cache
            .should_update_asdf(Duration::from_secs(86400))
    }

    pub fn should_update_asdf_plugin(&self, plugin: &str) -> bool {
        self.update_cache
            .should_update_asdf_plugin(plugin, Duration::from_secs(86400))
    }

    pub fn get_asdf_plugin_versions(&self, plugin: &str) -> Option<Vec<String>> {
        self.update_cache
            .get_asdf_plugin_versions(plugin, Duration::from_secs(3600))
    }
}

impl Empty for AsdfOperation {
    fn is_empty(&self) -> bool {
        self.installed.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfInstalled {
    pub tool: String,
    pub version: String,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub required_by: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfOperationUpdateCache {
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub asdf_updated_at: OffsetDateTime,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub plugins_updated_at: HashMap<String, OffsetDateTime>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub plugins_versions: HashMap<String, AsdfOperationUpdateCachePluginVersions>,
}

impl AsdfOperationUpdateCache {
    pub fn new() -> Self {
        Self {
            asdf_updated_at: set_origin_of_time(),
            plugins_updated_at: HashMap::new(),
            plugins_versions: HashMap::new(),
        }
    }

    pub fn updated_asdf(&mut self) {
        self.asdf_updated_at = OffsetDateTime::now_utc();
    }

    pub fn updated_asdf_plugin(&mut self, plugin: &str) {
        self.plugins_updated_at
            .insert(plugin.to_string(), OffsetDateTime::now_utc());
    }

    pub fn set_asdf_plugin_versions(&mut self, plugin: &str, versions: Vec<String>) {
        self.plugins_versions.insert(
            plugin.to_string(),
            AsdfOperationUpdateCachePluginVersions::new(versions),
        );
    }

    pub fn should_update_asdf(&self, expire_after: Duration) -> bool {
        (self.asdf_updated_at + expire_after) < OffsetDateTime::now_utc()
    }

    pub fn should_update_asdf_plugin(&self, plugin: &str, expire_after: Duration) -> bool {
        if let Some(plugin_updated_at) = self.plugins_updated_at.get(plugin) {
            (*plugin_updated_at + expire_after) < OffsetDateTime::now_utc()
        } else {
            true
        }
    }

    pub fn get_asdf_plugin_versions(
        &self,
        plugin: &str,
        expire_after: Duration,
    ) -> Option<Vec<String>> {
        if let Some(plugin_versions) = self.plugins_versions.get(plugin) {
            if (plugin_versions.updated_at + expire_after) < OffsetDateTime::now_utc() {
                return None;
            }
            Some(plugin_versions.versions.clone())
        } else {
            None
        }
    }
}

impl Empty for AsdfOperationUpdateCache {
    fn is_empty(&self) -> bool {
        self.plugins_versions.is_empty()
            && self.plugins_updated_at.is_empty()
            && self.asdf_updated_at == set_origin_of_time()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfOperationUpdateCachePluginVersions {
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
}

impl AsdfOperationUpdateCachePluginVersions {
    pub fn new(versions: Vec<String>) -> Self {
        Self {
            updated_at: OffsetDateTime::now_utc(),
            versions: versions,
        }
    }
}

impl Empty for AsdfOperationUpdateCachePluginVersions {
    fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustedRepositories {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<String>,
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl TrustedRepositories {
    pub fn new(repositories: Vec<String>) -> Self {
        Self {
            repositories: repositories,
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    // pub fn contains(&self, repository: &str) -> bool {
    // self.repositories.contains(&repository.to_string())
    // }

    // pub fn add(&mut self, repository: &str) {
    // if !self.contains(repository) {
    // self.repositories.push(repository.to_string());
    // }
    // }
}

impl Empty for TrustedRepositories {
    fn is_empty(&self) -> bool {
        self.repositories.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniPathUpdates {
    #[serde(default = "set_false", skip_serializing_if = "is_false")]
    pub updated: bool,
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

impl OmniPathUpdates {
    pub fn new() -> Self {
        Self {
            updated: true,
            updated_at: OffsetDateTime::now_utc(),
            expires_at: OffsetDateTime::now_utc()
                + Duration::from_secs(config(".").path_repo_updates.interval),
        }
    }

    pub fn updated(&self) -> bool {
        !self.expired() && self.updated
    }

    pub fn update(&mut self) {
        self.updated = true;
        self.updated_at = OffsetDateTime::now_utc();
        self.expires_at =
            self.updated_at + Duration::from_secs(config(".").path_repo_updates.interval);
    }
}

impl Expires for OmniPathUpdates {
    fn expired(&self) -> bool {
        self.expires_at < OffsetDateTime::now_utc()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironments {
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, UpEnvironment>,
    #[serde(default = "set_origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl Empty for UpEnvironments {
    fn is_empty(&self) -> bool {
        self.env.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironment {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<UpVersion>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub env_vars: HashMap<String, String>,
}

impl UpEnvironment {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            env_vars: HashMap::new(),
        }
    }

    pub fn versions_for_dir(&self, dir: &str) -> Vec<UpVersion> {
        let mut versions: BTreeMap<String, UpVersion> = BTreeMap::new();

        for version in self.versions.iter() {
            // Check if that version applies to the requested dir
            if version.dir != ""
                && dir != version.dir
                && !dir.starts_with(format!("{}/", version.dir).as_str())
            {
                continue;
            }

            // If there is already a version, check if the current one's dir is more specific
            if let Some(existing_version) = versions.get(&version.tool) {
                if existing_version.dir.len() > version.dir.len() {
                    continue;
                }
            }

            versions.insert(version.tool.clone(), version.clone());
        }

        versions.values().cloned().collect()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpVersion {
    pub tool: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
}

fn set_false() -> bool {
    false
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn set_origin_of_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH
}

fn entry_expired_option<T: Expires>(entry: &Option<T>) -> bool {
    if let Some(entry) = entry {
        entry.expired()
    } else {
        true
    }
}

fn entry_empty_option<T: Empty>(entry: &Option<T>) -> bool {
    if let Some(entry) = entry {
        entry.is_empty()
    } else {
        true
    }
}
