use std::collections::HashMap;

use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use tera::Context;
use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::utils::config_template_context;
use crate::internal::config::utils::render_config_template;
use crate::internal::config::utils::tera_render_error_message;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

#[derive(Default, Debug, Deserialize, Clone)]
pub struct SuggestCloneConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    repositories: Vec<SuggestCloneRepositoryConfig>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template_file: String,
}

impl Empty for SuggestCloneConfig {
    fn is_empty(&self) -> bool {
        self.repositories.is_empty() && self.template.is_empty() && self.template_file.is_empty()
    }
}

impl Serialize for SuggestCloneConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if !self.repositories.is_empty() {
            self.repositories.serialize(serializer)
        } else if !self.template.is_empty() || !self.template_file.is_empty() {
            let mut map = HashMap::new();
            if !self.template.is_empty() {
                map.insert("template".to_string(), self.template.clone());
            } else if !self.template_file.is_empty() {
                map.insert("template_file".to_string(), self.template_file.clone());
            }
            map.serialize(serializer)
        } else {
            serializer.serialize_none()
        }
    }
}

impl SuggestCloneConfig {
    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        if let Some(config_value) = config_value {
            // We can filter by values provided by the repository, as this is only
            // a repository-scoped configuration
            if let Some(config_value) = config_value.select_scope(&ConfigScope::Workdir) {
                return Self::parse_config_value(config_value);
            }
        }

        Self::default()
    }

    fn parse_config_value(config_value: ConfigValue) -> Self {
        if let Some(array) = config_value.as_array() {
            return Self {
                repositories: array
                    .iter()
                    .filter_map(SuggestCloneRepositoryConfig::from_config_value)
                    .collect(),
                template: "".to_string(),
                template_file: "".to_string(),
            };
        }

        if let Some(table) = config_value.as_table() {
            if let Some(array) = table.get("repositories") {
                if let Some(array) = array.as_array() {
                    return Self {
                        repositories: array
                            .iter()
                            .filter_map(SuggestCloneRepositoryConfig::from_config_value)
                            .collect(),
                        template: "".to_string(),
                        template_file: "".to_string(),
                    };
                }
            }

            if let Some(value) = table.get("template") {
                if let Some(value) = value.as_str_forced() {
                    return Self {
                        repositories: vec![],
                        template: value.to_string(),
                        template_file: "".to_string(),
                    };
                }
            } else if let Some(value) = table.get("template_file") {
                if let Some(filepath) = value.as_str_forced() {
                    return Self {
                        repositories: vec![],
                        template: "".to_string(),
                        template_file: filepath.to_string(),
                    };
                }
            }
        }

        Self::default()
    }

    pub fn repositories(&self) -> Vec<SuggestCloneRepositoryConfig> {
        self.repositories_in_context(".")
    }

    pub fn repositories_in_context(&self, path: &str) -> Vec<SuggestCloneRepositoryConfig> {
        let context = config_template_context(path);
        self.repositories_with_context(&context)
    }

    fn repositories_with_context(
        &self,
        template_context: &Context,
    ) -> Vec<SuggestCloneRepositoryConfig> {
        if !self.repositories.is_empty() {
            return self.repositories.clone();
        }

        let mut template = Tera::default();
        if !self.template.is_empty() {
            if let Err(err) = template.add_raw_template("suggest_clone", &self.template) {
                omni_warning!(tera_render_error_message(err));
                omni_warning!("suggest_clone will be ignored");
                return vec![];
            }
        } else if !self.template_file.is_empty() {
            if let Err(err) = template.add_template_file(&self.template_file, None) {
                omni_warning!(tera_render_error_message(err));
                omni_warning!("suggest_clone will be ignored");
                return vec![];
            }
        }

        if !template.templates.is_empty() {
            match render_config_template(&template, template_context) {
                Ok(value) => {
                    // Load the template as config value
                    let config_value = ConfigValue::from_str(&value);
                    // Parse the config value into an object of this type
                    let suggest_clone = Self::parse_config_value(config_value);
                    // In case this is recursive for some reason...
                    return suggest_clone.repositories_with_context(template_context);
                }
                Err(err) => {
                    omni_warning!(tera_render_error_message(err));
                    omni_warning!("suggest_clone will be ignored");
                }
            }
        }

        vec![]
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SuggestCloneTypeEnum {
    #[serde(rename = "package")]
    Package,
    #[serde(rename = "worktree")]
    Worktree,
}

impl FromStr for SuggestCloneTypeEnum {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "package" => Ok(Self::Package),
            "worktree" => Ok(Self::Worktree),
            _ => Err(format!("Invalid: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuggestCloneRepositoryConfig {
    pub handle: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    pub clone_type: SuggestCloneTypeEnum,
}

impl SuggestCloneRepositoryConfig {
    pub(super) fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
        if let Some(value) = config_value.as_str() {
            return Some(Self {
                handle: value.to_string(),
                args: vec![],
                clone_type: SuggestCloneTypeEnum::Package,
            });
        } else if let Some(table) = config_value.as_table() {
            let mut handle = None;
            if let Some(value) = table.get("handle") {
                if let Some(value) = value.as_str() {
                    handle = Some(value.to_string());
                }
            }

            handle.as_ref()?;

            let mut args = Vec::new();
            if let Some(value) = table.get("args") {
                if let Some(value) = value.as_str() {
                    if let Ok(value) = shell_words::split(&value) {
                        args.extend(value);
                    }
                }
            }

            let mut clone_type = SuggestCloneTypeEnum::Package;
            if let Some(value) = table.get("clone_type") {
                if let Some(value) = value.as_str() {
                    if let Ok(value) = SuggestCloneTypeEnum::from_str(&value) {
                        clone_type = value;
                    }
                }
            }

            return Some(Self {
                handle: handle.unwrap(),
                args,
                clone_type,
            });
        }

        None
    }

    pub fn clone_as_package(&self) -> bool {
        self.clone_type == SuggestCloneTypeEnum::Package
    }
}
