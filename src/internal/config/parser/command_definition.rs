use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandDefinition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    pub run: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax: Option<CommandSyntax>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcommands: Option<HashMap<String, CommandDefinition>>,
    #[serde(skip)]
    pub source: ConfigSource,
    #[serde(skip)]
    pub scope: ConfigScope,
}

impl CommandDefinition {
    pub(super) fn from_config_value(config_value: &ConfigValue) -> Self {
        let syntax = match config_value.get("syntax") {
            Some(value) => CommandSyntax::from_config_value(&value),
            None => None,
        };

        let category = match config_value.get("category") {
            Some(value) => {
                let mut category = Vec::new();
                if value.is_array() {
                    for value in value.as_array().unwrap() {
                        category.push(value.as_str().unwrap().to_string());
                    }
                } else {
                    category.push(value.as_str().unwrap().to_string());
                }
                Some(category)
            }
            None => None,
        };

        let subcommands = match config_value.get("subcommands") {
            Some(value) => {
                let mut subcommands = HashMap::new();
                for (key, value) in value.as_table().unwrap() {
                    subcommands.insert(
                        key.to_string(),
                        CommandDefinition::from_config_value(&value),
                    );
                }
                Some(subcommands)
            }
            None => None,
        };

        let aliases = match config_value.get_as_array("aliases") {
            Some(value) => value
                .iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect(),
            None => vec![],
        };

        Self {
            desc: config_value
                .get("desc")
                .map(|value| value.as_str().unwrap().to_string()),
            run: config_value
                .get_as_str("run")
                .unwrap_or("true".to_string())
                .to_string(),
            aliases,
            syntax,
            category,
            dir: config_value
                .get_as_str("dir")
                .map(|value| value.to_string()),
            subcommands,
            source: config_value.get_source().clone(),
            scope: config_value.current_scope().clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandSyntax {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SyntaxOptArg>,
}

impl CommandSyntax {
    pub fn new() -> Self {
        CommandSyntax {
            usage: None,
            parameters: vec![],
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        let config_value = ConfigValue::from_value(ConfigSource::Null, ConfigScope::Null, value);
        if let Some(command_syntax) = CommandSyntax::from_config_value(&config_value) {
            Ok(command_syntax)
        } else {
            Err(serde::de::Error::custom("invalid command syntax"))
        }
    }

    pub(super) fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
        let mut usage = None;
        let mut parameters = vec![];

        if let Some(array) = config_value.as_array() {
            parameters.extend(
                array
                    .iter()
                    .filter_map(|value| SyntaxOptArg::from_config_value(value, None)),
            );
        } else if let Some(table) = config_value.as_table() {
            let keys = [
                ("parameters", None),
                ("arguments", Some(true)),
                ("argument", Some(true)),
                ("options", Some(false)),
                ("option", Some(false)),
                ("optional", Some(false)),
            ];

            for (key, required) in keys {
                if let Some(value) = table.get(key) {
                    if let Some(value) = value.as_array() {
                        let arguments = value
                            .iter()
                            .filter_map(|value| SyntaxOptArg::from_config_value(value, required))
                            .collect::<Vec<SyntaxOptArg>>();
                        parameters.extend(arguments);
                    } else if let Some(arg) = SyntaxOptArg::from_config_value(value, required) {
                        parameters.push(arg);
                    }
                }
            }

            if let Some(value) = table.get("usage") {
                if let Some(value) = value.as_str() {
                    usage = Some(value.to_string());
                }
            }
        } else if let Some(value) = config_value.as_str() {
            usage = Some(value.to_string());
        }

        if parameters.is_empty() && usage.is_none() {
            return None;
        }

        Some(Self { usage, parameters })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyntaxOptArg {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    pub required: bool,
}

impl SyntaxOptArg {
    pub fn new(name: String, desc: Option<String>, required: bool) -> Self {
        Self {
            name,
            desc,
            required,
        }
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        required: Option<bool>,
    ) -> Option<Self> {
        let name;
        let mut desc = None;
        let mut required = required;

        if let Some(table) = config_value.as_table() {
            let value_for_details;

            if let Some(name_value) = table.get("name") {
                if let Some(name_value) = name_value.as_str() {
                    name = name_value.to_string();
                    value_for_details = Some(config_value.clone());
                } else {
                    return None;
                }
            } else if table.len() == 1 {
                if let Some((key, value)) = table.into_iter().next() {
                    name = key;
                    value_for_details = Some(value);
                } else {
                    return None;
                }
            } else {
                return None;
            }

            if let Some(value_for_details) = value_for_details {
                if let Some(value_str) = value_for_details.as_str() {
                    desc = Some(value_str.to_string());
                } else if let Some(value_table) = value_for_details.as_table() {
                    desc = value_table.get("desc")?.as_str();
                    if required.is_none() {
                        required = value_table.get("required")?.as_bool();
                    }
                }
            }
        } else {
            name = config_value.as_str().unwrap();
        }

        Some(Self {
            name,
            desc,
            required: required.unwrap_or(false),
        })
    }
}
