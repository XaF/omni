use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MatchSkipPromptIfConfig {
    pub enabled: bool,
    pub first_min: f64,
    pub second_max: f64,
}

impl Default for MatchSkipPromptIfConfig {
    fn default() -> Self {
        Self::from_config_value(None)
    }
}

impl MatchSkipPromptIfConfig {
    const DEFAULT_ENABLED: bool = true;
    const DEFAULT_FIRST_MIN: f64 = 0.80;
    const DEFAULT_SECOND_MAX: f64 = 0.60;

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        match config_value {
            Some(config_value) => Self {
                enabled: match config_value.get("enabled") {
                    Some(value) => value.as_bool().unwrap(),
                    None => Self::DEFAULT_ENABLED,
                },
                first_min: match config_value.get("first_min") {
                    Some(value) => value.as_float().unwrap(),
                    None => Self::DEFAULT_FIRST_MIN,
                },
                second_max: match config_value.get("second_max") {
                    Some(value) => value.as_float().unwrap(),
                    None => Self::DEFAULT_SECOND_MAX,
                },
            },
            None => Self {
                enabled: false,
                first_min: Self::DEFAULT_FIRST_MIN,
                second_max: Self::DEFAULT_SECOND_MAX,
            },
        }
    }
}
