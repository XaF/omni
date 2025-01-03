use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::cache::CargoInstallCacheConfig;
use crate::internal::config::parser::cache::GithubReleaseCacheConfig;
use crate::internal::config::parser::cache::GoInstallCacheConfig;
use crate::internal::config::parser::cache::HomebrewCacheConfig;
use crate::internal::config::parser::cache::MiseCacheConfig;
use crate::internal::config::parser::cache::UpEnvironmentCacheConfig;
use crate::internal::config::ConfigValue;
use crate::internal::env::cache_home;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheConfig {
    pub path: String,
    pub environment: UpEnvironmentCacheConfig,
    pub github_release: GithubReleaseCacheConfig,
    pub cargo_install: CargoInstallCacheConfig,
    pub go_install: GoInstallCacheConfig,
    pub homebrew: HomebrewCacheConfig,
    pub mise: MiseCacheConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            path: cache_home(),
            environment: UpEnvironmentCacheConfig::default(),
            github_release: GithubReleaseCacheConfig::default(),
            cargo_install: CargoInstallCacheConfig::default(),
            go_install: GoInstallCacheConfig::default(),
            homebrew: HomebrewCacheConfig::default(),
            mise: MiseCacheConfig::default(),
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
        let github_release =
            GithubReleaseCacheConfig::from_config_value(config_value.get("github_release"));
        let cargo_install =
            CargoInstallCacheConfig::from_config_value(config_value.get("cargo_install"));
        let go_install = GoInstallCacheConfig::from_config_value(config_value.get("go_install"));
        let homebrew = HomebrewCacheConfig::from_config_value(config_value.get("homebrew"));
        let mise = MiseCacheConfig::from_config_value(config_value.get("mise"));

        Self {
            path,
            environment,
            github_release,
            cargo_install,
            go_install,
            homebrew,
            mise,
        }
    }
}
