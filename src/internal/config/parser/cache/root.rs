use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::cache::AsdfCacheConfig;
use crate::internal::config::parser::cache::HomebrewCacheConfig;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;
use crate::internal::env::cache_home;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheConfig {
    pub path: String,
    pub asdf: AsdfCacheConfig,
    pub github_release_versions_expire: u64,
    pub homebrew: HomebrewCacheConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            path: cache_home(),
            asdf: AsdfCacheConfig::default(),
            github_release_versions_expire: Self::DEFAULT_GITHUB_RELEASE_VERSIONS_EXPIRE,
            homebrew: HomebrewCacheConfig::default(),
        }
    }
}

impl CacheConfig {
    const DEFAULT_GITHUB_RELEASE_VERSIONS_EXPIRE: u64 = 86400; // 1 day

    pub fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let path = match config_value.get("path") {
            Some(value) => value.as_str().unwrap().to_string(),
            None => cache_home(),
        };

        let asdf = AsdfCacheConfig::from_config_value(config_value.get("asdf"));

        let github_release_versions_expire = parse_duration_or_default(
            config_value.get("github_release_versions_expire").as_ref(),
            Self::DEFAULT_GITHUB_RELEASE_VERSIONS_EXPIRE,
        );

        let homebrew = HomebrewCacheConfig::from_config_value(config_value.get("homebrew"));

        Self {
            path,
            asdf,
            github_release_versions_expire,
            homebrew,
        }
    }
}
