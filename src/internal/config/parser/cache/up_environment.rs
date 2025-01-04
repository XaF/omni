use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentCacheConfig {
    pub retention: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_workdir: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total: Option<usize>,
}

impl Default for UpEnvironmentCacheConfig {
    fn default() -> Self {
        Self {
            retention: Self::DEFAULT_RETENTION,
            max_per_workdir: None,
            max_total: None,
        }
    }
}

impl UpEnvironmentCacheConfig {
    const DEFAULT_RETENTION: u64 = 7776000; // 90 days

    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let retention = parse_duration_or_default(
            config_value.get("retention").as_ref(),
            Self::DEFAULT_RETENTION,
            &format!("{}.retention", error_key),
            on_error,
        );

        let max_per_workdir = match config_value.get("max_per_workdir") {
            Some(v) => match v.as_unsigned_integer() {
                Some(v) => Some(v as usize),
                None => {
                    on_error(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.max_per_workdir", error_key),
                        expected: "unsigned integer".to_string(),
                        actual: v.as_serde_yaml(),
                    });
                    None
                }
            },
            None => None,
        };

        let max_total = match config_value.get("max_total") {
            Some(v) => match v.as_unsigned_integer() {
                Some(v) => Some(v as usize),
                None => {
                    on_error(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.max_total", error_key),
                        expected: "unsigned integer".to_string(),
                        actual: v.as_serde_yaml(),
                    });
                    None
                }
            },
            None => None,
        };

        Self {
            retention,
            max_per_workdir,
            max_total,
        }
    }
}
