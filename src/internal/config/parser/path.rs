use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::config_value::ConfigData;
use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;
use crate::internal::git::package_path_from_handle;
use crate::internal::git::package_root_path;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct PathConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub append: Vec<PathEntryConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prepend: Vec<PathEntryConfig>,
}

impl PathConfig {
    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let append = if let Some(append) = config_value.get("append") {
            if let Some(array) = append.as_array() {
                array
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, value)| {
                        PathEntryConfig::from_config_value(
                            value,
                            &error_handler.with_key("append").with_index(idx),
                        )
                    })
                    .collect()
            } else {
                error_handler
                    .with_key("append")
                    .with_expected("array")
                    .with_actual(append)
                    .error(ConfigErrorKind::InvalidValueType);

                vec![]
            }
        } else {
            vec![]
        };

        let prepend = if let Some(prepend) = config_value.get("prepend") {
            if let Some(array) = prepend.as_array() {
                array
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, value)| {
                        PathEntryConfig::from_config_value(
                            value,
                            &error_handler.with_key("prepend").with_index(idx),
                        )
                    })
                    .collect()
            } else {
                error_handler
                    .with_key("prepend")
                    .with_expected("array")
                    .with_actual(prepend)
                    .error(ConfigErrorKind::InvalidValueType);

                vec![]
            }
        } else {
            vec![]
        };

        Self { append, prepend }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PathEntryConfig {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(skip)]
    pub full_path: String,
}

impl fmt::Display for PathEntryConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.full_path)
    }
}

impl PathEntryConfig {
    pub fn from_path(path: &str) -> Self {
        Self {
            path: path.to_string(),
            package: None,
            full_path: if path.starts_with('/') {
                path.to_string()
            } else {
                "".to_string()
            },
        }
    }

    pub fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        if config_value.is_table() {
            let path =
                config_value.get_as_str_or_default("path", "", &error_handler.with_key("path"));
            let package =
                config_value.get_as_str_or_none("package", &error_handler.with_key("package"));
            let absolute_path = path.starts_with('/');

            if let Some(package) = package {
                if absolute_path {
                    error_handler
                        .with_key("package")
                        .with_actual(config_value.get("package").expect("package should exist"))
                        .error(ConfigErrorKind::UnsupportedValueInContext)
                } else if let Some(package_path) = package_path_from_handle(&package) {
                    let mut full_path = package_path;
                    if !path.is_empty() {
                        full_path = full_path.join(path.clone());
                    }

                    return Some(Self {
                        path: path.clone(),
                        package: Some(package.to_string()),
                        full_path: full_path.to_str().unwrap().to_string(),
                    });
                } else {
                    error_handler
                        .with_key("package")
                        .with_context("package", package)
                        .error(ConfigErrorKind::InvalidPackage);

                    return None;
                }
            }

            Some(Self {
                path: path.clone(),
                package: None,
                full_path: path,
            })
        } else if let Some(path) = config_value.as_str_forced() {
            Some(Self {
                path: path.clone(),
                package: None,
                full_path: path,
            })
        } else {
            error_handler
                .with_expected(vec!["string", "table"])
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            None
        }
    }

    pub fn as_config_value(&self) -> ConfigValue {
        if let Some(package) = &self.package {
            let mut map = HashMap::new();
            map.insert(
                "path".to_string(),
                ConfigValue::from_str(&self.path).expect("path should be a string"),
            );
            map.insert(
                "package".to_string(),
                ConfigValue::from_str(package).expect("package should be a string"),
            );
            ConfigValue::new(
                ConfigSource::Null,
                ConfigScope::Null,
                Some(Box::new(ConfigData::Mapping(map))),
            )
        } else {
            ConfigValue::from_str(&self.full_path).expect("full_path should be a string")
        }
    }

    pub fn is_package(&self) -> bool {
        self.package.is_some() || PathBuf::from(&self.full_path).starts_with(package_root_path())
    }

    pub fn package_path(&self) -> Option<PathBuf> {
        if let Some(package) = &self.package {
            return package_path_from_handle(package);
        }

        None
    }

    pub fn is_valid(&self) -> bool {
        !self.full_path.is_empty() && self.full_path.starts_with('/')
    }

    pub fn starts_with(&self, path_entry: &PathEntryConfig) -> bool {
        if !self.is_valid() {
            return false;
        }

        PathBuf::from(&self.full_path).starts_with(&path_entry.full_path)
    }

    pub fn starts_with_path(&self, path: PathBuf) -> bool {
        if !self.is_valid() {
            return false;
        }

        PathBuf::from(&self.full_path).starts_with(path)
    }

    pub fn includes_path(&self, path: PathBuf) -> bool {
        if !self.is_valid() {
            return false;
        }

        PathBuf::from(&path).starts_with(&self.full_path)
    }

    pub fn replace(&mut self, path_from: &PathEntryConfig, path_to: &PathEntryConfig) -> bool {
        if self.starts_with(path_from) {
            let new_full_path = format!(
                "{}/{}",
                path_to.full_path,
                PathBuf::from(&self.full_path)
                    .strip_prefix(&path_from.full_path)
                    .unwrap()
                    .display(),
            );
            if let Some(package) = path_to.package.clone() {
                if let Some(package_path) = package_path_from_handle(&package) {
                    self.full_path = new_full_path;
                    self.package = Some(package);
                    self.path = PathBuf::from(&self.full_path)
                        .strip_prefix(&package_path)
                        .unwrap()
                        .display()
                        .to_string();

                    return true;
                }
            } else {
                self.full_path = new_full_path;
                self.package = None;
                self.path.clone_from(&self.full_path);

                return true;
            }
        }
        false
    }
}
