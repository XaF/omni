use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;
use tera::Context;
use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

#[derive(Default, Debug, Deserialize, Clone)]
pub struct SuggestConfig {
    #[serde(skip_serializing_if = "ConfigValue::is_null")]
    pub config: ConfigValue,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template_file: String,
}

impl Empty for SuggestConfig {
    fn is_empty(&self) -> bool {
        self.config.is_null() && self.template.is_empty() && self.template_file.is_empty()
    }
}

impl Serialize for SuggestConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if !self.config.is_null() {
            self.config.serialize(serializer)
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

impl SuggestConfig {
    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        if let Some(config_value) = config_value {
            // We can filter by values provided by the repository, as this is only
            // a repository-scoped configuration
            if let Some(config_value) = config_value.select_scope(&ConfigScope::Workdir) {
                return Self::parse_config_value(config_value, error_key, errors);
            }
        }

        Self::default()
    }

    fn parse_config_value(
        config_value: ConfigValue,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        if let Some(table) = config_value.as_table() {
            if let Some(config) = table.get("config") {
                return Self {
                    config: config.clone(),
                    template: "".to_string(),
                    template_file: "".to_string(),
                };
            }

            if let Some(value) = table.get("template") {
                if let Some(value) = value.as_str_forced() {
                    return Self {
                        config: ConfigValue::default(),
                        template: value.to_string(),
                        template_file: "".to_string(),
                    };
                } else {
                    errors.push(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.template", error_key),
                        actual: value.as_serde_yaml(),
                        expected: "string".to_string(),
                    });
                }
            } else if let Some(value) = table.get("template_file") {
                if let Some(filepath) = value.as_str_forced() {
                    return Self {
                        config: ConfigValue::default(),
                        template: "".to_string(),
                        template_file: filepath.to_string(),
                    };
                } else {
                    errors.push(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.template_file", error_key),
                        actual: value.as_serde_yaml(),
                        expected: "string".to_string(),
                    });
                }
            }
        }

        Self {
            config: config_value.clone(),
            template: "".to_string(),
            template_file: "".to_string(),
        }
    }

    pub fn config(&self) -> ConfigValue {
        self.config_in_context(".")
    }

    pub fn config_in_context(&self, path: &str) -> ConfigValue {
        let context = config_template_context(path);
        self.config_with_context(&context)
    }

    fn config_with_context(&self, template_context: &Context) -> ConfigValue {
        if !self.config.is_null() {
            return self.config.clone();
        }

        let mut template = Tera::default();
        if !self.template.is_empty() {
            if let Err(err) = template.add_raw_template("suggest_config", &self.template) {
                omni_warning!(tera_render_error_message(err));
                omni_warning!("suggest_config will be ignored");
                return ConfigValue::default();
            }
        } else if !self.template_file.is_empty() {
            if let Err(err) = template.add_template_file(&self.template_file, None) {
                omni_warning!(tera_render_error_message(err));
                omni_warning!("suggest_config will be ignored");
                return ConfigValue::default();
            }
        }

        if !template.templates.is_empty() {
            match render_config_template(&template, template_context) {
                Ok(value) => {
                    // Load the template as config value
                    match ConfigValue::from_str(&value) {
                        Ok(value) => {
                            // Parse the config value into an object of this type
                            let suggest = Self::parse_config_value(value, "", &mut vec![]);
                            // In case this is recursive for some reason...
                            return suggest.config_with_context(template_context);
                        }
                        Err(err) => {
                            omni_warning!(format!(
                                "Failed to parse suggest_config template: {}",
                                err
                            ));
                            omni_warning!("suggest_config will be ignored");
                        }
                    }
                }
                Err(err) => {
                    omni_warning!(tera_render_error_message(err));
                    omni_warning!("suggest_config will be ignored");
                }
            }
        }

        ConfigValue::default()
    }
}
