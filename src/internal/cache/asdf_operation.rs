use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_asdf_operation_cache;
use crate::internal::cache::loaders::set_asdf_operation_cache;
use crate::internal::cache::offsetdatetime_hashmap;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;
use crate::internal::config::global_config;
use crate::internal::env::now as omni_now;

const ASDF_OPERATION_CACHE_NAME: &str = "asdf_operation";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<AsdfInstalled>,
    #[serde(
        default = "AsdfOperationUpdateCache::new",
        skip_serializing_if = "AsdfOperationUpdateCache::is_empty"
    )]
    pub update_cache: AsdfOperationUpdateCache,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

impl AsdfOperationCache {
    pub fn updated(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
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
        self.update_cache.should_update_asdf(Duration::from_secs(
            global_config().cache.asdf.update_expire,
        ))
    }

    pub fn should_update_asdf_plugin(&self, plugin: &str) -> bool {
        self.update_cache.should_update_asdf_plugin(
            plugin,
            Duration::from_secs(global_config().cache.asdf.plugin_update_expire),
        )
    }

    pub fn get_asdf_plugin_versions(&self, plugin: &str) -> Option<Vec<String>> {
        self.update_cache.get_asdf_plugin_versions(
            plugin,
            Duration::from_secs(global_config().cache.asdf.plugin_versions_expire),
        )
    }

    pub fn add_installed(
        &mut self,
        workdir_id: &str,
        tool: &str,
        version: &str,
        tool_real_name: Option<&str>,
    ) -> bool {
        let inserted = if let Some(install) = self
            .installed
            .iter_mut()
            .find(|i| i.tool == tool && i.version == version)
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
            let install = AsdfInstalled {
                tool: tool.to_string(),
                tool_real_name: tool_real_name.map(|s| s.to_string()),
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

impl Empty for AsdfOperationCache {
    fn is_empty(&self) -> bool {
        self.installed.is_empty() && self.update_cache.is_empty()
    }
}

impl CacheObject for AsdfOperationCache {
    fn new_empty() -> Self {
        Self {
            installed: Vec::new(),
            update_cache: AsdfOperationUpdateCache::new(),
            updated_at: utils::origin_of_time(),
        }
    }

    fn get() -> Self {
        get_asdf_operation_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(ASDF_OPERATION_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(
            ASDF_OPERATION_CACHE_NAME,
            processing_fn,
            set_asdf_operation_cache,
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfInstalled {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_real_name: Option<String>,
    pub version: String,
    #[serde(default = "BTreeSet::new", skip_serializing_if = "BTreeSet::is_empty")]
    pub required_by: BTreeSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

impl AsdfInstalled {
    pub fn stale(&self) -> bool {
        self.last_required_at < omni_now()
    }

    pub fn removable(&self) -> bool {
        if !self.required_by.is_empty() {
            return false;
        }

        let config = global_config();
        let grace_period = config.cache.asdf.cleanup_after;
        let grace_period = time::Duration::seconds(grace_period as i64);

        (self.last_required_at + grace_period) < omni_now()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfOperationUpdateCache {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub asdf_updated_at: OffsetDateTime,
    #[serde(
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty",
        with = "offsetdatetime_hashmap"
    )]
    pub plugins_updated_at: HashMap<String, OffsetDateTime>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub plugins_versions: HashMap<String, AsdfOperationUpdateCachePluginVersions>,
}

impl AsdfOperationUpdateCache {
    pub fn new() -> Self {
        Self {
            asdf_updated_at: utils::origin_of_time(),
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
            && self.asdf_updated_at == utils::origin_of_time()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfOperationUpdateCachePluginVersions {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
}

impl AsdfOperationUpdateCachePluginVersions {
    pub fn new(versions: Vec<String>) -> Self {
        Self {
            updated_at: OffsetDateTime::now_utc(),
            versions,
        }
    }
}

impl Empty for AsdfOperationUpdateCachePluginVersions {
    fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }
}
