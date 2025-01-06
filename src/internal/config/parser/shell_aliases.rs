use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::ConfigValue;

#[derive(Debug, Deserialize, Clone)]
pub struct ShellAliasesConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<ShellAliasConfig>,
}

impl Empty for ShellAliasesConfig {
    fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

impl Serialize for ShellAliasesConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.aliases.serialize(serializer)
    }
}

impl ShellAliasesConfig {
    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let mut aliases = vec![];
        if let Some(config_value) = config_value {
            if let Some(array) = config_value.as_array() {
                for (idx, value) in array.iter().enumerate() {
                    if let Some(alias) =
                        ShellAliasConfig::from_config_value(value, &error_handler.with_index(idx))
                    {
                        aliases.push(alias);
                    }
                }
            } else {
                error_handler
                    .with_expected("array")
                    .with_actual(config_value)
                    .error(ConfigErrorKind::InvalidValueType);
            }
        }
        Self { aliases }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShellAliasConfig {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl ShellAliasConfig {
    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        if let Some(value) = config_value.as_str() {
            Some(Self {
                alias: value.to_string(),
                target: None,
            })
        } else if let Some(table) = config_value.as_table() {
            let alias = if let Some(value) = table.get("alias") {
                if let Some(value) = value.as_str() {
                    value.to_string()
                } else {
                    error_handler
                        .with_key("alias")
                        .with_expected("string")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);

                    return None;
                }
            } else {
                error_handler
                    .with_key("alias")
                    .error(ConfigErrorKind::MissingKey);

                return None;
            };

            let mut target = None;
            if let Some(value) = table.get("target") {
                if let Some(value) = value.as_str() {
                    target = Some(value.to_string());
                } else {
                    error_handler
                        .with_key("target")
                        .with_expected("string")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);

                    return None;
                }
            }

            Some(Self { alias, target })
        } else {
            error_handler
                .with_expected(vec!["string", "table"])
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            None
        }
    }
}
