use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::config_value::ConfigData;
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
    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let append = match config_value.get_as_array("append") {
            Some(value) => value
                .iter()
                .map(PathEntryConfig::from_config_value)
                .collect(),
            None => vec![],
        };

        let prepend = match config_value.get_as_array("prepend") {
            Some(value) => value
                .iter()
                .map(PathEntryConfig::from_config_value)
                .collect(),
            None => vec![],
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

    pub fn from_config_value(config_value: &ConfigValue) -> Self {
        if config_value.is_table() {
            let path = config_value
                .get_as_str("path")
                .unwrap_or("".to_string())
                .to_string();

            if !path.starts_with('/') {
                if let Some(package) = config_value.get("package") {
                    let package = package.as_str().unwrap();
                    if let Some(package_path) = package_path_from_handle(&package) {
                        let mut full_path = package_path;
                        if !path.is_empty() {
                            full_path = full_path.join(path.clone());
                        }

                        return Self {
                            path: path.clone(),
                            package: Some(package.to_string()),
                            full_path: full_path.to_str().unwrap().to_string(),
                        };
                    }
                }
            }

            Self {
                path: path.clone(),
                package: None,
                full_path: path,
            }
        } else {
            let path = config_value.as_str().unwrap_or("".to_string()).to_string();
            Self {
                path: path.clone(),
                package: None,
                full_path: path,
            }
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

    pub fn as_string(&self) -> String {
        self.full_path.clone()
    }

    pub fn starts_with(&self, path_entry: &PathEntryConfig) -> bool {
        if !self.is_valid() {
            return false;
        }

        PathBuf::from(&self.full_path).starts_with(&path_entry.full_path)
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
                self.path = self.full_path.clone();

                return true;
            }
        }
        false
    }
}
