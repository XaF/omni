use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CloneConfig {
    pub auto_up: bool,
    pub ls_remote_timeout: u64,
}

impl CloneConfig {
    const DEFAULT_AUTO_UP: bool = true;
    const DEFAULT_LS_REMOTE_TIMEOUT: u64 = 5;

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => {
                return Self {
                    auto_up: Self::DEFAULT_AUTO_UP,
                    ls_remote_timeout: Self::DEFAULT_LS_REMOTE_TIMEOUT,
                };
            }
        };

        let ls_remote_timeout = parse_duration_or_default(
            config_value.get("ls_remote_timeout").as_ref(),
            config_value
                .get_as_unsigned_integer("ls_remote_timeout_seconds")
                .unwrap_or(Self::DEFAULT_LS_REMOTE_TIMEOUT),
            &format!("{}.ls_remote_timeout", error_key),
            on_error,
        );

        Self {
            auto_up: config_value.get_as_bool_or_default(
                "auto_up",
                Self::DEFAULT_AUTO_UP,
                &format!("{}.auto_up", error_key),
                on_error,
            ),
            ls_remote_timeout,
        }
    }
}
