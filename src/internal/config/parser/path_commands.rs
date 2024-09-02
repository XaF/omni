use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathCommandsConfig {
    pub help_parser: bool,
}

impl Default for PathCommandsConfig {
    fn default() -> Self {
        Self {
            help_parser: Self::DEFAULT_HELP_PARSER,
        }
    }
}

impl PathCommandsConfig {
    const DEFAULT_HELP_PARSER: bool = true;

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        Self {
            help_parser: match config_value.get("help_parser") {
                Some(value) => value.as_bool().unwrap(),
                None => Self::DEFAULT_HELP_PARSER,
            },
        }
    }
}
