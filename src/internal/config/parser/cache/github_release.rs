use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubReleaseCacheConfig {
    pub versions_expire: u64,
    pub versions_retention: u64,
    pub cleanup_after: u64,
}

impl Default for GithubReleaseCacheConfig {
    fn default() -> Self {
        Self {
            versions_expire: Self::DEFAULT_VERSIONS_EXPIRE,
            versions_retention: Self::DEFAULT_VERSIONS_RETENTION,
            cleanup_after: Self::DEFAULT_CLEANUP_AFTER,
        }
    }
}

impl GithubReleaseCacheConfig {
    const DEFAULT_VERSIONS_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_VERSIONS_RETENTION: u64 = 7776000; // 90 days
    const DEFAULT_CLEANUP_AFTER: u64 = 604800; // 1 week

    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let versions_expire = parse_duration_or_default(
            config_value.get("versions_expire").as_ref(),
            Self::DEFAULT_VERSIONS_EXPIRE,
            &error_handler.with_key("versions_expire"),
        );

        let versions_retention = parse_duration_or_default(
            config_value.get("versions_retention").as_ref(),
            Self::DEFAULT_VERSIONS_RETENTION,
            &error_handler.with_key("versions_retention"),
        );

        let cleanup_after = parse_duration_or_default(
            config_value.get("cleanup_after").as_ref(),
            Self::DEFAULT_CLEANUP_AFTER,
            &error_handler.with_key("cleanup_after"),
        );

        Self {
            versions_expire,
            versions_retention,
            cleanup_after,
        }
    }
}
