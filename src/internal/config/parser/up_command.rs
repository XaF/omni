use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpCommandConfig {
    pub auto_bootstrap: bool,
    pub notify_workdir_config_updated: bool,
    pub notify_workdir_config_available: bool,
    pub preferred_tools: Vec<String>,
    pub upgrade: bool,
}

impl Default for UpCommandConfig {
    fn default() -> Self {
        Self {
            auto_bootstrap: Self::DEFAULT_AUTO_BOOTSTRAP,
            notify_workdir_config_updated: Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED,
            notify_workdir_config_available: Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE,
            preferred_tools: Vec::new(),
            upgrade: Self::DEFAULT_UPGRADE,
        }
    }
}

impl UpCommandConfig {
    const DEFAULT_AUTO_BOOTSTRAP: bool = true;
    const DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED: bool = true;
    const DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE: bool = true;
    const DEFAULT_UPGRADE: bool = false;

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let config_value_global = match config_value.reject_scope(&ConfigScope::Workdir) {
            Some(config_value) => config_value,
            None => ConfigValue::default(),
        };

        let preferred_tools =
            if let Some(preferred_tools) = config_value_global.get_as_array("preferred_tools") {
                preferred_tools
                    .iter()
                    .filter_map(|value| value.as_str().map(|value| value.to_string()))
                    .collect()
            } else if let Some(preferred_tool) =
                config_value_global.get_as_str_forced("preferred_tools")
            {
                vec![preferred_tool]
            } else {
                Vec::new()
            };

        Self {
            auto_bootstrap: config_value_global
                .get_as_bool_forced("auto_bootstrap")
                .unwrap_or(Self::DEFAULT_AUTO_BOOTSTRAP),
            notify_workdir_config_updated: config_value_global
                .get_as_bool_forced("notify_workdir_config_updated")
                .unwrap_or(Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED),
            notify_workdir_config_available: config_value_global
                .get_as_bool_forced("notify_workdir_config_available")
                .unwrap_or(Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE),
            preferred_tools,
            // The upgrade option is fine to handle as a workdir option too
            upgrade: config_value
                .get_as_bool_forced("upgrade")
                .unwrap_or(Self::DEFAULT_UPGRADE),
        }
    }
}
