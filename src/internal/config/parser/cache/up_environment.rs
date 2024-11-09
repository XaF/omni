use serde::Deserialize;
use serde::Serialize;

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

    pub fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let retention = parse_duration_or_default(
            config_value.get("retention").as_ref(),
            Self::DEFAULT_RETENTION,
        );

        let max_per_workdir = config_value
            .get("max_per_workdir")
            .and_then(|v| v.as_unsigned_integer())
            .map(|v| v as usize);

        let max_total = config_value
            .get("max_total")
            .and_then(|v| v.as_unsigned_integer())
            .map(|v| v as usize);

        Self {
            retention,
            max_per_workdir,
            max_total,
        }
    }
}
