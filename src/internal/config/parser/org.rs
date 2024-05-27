use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrgConfig {
    pub handle: String,
    pub trusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_path_format: Option<String>,
}

impl Default for OrgConfig {
    fn default() -> Self {
        Self {
            handle: "".to_string(),
            trusted: false,
            worktree: None,
            repo_path_format: None,
        }
    }
}

impl OrgConfig {
    pub fn from_str(value_str: &str) -> Self {
        let mut split = value_str.split('=');
        let handle = split.next().unwrap().to_string();
        let worktree = split.next().map(|value| value.to_string());
        Self {
            handle,
            trusted: true,
            worktree,
            repo_path_format: None,
        }
    }

    pub fn from_config_value(config_value: &ConfigValue) -> Self {
        // If the config_value contains a value directly, we want to consider
        // it as the "handle=worktree", and not as a table.
        if config_value.is_str() {
            let value_str = config_value.as_str().unwrap();
            return OrgConfig::from_str(&value_str);
        }

        Self {
            handle: config_value.get_as_str("handle").unwrap().to_string(),
            trusted: config_value.get_as_bool("trusted").unwrap_or(false),
            worktree: config_value.get_as_str("worktree"),
            repo_path_format: config_value.get_as_str("repo_path_format"),
        }
    }
}
