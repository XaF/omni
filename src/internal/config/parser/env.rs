use std::collections::HashMap;
use std::ops::Deref;
use std::path::PathBuf;

use itertools::Itertools;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::utils::abs_path_from_path;
use crate::internal::config::config_value::ConfigData;
use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;

#[derive(Debug, Default, Deserialize, Clone)]
pub struct EnvConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<EnvOperationConfig>,
}

impl Deref for EnvConfig {
    type Target = Vec<EnvOperationConfig>;

    fn deref(&self) -> &Self::Target {
        &self.operations
    }
}

impl From<EnvConfig> for Vec<EnvOperationConfig> {
    fn from(env_config: EnvConfig) -> Self {
        env_config.operations
    }
}

impl Serialize for EnvConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.is_empty() {
            serializer.serialize_none()
        } else {
            self.operations.serialize(serializer)
        }
    }
}

impl Empty for EnvConfig {
    fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl EnvConfig {
    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let operations = if let Some(config_value) = config_value {
            let operations_array = if let Some(array) = config_value.as_array() {
                array
            } else if let Some(table) = config_value.as_table() {
                // If this is a map, create a list of individual maps for each
                // key/value pair, sorted by key for deterministic output.
                table
                    .iter()
                    .sorted_by_key(|(key, _)| key.to_string())
                    .map(|(key, value)| {
                        let mut map = HashMap::new();
                        map.insert(key.to_string(), value.clone());
                        ConfigValue::new(
                            config_value.get_source().clone(),
                            config_value.get_scope().clone(),
                            Some(Box::new(ConfigData::Mapping(map))),
                        )
                    })
                    .collect::<Vec<ConfigValue>>()
            } else {
                error_handler
                    .with_expected(vec!["array", "table"])
                    .with_actual(config_value)
                    .error(ConfigErrorKind::InvalidValueType);

                vec![]
            };

            operations_array
                .iter()
                .enumerate()
                .flat_map(|(idx, item)| {
                    EnvOperationConfig::from_config_value(item, &error_handler.with_index(idx))
                })
                .collect()
        } else {
            vec![]
        };

        Self { operations }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct EnvOperationConfig {
    pub name: String,
    pub value: Option<String>,
    pub operation: EnvOperationEnum,
}

impl EnvOperationConfig {
    fn from_config_value_multi(
        name: &str,
        config_value: &ConfigValue,
        operation: EnvOperationEnum,
        error_handler: &ConfigErrorHandler,
    ) -> Vec<Self> {
        if let Some(array) = config_value.as_array() {
            array
                .iter()
                .map(|config_value| match config_value.as_table() {
                    Some(table) => table,
                    None => {
                        let mut table = HashMap::new();
                        table.insert("value".to_string(), config_value.clone());

                        table
                    }
                })
                .enumerate()
                .filter_map(|(index, table)| {
                    Self::from_table(name, table, operation, &error_handler.with_index(index))
                })
                .collect()
        } else if let Some(table) = config_value.as_table() {
            match Self::from_table(name, table, operation, &error_handler.with_key(name)) {
                Some(value) => vec![value],
                None => vec![],
            }
        } else {
            let mut table = HashMap::new();
            table.insert("value".to_string(), config_value.clone());

            match Self::from_table(name, table, operation, error_handler) {
                Some(value) => vec![value],
                None => vec![],
            }
        }
    }

    fn from_table(
        name: &str,
        table: HashMap<String, ConfigValue>,
        operation: EnvOperationEnum,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let value_type = match table.get("type") {
            Some(value_type) => match value_type.as_str() {
                Some(vtype) if vtype == "text" || vtype == "path" => vtype.to_string(),
                Some(_) => {
                    error_handler
                        .with_key("type")
                        .with_expected(vec!["text", "path"])
                        .with_actual(value_type)
                        .error(ConfigErrorKind::InvalidValue);

                    return None;
                }
                None => {
                    error_handler
                        .with_key("type")
                        .with_expected("string")
                        .with_actual(value_type)
                        .error(ConfigErrorKind::InvalidValueType);

                    return None;
                }
            },
            None => "text".to_string(),
        };

        let value = if let Some(config_value) = table.get("value") {
            if let Some(value) = config_value.as_str_forced() {
                // If the value type is "path", we want to expand the path
                // before returning it. We can use the value ConfigSource
                // to determine the current scope.
                if value_type == "path" {
                    let source_path = match config_value.get_source() {
                        ConfigSource::File(path) => Some(path.to_string()),
                        ConfigSource::Package(path_entry) => Some(path_entry.to_string()),
                        _ => {
                            error_handler
                                .with_key("type")
                                .with_actual(value.clone())
                                .error(ConfigErrorKind::UnsupportedValueInContext);

                            None
                        }
                    };

                    match source_path {
                        Some(source_path) => {
                            let parent_path = PathBuf::from(source_path)
                                .parent()
                                .expect("config file path has no parent")
                                .to_string_lossy()
                                .to_string();
                            Some(
                                abs_path_from_path(&value, Some(&parent_path))
                                    .to_string_lossy()
                                    .to_string(),
                            )
                        }
                        None => Some(value.to_string()),
                    }
                } else {
                    Some(value.to_string())
                }
            } else {
                error_handler
                    .with_key("value")
                    .with_expected("string")
                    .with_actual(config_value)
                    .error(ConfigErrorKind::InvalidValueType);

                None
            }
        } else {
            None
        };

        if value.is_none() && operation != EnvOperationEnum::Set {
            error_handler
                .with_key("value")
                .error(ConfigErrorKind::MissingKey);

            return None;
        }

        Some(Self {
            name: name.to_string(),
            value,
            operation,
        })
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Vec<Self> {
        // The config_value should be a table.
        let table = match config_value.as_table() {
            Some(table) => table,
            None => {
                error_handler
                    .with_expected("table")
                    .with_actual(config_value)
                    .error(ConfigErrorKind::InvalidValueType);

                return vec![];
            }
        };

        // There should be exactly one key/value pair in the table.
        if table.len() != 1 {
            error_handler
                .with_actual(config_value)
                .error(ConfigErrorKind::NotExactlyOneKeyInTable);

            return vec![];
        }

        // Get the unique key, value pair; we can unwrap here because we know
        // there is exactly one pair.
        let (name, value) = table.iter().next().unwrap();

        // Now we can try and figure out how to parse the value
        if let Some(table) = value.as_table() {
            if let Some(config_value) = table.get("set") {
                return match Self::from_config_value_multi(
                    name,
                    config_value,
                    EnvOperationEnum::Set,
                    &error_handler.with_key("set"),
                )
                .pop()
                {
                    Some(value) => vec![value],
                    _ => vec![],
                };
            }

            let mut operations = vec![];
            let mut matched_any = false;

            if let Some(config_value) = table.get("remove") {
                matched_any = true;
                operations.extend(Self::from_config_value_multi(
                    name,
                    config_value,
                    EnvOperationEnum::Remove,
                    &error_handler.with_key("remove"),
                ))
            }

            if let Some(config_value) = table.get("prepend") {
                matched_any = true;
                operations.extend(Self::from_config_value_multi(
                    name,
                    config_value,
                    EnvOperationEnum::Prepend,
                    &error_handler.with_key("prepend"),
                ))
            }

            if let Some(config_value) = table.get("append") {
                matched_any = true;
                operations.extend(Self::from_config_value_multi(
                    name,
                    config_value,
                    EnvOperationEnum::Append,
                    &error_handler.with_key("append"),
                ))
            }

            if let Some(config_value) = table.get("prefix") {
                matched_any = true;
                operations.extend(Self::from_config_value_multi(
                    name,
                    config_value,
                    EnvOperationEnum::Prefix,
                    &error_handler.with_key("prefix"),
                ))
            }

            if let Some(config_value) = table.get("suffix") {
                matched_any = true;
                operations.extend(Self::from_config_value_multi(
                    name,
                    config_value,
                    EnvOperationEnum::Suffix,
                    &error_handler.with_key("suffix"),
                ))
            }

            if matched_any {
                return operations;
            }

            match Self::from_table(name, table, EnvOperationEnum::Set, error_handler) {
                Some(value) => vec![value],
                None => vec![],
            }
        } else if let Some(value) =
            Self::from_config_value_multi(name, value, EnvOperationEnum::Set, error_handler).pop()
        {
            vec![value]
        } else {
            vec![]
        }
    }
}

impl Serialize for EnvOperationConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.operation {
            EnvOperationEnum::Set => {
                let mut env_var = HashMap::new();
                env_var.insert(self.name.clone(), self.value.clone());
                env_var.serialize(serializer)
            }
            EnvOperationEnum::Prepend
            | EnvOperationEnum::Append
            | EnvOperationEnum::Remove
            | EnvOperationEnum::Prefix
            | EnvOperationEnum::Suffix => {
                let mut env_var_wrapped = HashMap::new();
                env_var_wrapped.insert(self.operation.to_string(), self.value.clone());

                let mut env_var = HashMap::new();
                env_var.insert(self.name.clone(), env_var_wrapped);
                env_var.serialize(serializer)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy, Default, Hash)]
pub enum EnvOperationEnum {
    #[default]
    #[serde(rename = "s", alias = "set")]
    Set,
    #[serde(rename = "p", alias = "prepend")]
    Prepend,
    #[serde(rename = "a", alias = "append")]
    Append,
    #[serde(rename = "r", alias = "remove")]
    Remove,
    #[serde(rename = "pf", alias = "prefix")]
    Prefix,
    #[serde(rename = "sf", alias = "suffix")]
    Suffix,
}

impl std::fmt::Display for EnvOperationEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", std::str::from_utf8(self.as_bytes()).unwrap())
    }
}

impl EnvOperationEnum {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            EnvOperationEnum::Set => b"set",
            EnvOperationEnum::Prepend => b"prepend",
            EnvOperationEnum::Append => b"append",
            EnvOperationEnum::Remove => b"remove",
            EnvOperationEnum::Prefix => b"prefix",
            EnvOperationEnum::Suffix => b"suffix",
        }
    }

    pub fn is_default(other: &EnvOperationEnum) -> bool {
        *other == EnvOperationEnum::default()
    }
}
