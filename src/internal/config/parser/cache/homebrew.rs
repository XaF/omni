use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewCacheConfig {
    pub update_expire: u64,
    pub tap_update_expire: u64,
    pub install_update_expire: u64,
    pub install_check_expire: u64,
    pub cleanup_after: u64,
}

impl Default for HomebrewCacheConfig {
    fn default() -> Self {
        Self {
            update_expire: Self::DEFAULT_UPDATE_EXPIRE,
            tap_update_expire: Self::DEFAULT_TAP_UPDATE_EXPIRE,
            install_update_expire: Self::DEFAULT_INSTALL_UPDATE_EXPIRE,
            install_check_expire: Self::DEFAULT_INSTALL_CHECK_EXPIRE,
            cleanup_after: Self::DEFAULT_CLEANUP_AFTER,
        }
    }
}

impl HomebrewCacheConfig {
    const DEFAULT_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_TAP_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_INSTALL_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_INSTALL_CHECK_EXPIRE: u64 = 43200; // 12 hours
    const DEFAULT_CLEANUP_AFTER: u64 = 604800; // 1 week

    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let update_expire = parse_duration_or_default(
            config_value.get("update_expire").as_ref(),
            Self::DEFAULT_UPDATE_EXPIRE,
            &error_handler.with_key("update_expire"),
        );

        let tap_update_expire = parse_duration_or_default(
            config_value.get("tap_update_expire").as_ref(),
            Self::DEFAULT_TAP_UPDATE_EXPIRE,
            &error_handler.with_key("tap_update_expire"),
        );

        let install_update_expire = parse_duration_or_default(
            config_value.get("install_update_expire").as_ref(),
            Self::DEFAULT_INSTALL_UPDATE_EXPIRE,
            &error_handler.with_key("install_update_expire"),
        );

        let install_check_expire = parse_duration_or_default(
            config_value.get("install_check_expire").as_ref(),
            Self::DEFAULT_INSTALL_CHECK_EXPIRE,
            &error_handler.with_key("install_check_expire"),
        );

        let cleanup_after = parse_duration_or_default(
            config_value.get("cleanup_after").as_ref(),
            Self::DEFAULT_CLEANUP_AFTER,
            &error_handler.with_key("cleanup_after"),
        );

        Self {
            update_expire,
            tap_update_expire,
            install_update_expire,
            install_check_expire,
            cleanup_after,
        }
    }
}
