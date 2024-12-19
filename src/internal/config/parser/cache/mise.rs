use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MiseCacheConfig {
    pub update_expire: u64,
    pub plugin_update_expire: u64,
    pub plugin_versions_expire: u64,
    pub plugin_versions_retention: u64,
    pub cleanup_after: u64,
}

impl Default for MiseCacheConfig {
    fn default() -> Self {
        Self {
            update_expire: Self::DEFAULT_UPDATE_EXPIRE,
            plugin_update_expire: Self::DEFAULT_PLUGIN_UPDATE_EXPIRE,
            plugin_versions_expire: Self::DEFAULT_PLUGIN_VERSIONS_EXPIRE,
            plugin_versions_retention: Self::DEFAULT_PLUGIN_VERSIONS_RETENTION,
            cleanup_after: Self::DEFAULT_CLEANUP_AFTER,
        }
    }
}

impl MiseCacheConfig {
    const DEFAULT_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_PLUGIN_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_PLUGIN_VERSIONS_EXPIRE: u64 = 3600; // 1 hour
    const DEFAULT_PLUGIN_VERSIONS_RETENTION: u64 = 7776000; // 90 days
    const DEFAULT_CLEANUP_AFTER: u64 = 604800; // 1 week

    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let update_expire = parse_duration_or_default(
            config_value.get("update_expire").as_ref(),
            Self::DEFAULT_UPDATE_EXPIRE,
            "cache.mise.update_expire",
            errors,
        );

        let plugin_update_expire = parse_duration_or_default(
            config_value.get("plugin_update_expire").as_ref(),
            Self::DEFAULT_PLUGIN_UPDATE_EXPIRE,
            "cache.mise.plugin_update_expire",
            errors,
        );

        let plugin_versions_expire = parse_duration_or_default(
            config_value.get("plugin_versions_expire").as_ref(),
            Self::DEFAULT_PLUGIN_VERSIONS_EXPIRE,
            "cache.mise.plugin_versions_expire",
            errors,
        );

        let plugin_versions_retention = parse_duration_or_default(
            config_value.get("plugin_versions_retention").as_ref(),
            Self::DEFAULT_PLUGIN_VERSIONS_RETENTION,
            "cache.mise.plugin_versions_retention",
            errors,
        );

        let cleanup_after = parse_duration_or_default(
            config_value.get("cleanup_after").as_ref(),
            Self::DEFAULT_CLEANUP_AFTER,
            "cache.mise.cleanup_after",
            errors,
        );

        Self {
            update_expire,
            plugin_update_expire,
            plugin_versions_expire,
            plugin_versions_retention,
            cleanup_after,
        }
    }
}
