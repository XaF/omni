use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorKind;
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
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let update_expire = parse_duration_or_default(
            config_value.get("update_expire").as_ref(),
            Self::DEFAULT_UPDATE_EXPIRE,
            &format!("{}.update_expire", error_key),
            on_error,
        );

        let tap_update_expire = parse_duration_or_default(
            config_value.get("tap_update_expire").as_ref(),
            Self::DEFAULT_TAP_UPDATE_EXPIRE,
            &format!("{}.tap_update_expire", error_key),
            on_error,
        );

        let install_update_expire = parse_duration_or_default(
            config_value.get("install_update_expire").as_ref(),
            Self::DEFAULT_INSTALL_UPDATE_EXPIRE,
            &format!("{}.install_update_expire", error_key),
            on_error,
        );

        let install_check_expire = parse_duration_or_default(
            config_value.get("install_check_expire").as_ref(),
            Self::DEFAULT_INSTALL_CHECK_EXPIRE,
            &format!("{}.install_check_expire", error_key),
            on_error,
        );

        let cleanup_after = parse_duration_or_default(
            config_value.get("cleanup_after").as_ref(),
            Self::DEFAULT_CLEANUP_AFTER,
            &format!("{}.cleanup_after", error_key),
            on_error,
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
