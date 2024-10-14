use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AskPassConfig {
    pub enabled: bool,
    pub enable_gui: bool,
    pub prefer_gui: bool,
}

impl Default for AskPassConfig {
    fn default() -> Self {
        Self {
            enabled: Self::DEFAULT_ENABLED,
            enable_gui: Self::DEFAULT_ENABLE_GUI,
            prefer_gui: Self::DEFAULT_PREFER_GUI,
        }
    }
}

impl AskPassConfig {
    const DEFAULT_ENABLED: bool = true;
    const DEFAULT_ENABLE_GUI: bool = true;
    const DEFAULT_PREFER_GUI: bool = false;

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let config_value = match config_value.reject_scope(&ConfigScope::Workdir) {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        Self {
            enabled: config_value
                .get_as_bool_forced("enabled")
                .unwrap_or(Self::DEFAULT_ENABLED),
            enable_gui: config_value
                .get_as_bool_forced("enable_gui")
                .unwrap_or(Self::DEFAULT_ENABLE_GUI),
            prefer_gui: config_value
                .get_as_bool_forced("prefer_gui")
                .unwrap_or(Self::DEFAULT_PREFER_GUI),
        }
    }
}
