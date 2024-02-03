use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
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
    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let mut aliases = vec![];
        if let Some(config_value) = config_value {
            if let Some(array) = config_value.as_array() {
                for value in array {
                    if let Some(alias) = ShellAliasConfig::from_config_value(&value) {
                        aliases.push(alias);
                    }
                }
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
    pub(super) fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
        if let Some(value) = config_value.as_str() {
            return Some(Self {
                alias: value.to_string(),
                target: None,
            });
        } else if let Some(table) = config_value.as_table() {
            let mut alias = None;
            if let Some(value) = table.get("alias") {
                if let Some(value) = value.as_str() {
                    alias = Some(value.to_string());
                }
            }

            alias.as_ref()?;

            let mut target = None;
            if let Some(value) = table.get("target") {
                if let Some(value) = value.as_str() {
                    target = Some(value.to_string());
                }
            }

            return Some(Self {
                alias: alias.unwrap(),
                target,
            });
        }

        None
    }
}
