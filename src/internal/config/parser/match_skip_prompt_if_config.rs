use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MatchSkipPromptIfConfig {
    pub enabled: bool,
    pub first_min: f64,
    pub second_max: f64,
}

impl Default for MatchSkipPromptIfConfig {
    fn default() -> Self {
        Self {
            // By default if nothing is specified, we disable this
            enabled: false,
            first_min: Self::DEFAULT_FIRST_MIN,
            second_max: Self::DEFAULT_SECOND_MAX,
        }
    }
}

impl MatchSkipPromptIfConfig {
    const DEFAULT_ENABLED: bool = true;
    const DEFAULT_FIRST_MIN: f64 = 0.80;
    const DEFAULT_SECOND_MAX: f64 = 0.60;

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        match config_value {
            Some(config_value) => Self {
                enabled: config_value.get_as_bool_or_default(
                    "enabled",
                    Self::DEFAULT_ENABLED,
                    &format!("{}.enabled", error_key),
                    on_error,
                ),
                first_min: config_value.get_as_float_or_default(
                    "first_min",
                    Self::DEFAULT_FIRST_MIN,
                    &format!("{}.first_min", error_key),
                    on_error,
                ),
                second_max: config_value.get_as_float_or_default(
                    "second_max",
                    Self::DEFAULT_SECOND_MAX,
                    &format!("{}.second_max", error_key),
                    on_error,
                ),
            },
            None => Self::default(),
        }
    }
}
