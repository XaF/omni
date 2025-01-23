use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::StringFilter;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;
use crate::internal::env::shell_is_interactive;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathRepoUpdatesConfig {
    pub enabled: bool,
    pub self_update: PathRepoUpdatesSelfUpdateEnum,
    pub on_command_not_found: PathRepoUpdatesOnCommandNotFoundEnum,
    pub pre_auth: bool,
    pub pre_auth_timeout: u64,
    pub background_updates: bool,
    pub background_updates_timeout: u64,
    pub interval: u64,
    pub ref_type: String,
    #[serde(skip_serializing_if = "StringFilter::is_default")]
    pub ref_match: StringFilter,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub per_repo_config: Vec<PathRepoUpdatesPerRepoConfig>,
}

impl Default for PathRepoUpdatesConfig {
    fn default() -> Self {
        Self {
            enabled: Self::DEFAULT_ENABLED,
            self_update: PathRepoUpdatesSelfUpdateEnum::default(),
            on_command_not_found: PathRepoUpdatesOnCommandNotFoundEnum::default(),
            pre_auth: Self::DEFAULT_PRE_AUTH,
            pre_auth_timeout: Self::DEFAULT_PRE_AUTH_TIMEOUT,
            background_updates: Self::DEFAULT_BACKGROUND_UPDATES,
            background_updates_timeout: Self::DEFAULT_BACKGROUND_UPDATES_TIMEOUT,
            interval: Self::DEFAULT_INTERVAL,
            ref_type: Self::DEFAULT_REF_TYPE.to_string(),
            ref_match: StringFilter::default(),
            per_repo_config: Vec::new(),
        }
    }
}

impl PathRepoUpdatesConfig {
    const DEFAULT_ENABLED: bool = true;
    const DEFAULT_PRE_AUTH: bool = true;
    const DEFAULT_PRE_AUTH_TIMEOUT: u64 = 120; // 2 minutes
    const DEFAULT_BACKGROUND_UPDATES: bool = true;
    const DEFAULT_BACKGROUND_UPDATES_TIMEOUT: u64 = 3600; // 1 hour
    const DEFAULT_INTERVAL: u64 = 43200; // 12 hours
    const DEFAULT_REF_TYPE: &'static str = "branch";

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let mut per_repo_config = Vec::new();
        if let Some(value) = config_value.get("per_repo_config") {
            // This can be either a table (old format) or a list (new format)
            // In the case it's a table, we need to extract the repository id from the key

            if let Some(array) = value.as_array() {
                for (index, value) in array.iter().enumerate() {
                    per_repo_config.push(PathRepoUpdatesPerRepoConfig::from_config_value(
                        value,
                        &error_handler.with_key("per_repo_config").with_index(index),
                        None,
                    ));
                }
            } else if let Some(table) = value.as_table() {
                for (key, value) in table {
                    per_repo_config.push(PathRepoUpdatesPerRepoConfig::from_config_value(
                        &value,
                        &error_handler.with_key("per_repo_config").with_key(&key),
                        Some(key.to_string()),
                    ));
                }
            } else {
                error_handler
                    .with_key("per_repo_config")
                    .with_expected(vec!["array", "table"])
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);
            }
        };

        let pre_auth_timeout = parse_duration_or_default(
            config_value.get("pre_auth_timeout").as_ref(),
            Self::DEFAULT_PRE_AUTH_TIMEOUT,
            &error_handler.with_key("pre_auth_timeout"),
        );
        let background_updates_timeout = parse_duration_or_default(
            config_value.get("background_updates_timeout").as_ref(),
            Self::DEFAULT_BACKGROUND_UPDATES_TIMEOUT,
            &error_handler.with_key("background_updates_timeout"),
        );
        let interval = parse_duration_or_default(
            config_value.get("interval").as_ref(),
            Self::DEFAULT_INTERVAL,
            &error_handler.with_key("interval"),
        );

        let self_update = if let Some(value) = config_value.get("self_update") {
            if let Some(value) = value.as_bool() {
                PathRepoUpdatesSelfUpdateEnum::from_bool(value)
            } else if let Some(value) = value.as_str() {
                // TODO: handle errors here ?
                PathRepoUpdatesSelfUpdateEnum::from_str(&value)
            } else if let Some(value) = value.as_integer() {
                PathRepoUpdatesSelfUpdateEnum::from_int(value)
            } else {
                error_handler
                    .with_key("self_update")
                    .with_expected(vec!["boolean", "string", "integer"])
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);

                PathRepoUpdatesSelfUpdateEnum::default()
            }
        } else {
            PathRepoUpdatesSelfUpdateEnum::default()
        };

        let on_command_not_found = if let Some(value) = config_value.get("on_command_not_found") {
            if let Some(value) = value.as_bool() {
                PathRepoUpdatesOnCommandNotFoundEnum::from_bool(value)
            } else if let Some(value) = value.as_str() {
                // TODO: handle errors here ?
                PathRepoUpdatesOnCommandNotFoundEnum::from_str(&value)
            } else if let Some(value) = value.as_integer() {
                PathRepoUpdatesOnCommandNotFoundEnum::from_int(value)
            } else {
                error_handler
                    .with_key("on_command_not_found")
                    .with_expected(vec!["boolean", "string", "integer"])
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);

                PathRepoUpdatesOnCommandNotFoundEnum::default()
            }
        } else {
            PathRepoUpdatesOnCommandNotFoundEnum::default()
        };

        let ref_type = if let Some(value) = config_value.get("ref_type") {
            if let Some(value) = value.as_str() {
                value.to_string()
            } else {
                error_handler
                    .with_key("ref_type")
                    .with_expected("string")
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);

                Self::DEFAULT_REF_TYPE.to_string()
            }
        } else {
            Self::DEFAULT_REF_TYPE.to_string()
        };

        let ref_match = StringFilter::from_config_value(
            config_value.get("ref_match"),
            &error_handler.with_key("ref_match"),
        );

        Self {
            enabled: config_value.get_as_bool_or_default(
                "enabled",
                Self::DEFAULT_ENABLED,
                &error_handler.with_key("enabled"),
            ),
            self_update,
            on_command_not_found,
            pre_auth: config_value.get_as_bool_or_default(
                "pre_auth",
                Self::DEFAULT_PRE_AUTH,
                &error_handler.with_key("pre_auth"),
            ),
            pre_auth_timeout,
            background_updates: config_value.get_as_bool_or_default(
                "background_updates",
                Self::DEFAULT_BACKGROUND_UPDATES,
                &error_handler.with_key("background_updates"),
            ),
            background_updates_timeout,
            interval,
            ref_type,
            ref_match,
            per_repo_config,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum PathRepoUpdatesSelfUpdateEnum {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
    #[serde(rename = "nocheck")]
    NoCheck,
    #[default]
    #[serde(other, rename = "ask")]
    Ask,
}

impl PathRepoUpdatesSelfUpdateEnum {
    pub fn from_bool(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "y" => Self::True,
            "false" | "no" | "n" => Self::False,
            "nocheck" => Self::NoCheck,
            "ask" => Self::Ask,
            _ => Self::default(),
        }
    }

    pub fn from_int(value: i64) -> Self {
        match value {
            0 => Self::False,
            1 => Self::True,
            _ => Self::Ask,
        }
    }

    pub fn do_not_check(&self) -> bool {
        matches!(self, PathRepoUpdatesSelfUpdateEnum::NoCheck)
    }

    pub fn is_false(&self) -> bool {
        match self {
            Self::False => true,
            Self::Ask => !shell_is_interactive(),
            _ => false,
        }
    }

    pub fn is_ask(&self) -> bool {
        match self {
            Self::Ask => shell_is_interactive(),
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum PathRepoUpdatesOnCommandNotFoundEnum {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
    #[default]
    #[serde(other, rename = "ask")]
    Ask,
}

impl PathRepoUpdatesOnCommandNotFoundEnum {
    pub fn from_bool(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "y" => Self::True,
            "false" | "no" | "n" => Self::False,
            "ask" => Self::Ask,
            _ => Self::default(),
        }
    }

    pub fn from_int(value: i64) -> Self {
        match value {
            0 => Self::False,
            1 => Self::True,
            _ => Self::default(),
        }
    }

    pub fn is_false(&self) -> bool {
        match self {
            Self::False => true,
            Self::Ask => !shell_is_interactive(),
            _ => false,
        }
    }

    pub fn is_ask(&self) -> bool {
        match self {
            Self::Ask => shell_is_interactive(),
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathRepoUpdatesPerRepoConfig {
    pub workdir_id: StringFilter,
    pub enabled: bool,
    pub ref_type: String,
    #[serde(skip_serializing_if = "StringFilter::is_default")]
    pub ref_match: StringFilter,
}

impl PathRepoUpdatesPerRepoConfig {
    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
        workdir_id: Option<String>,
    ) -> Self {
        let workdir_id = if let Some(wdid) = workdir_id {
            // If the workdir id is provided in the function call, use it as exact match
            StringFilter::Exact(wdid)
        } else {
            StringFilter::from_config_value(
                config_value.get("workdir_id"),
                &error_handler.with_key("workdir_id"),
            )
        };

        let enabled = config_value.get_as_bool_or_default(
            "enabled",
            true,
            &error_handler.with_key("enabled"),
        );

        let ref_type = config_value.get_as_str_or_default(
            "ref_type",
            "branch",
            &error_handler.with_key("ref_type"),
        );

        let ref_match = StringFilter::from_config_value(
            config_value.get("ref_match"),
            &error_handler.with_key("ref_match"),
        );

        Self {
            workdir_id,
            enabled,
            ref_type,
            ref_match,
        }
    }
}
