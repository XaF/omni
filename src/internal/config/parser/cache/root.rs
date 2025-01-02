use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::cache::CargoInstallCacheConfig;
use crate::internal::config::parser::cache::GithubReleaseCacheConfig;
use crate::internal::config::parser::cache::GoInstallCacheConfig;
use crate::internal::config::parser::cache::HomebrewCacheConfig;
use crate::internal::config::parser::cache::MiseCacheConfig;
use crate::internal::config::parser::cache::UpEnvironmentCacheConfig;
use crate::internal::config::parser::errors::ConfigErrorKind;
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
    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let path = match config_value.get("path") {
            Some(value) => match value.as_str() {
                Some(value) => value.to_string(),
                None => {
                    errors.push(ConfigErrorKind::ValueType {
                        key: format!("{}.path", error_key),
                        expected: "string".to_string(),
                        found: value.as_serde_yaml(),
                    });
                    cache_home()
                }
            },
            None => cache_home(),
        };

        let environment = UpEnvironmentCacheConfig::from_config_value(
            config_value.get("environment"),
            &format!("{}.environment", error_key),
            errors,
        );
        let github_release = GithubReleaseCacheConfig::from_config_value(
            config_value.get("github_release"),
            &format!("{}.github_release", error_key),
            errors,
        );
        let cargo_install = CargoInstallCacheConfig::from_config_value(
            config_value.get("cargo_install"),
            &format!("{}.cargo_install", error_key),
            errors,
        );
        let go_install = GoInstallCacheConfig::from_config_value(
            config_value.get("go_install"),
            &format!("{}.go_install", error_key),
            errors,
        );
        let homebrew = HomebrewCacheConfig::from_config_value(
            config_value.get("homebrew"),
            &format!("{}.homebrew", error_key),
            errors,
        );
        let mise = MiseCacheConfig::from_config_value(
            config_value.get("mise"),
            &format!("{}.mise", error_key),
            errors,
        );

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
