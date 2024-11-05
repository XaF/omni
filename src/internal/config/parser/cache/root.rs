use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::cache::AsdfCacheConfig;
use crate::internal::config::parser::cache::GithubReleaseCacheConfig;
use crate::internal::config::parser::cache::HomebrewCacheConfig;
use crate::internal::config::parser::cache::UpEnvironmentCacheConfig;
use crate::internal::config::ConfigValue;
use crate::internal::env::cache_home;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheConfig {
    pub path: String,
    pub environment: UpEnvironmentCacheConfig,
    pub asdf: AsdfCacheConfig,
    pub github_release: GithubReleaseCacheConfig,
    pub homebrew: HomebrewCacheConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            path: cache_home(),
            environment: UpEnvironmentCacheConfig::default(),
            asdf: AsdfCacheConfig::default(),
            github_release: GithubReleaseCacheConfig::default(),
            homebrew: HomebrewCacheConfig::default(),
        }
    }
}

impl CacheConfig {
    pub fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let path = match config_value.get("path") {
            Some(value) => value.as_str().unwrap().to_string(),
            None => cache_home(),
        };

        let environment =
            UpEnvironmentCacheConfig::from_config_value(config_value.get("environment"));
        let asdf = AsdfCacheConfig::from_config_value(config_value.get("asdf"));
        let github_release =
            GithubReleaseCacheConfig::from_config_value(config_value.get("github_release"));
        let homebrew = HomebrewCacheConfig::from_config_value(config_value.get("homebrew"));

        Self {
            path,
            environment,
            asdf,
            github_release,
            homebrew,
        }
    }
}
