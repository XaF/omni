use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoInstallCacheConfig {
    pub versions_expire: u64,
    pub versions_retention: u64,
    pub cleanup_after: u64,
}

impl Default for GoInstallCacheConfig {
    fn default() -> Self {
        Self {
            versions_expire: Self::DEFAULT_VERSIONS_EXPIRE,
            versions_retention: Self::DEFAULT_VERSIONS_RETENTION,
            cleanup_after: Self::DEFAULT_CLEANUP_AFTER,
        }
    }
}

impl GoInstallCacheConfig {
    const DEFAULT_VERSIONS_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_VERSIONS_RETENTION: u64 = 7776000; // 90 days
    const DEFAULT_CLEANUP_AFTER: u64 = 604800; // 1 week

    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let versions_expire = parse_duration_or_default(
            config_value.get("versions_expire").as_ref(),
            Self::DEFAULT_VERSIONS_EXPIRE,
            &format!("{}.versions_expire", error_key),
            errors,
        );

        let versions_retention = parse_duration_or_default(
            config_value.get("versions_retention").as_ref(),
            Self::DEFAULT_VERSIONS_RETENTION,
            &format!("{}.versions_retention", error_key),
            errors,
        );

        let cleanup_after = parse_duration_or_default(
            config_value.get("cleanup_after").as_ref(),
            Self::DEFAULT_CLEANUP_AFTER,
            &format!("{}.cleanup_after", error_key),
            errors,
        );

        Self {
            versions_expire,
            versions_retention,
            cleanup_after,
        }
    }
}
