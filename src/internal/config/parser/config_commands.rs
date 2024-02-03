use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigCommandsConfig {
    pub split_on_dash: bool,
    pub split_on_slash: bool,
}

impl Default for ConfigCommandsConfig {
    fn default() -> Self {
        Self {
            split_on_dash: Self::DEFAULT_SPLIT_ON_DASH,
            split_on_slash: Self::DEFAULT_SPLIT_ON_SLASH,
        }
    }
}

impl ConfigCommandsConfig {
    const DEFAULT_SPLIT_ON_DASH: bool = true;
    const DEFAULT_SPLIT_ON_SLASH: bool = true;

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        Self {
            split_on_dash: match config_value.get("split_on_dash") {
                Some(value) => value.as_bool().unwrap(),
                None => Self::DEFAULT_SPLIT_ON_DASH,
            },
            split_on_slash: match config_value.get("split_on_slash") {
                Some(value) => value.as_bool().unwrap(),
                None => Self::DEFAULT_SPLIT_ON_SLASH,
            },
        }
    }
}
