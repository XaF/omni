use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorKind;
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

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        Self {
            split_on_dash: config_value.get_as_bool_or_default(
                "split_on_dash",
                Self::DEFAULT_SPLIT_ON_DASH,
                &format!("{}.split_on_dash", error_key),
                on_error,
            ),
            split_on_slash: config_value.get_as_bool_or_default(
                "split_on_slash",
                Self::DEFAULT_SPLIT_ON_SLASH,
                &format!("{}.split_on_slash", error_key),
                on_error,
            ),
        }
    }
}
