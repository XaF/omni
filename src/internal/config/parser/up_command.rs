use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpCommandConfig {
    pub auto_bootstrap: bool,
    pub notify_workdir_config_updated: bool,
    pub notify_workdir_config_available: bool,
}

impl UpCommandConfig {
    const DEFAULT_AUTO_BOOTSTRAP: bool = true;
    const DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED: bool = true;
    const DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE: bool = true;

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        if let Some(config_value) = config_value {
            if let Some(config_value) = config_value.reject_scope(&ConfigScope::Workdir) {
                return Self {
                    auto_bootstrap: config_value
                        .get_as_bool("auto_bootstrap")
                        .unwrap_or(Self::DEFAULT_AUTO_BOOTSTRAP),
                    notify_workdir_config_updated: config_value
                        .get_as_bool("notify_workdir_config_updated")
                        .unwrap_or(Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED),
                    notify_workdir_config_available: config_value
                        .get_as_bool("notify_workdir_config_available")
                        .unwrap_or(Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE),
                };
            }
        }

        Self {
            auto_bootstrap: Self::DEFAULT_AUTO_BOOTSTRAP,
            notify_workdir_config_updated: Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED,
            notify_workdir_config_available: Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE,
        }
    }
}
