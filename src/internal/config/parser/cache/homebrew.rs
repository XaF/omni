use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewCacheConfig {
    pub update_expire: u64,
    pub install_update_expire: u64,
    pub install_check_expire: u64,
}

impl Default for HomebrewCacheConfig {
    fn default() -> Self {
        Self {
            update_expire: Self::DEFAULT_UPDATE_EXPIRE,
            install_update_expire: Self::DEFAULT_INSTALL_UPDATE_EXPIRE,
            install_check_expire: Self::DEFAULT_INSTALL_CHECK_EXPIRE,
        }
    }
}

impl HomebrewCacheConfig {
    const DEFAULT_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_INSTALL_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_INSTALL_CHECK_EXPIRE: u64 = 43200; // 12 hours

    pub fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let update_expire = parse_duration_or_default(
            config_value.get("update_expire").as_ref(),
            Self::DEFAULT_UPDATE_EXPIRE,
        );

        let install_update_expire = parse_duration_or_default(
            config_value.get("install_update_expire").as_ref(),
            Self::DEFAULT_INSTALL_UPDATE_EXPIRE,
        );

        let install_check_expire = parse_duration_or_default(
            config_value.get("install_check_expire").as_ref(),
            Self::DEFAULT_INSTALL_CHECK_EXPIRE,
        );

        Self {
            update_expire,
            install_update_expire,
            install_check_expire,
        }
    }
}
