use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
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
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let mut aliases = vec![];
        if let Some(config_value) = config_value {
            if let Some(array) = config_value.as_array() {
                for (idx, value) in array.iter().enumerate() {
                    if let Some(alias) = ShellAliasConfig::from_config_value(
                        &value,
                        &format!("{}[{}]", error_key, idx),
                        errors,
                    ) {
                        aliases.push(alias);
                    }
                }
            } else {
                errors.push(ConfigErrorKind::InvalidValueType {
                    key: error_key.to_string(),
                    actual: config_value.as_serde_yaml(),
                    expected: "array".to_string(),
                });
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
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
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
                    errors.push(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.alias", error_key),
                        actual: value.as_serde_yaml(),
                        expected: "string".to_string(),
                    });
                    return None;
                }
            } else {
                errors.push(ConfigErrorKind::MissingKey {
                    key: format!("{}.alias", error_key),
                });
                return None;
            };

            let mut target = None;
            if let Some(value) = table.get("target") {
                if let Some(value) = value.as_str() {
                    target = Some(value.to_string());
                } else {
                    errors.push(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.target", error_key),
                        actual: value.as_serde_yaml(),
                        expected: "string".to_string(),
                    });
                    return None;
                }
            }

            Some(Self { alias, target })
        } else {
            errors.push(ConfigErrorKind::InvalidValueType {
                key: error_key.to_string(),
                actual: config_value.as_serde_yaml(),
                expected: "string or table".to_string(),
            });

            None
        }
    }
}
