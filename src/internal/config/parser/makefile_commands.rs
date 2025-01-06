use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MakefileCommandsConfig {
    pub enabled: bool,
    pub split_on_dash: bool,
    pub split_on_slash: bool,
}

impl Default for MakefileCommandsConfig {
    fn default() -> Self {
        Self {
            enabled: Self::DEFAULT_ENABLED,
            split_on_dash: Self::DEFAULT_SPLIT_ON_DASH,
            split_on_slash: Self::DEFAULT_SPLIT_ON_SLASH,
        }
    }
}

impl MakefileCommandsConfig {
    const DEFAULT_ENABLED: bool = true;
    const DEFAULT_SPLIT_ON_DASH: bool = true;
    const DEFAULT_SPLIT_ON_SLASH: bool = true;

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let enabled = config_value.get_as_bool_or_default(
            "enabled",
            Self::DEFAULT_ENABLED,
            &error_handler.with_key("enabled"),
        );

        let split_on_dash = config_value.get_as_bool_or_default(
            "split_on_dash",
            Self::DEFAULT_SPLIT_ON_DASH,
            &error_handler.with_key("split_on_dash"),
        );

        let split_on_slash = config_value.get_as_bool_or_default(
            "split_on_slash",
            Self::DEFAULT_SPLIT_ON_SLASH,
            &error_handler.with_key("split_on_slash"),
        );

        Self {
            enabled,
            split_on_dash,
            split_on_slash,
        }
    }
}
