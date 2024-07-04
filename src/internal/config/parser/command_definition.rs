use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils as cache_utils;
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct SyntaxOptArg {
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alt_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub required: bool,
}

impl SyntaxOptArg {
    /// Split a string over a separator, but ignore the separator if it is inside brackets
    /// (i.e. '{' and '}', or '[' and ']', or '(' and ')')
    fn bg_smart_split(s: &str, sep: char, max_splits: Option<usize>) -> Vec<&str> {
        let mut parts = vec![];
        let mut brackets = vec![];
        let mut start = 0;

        for (i, c) in s.char_indices() {
            if c == '{' || c == '[' || c == '(' {
                brackets.push(c);
            } else if c == '}' || c == ']' || c == ')' {
                if let Some(last_bracket) = brackets.last() {
                    if (c == '}' && *last_bracket == '{')
                        || (c == ']' && *last_bracket == '[')
                        || (c == ')' && *last_bracket == '(')
                    {
                        brackets.pop();
                    }
                }
            } else if c == sep && brackets.is_empty() {
                parts.push(&s[start..i]);
                start = i + 1;

                if let Some(max_splits) = max_splits {
                    if parts.len() + 1 == max_splits {
                        break;
                    }
                }
            }
        }

        parts.push(&s[start..]);
        parts
    }

    fn smart_split(s: &str, sep: char) -> Vec<String> {
        Self::bg_smart_split(s, sep, None)
            .iter()
            .map(|part| part.to_string())
            .collect()
    }

    fn smart_splitn(s: &str, n: usize, sep: char) -> Vec<String> {
        Self::bg_smart_split(s, sep, Some(n))
            .iter()
            .map(|part| part.to_string())
            .collect()
    }

    pub fn new(name: String, desc: Option<String>, required: bool) -> Self {
        let mut alt_names = vec![];
        let mut placeholder = None;

        // Split the name over commas
        let split_names = Self::smart_split(&name, ',');
        for split_name in split_names {
            // Remove leading and trailing whitespaces
            let split_name = split_name.trim();

            // Split over space
            let mut parts = Self::smart_splitn(&split_name, 2, ' ').into_iter();

            // Get the first part, which is the name, and add it to the alt_names
            let cur_name = parts.next().unwrap();
            alt_names.push(cur_name.to_string());

            // If there is a second part, it is the placeholder, so if the
            // placeholder is not already set, set it
            if placeholder.is_none() {
                if let Some(placeholder_str) = parts.next() {
                    placeholder = Some(placeholder_str.to_string());
                }
            }
        }

        // Pop the first element of alt_names and set it as the name
        let name = if !alt_names.is_empty() {
            alt_names.remove(0)
        } else {
            name
        };

        Self {
            name,
            alt_names,
            placeholder,
            desc,
            required,
        }
    }

    pub fn new_option_with_desc(name: &str, desc: &str) -> Self {
        Self::new(name.to_string(), Some(desc.to_string()), false)
    }

    pub fn new_required_with_desc(name: &str, desc: &str) -> Self {
        Self::new(name.to_string(), Some(desc.to_string()), true)
    }

    pub fn usage(&self) -> String {
        let mut usage = self.name.clone();
        if let Some(placeholder) = &self.placeholder {
            usage.push(' ');
            usage.push_str(placeholder);
        }

        if !self.required {
            usage = format!("[{}]", usage);
        } else if self.is_positional() {
            usage = format!("<{}>", usage);
        }

        usage
    }

    pub fn long_usage(&self) -> String {
        let mut all_names = vec![self.name.clone()];
        all_names.extend(self.alt_names.clone());

        if let Some(placeholder) = &self.placeholder {
            all_names = all_names
                .iter()
                .map(|name| format!("{} {}", name, placeholder))
                .collect();
        }

        all_names.join(", ")
    }

    pub fn is_positional(&self) -> bool {
        !self.name.starts_with('-')
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

        Some(Self::new(name, desc, required.unwrap_or(false)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntaxoptarg_simple_option_with_desc() {
        let actual = SyntaxOptArg::new_option_with_desc("name", "desc");
        let expected = SyntaxOptArg {
            name: "name".to_string(),
            alt_names: vec![],
            placeholder: None,
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_simple_required_with_desc() {
        let actual = SyntaxOptArg::new_required_with_desc("name", "desc");
        let expected = SyntaxOptArg {
            name: "name".to_string(),
            alt_names: vec![],
            placeholder: None,
            desc: Some("desc".to_string()),
            required: true,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_complex_option_with_desc() {
        let actual = SyntaxOptArg::new_option_with_desc("{opt1, opt2}", "desc");
        let expected = SyntaxOptArg {
            name: "{opt1, opt2}".to_string(),
            alt_names: vec![],
            placeholder: None,
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_with_simple_placeholder() {
        let actual = SyntaxOptArg::new_option_with_desc("--opt OPT", "desc");
        let expected = SyntaxOptArg {
            name: "--opt".to_string(),
            alt_names: vec![],
            placeholder: Some("OPT".to_string()),
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_with_complex_placeholder() {
        let actual = SyntaxOptArg::new_option_with_desc("--opt {val1, val2}", "desc");
        let expected = SyntaxOptArg {
            name: "--opt".to_string(),
            alt_names: vec![],
            placeholder: Some("{val1, val2}".to_string()),
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_with_alt_name() {
        let actual = SyntaxOptArg::new_option_with_desc("--opt1, --opt2", "desc");
        let expected = SyntaxOptArg {
            name: "--opt1".to_string(),
            alt_names: vec!["--opt2".to_string()],
            placeholder: None,
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_with_alt_names() {
        let actual = SyntaxOptArg::new_option_with_desc("--opt1, --opt2,-o", "desc");
        let expected = SyntaxOptArg {
            name: "--opt1".to_string(),
            alt_names: vec!["--opt2".to_string(), "-o".to_string()],
            placeholder: None,
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_with_alt_names_and_same_placeholders() {
        let actual = SyntaxOptArg::new_option_with_desc("--opt1 OPT, --opt2 OPT,-o OPT", "desc");
        let expected = SyntaxOptArg {
            name: "--opt1".to_string(),
            alt_names: vec!["--opt2".to_string(), "-o".to_string()],
            placeholder: Some("OPT".to_string()),
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_with_alt_names_and_one_placeholders() {
        let actual = SyntaxOptArg::new_option_with_desc("--opt1, --opt2,-o OPT", "desc");
        let expected = SyntaxOptArg {
            name: "--opt1".to_string(),
            alt_names: vec!["--opt2".to_string(), "-o".to_string()],
            placeholder: Some("OPT".to_string()),
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_with_alt_names_and_different_placeholders() {
        let actual = SyntaxOptArg::new_option_with_desc("--opt1 OPT1, --opt2 OPT2,-o OPT3", "desc");
        let expected = SyntaxOptArg {
            name: "--opt1".to_string(),
            alt_names: vec!["--opt2".to_string(), "-o".to_string()],
            placeholder: Some("OPT1".to_string()),
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syntaxoptarg_option_usage() {
        let opt = SyntaxOptArg {
            name: "--opt".to_string(),
            alt_names: vec!["--opt2".to_string(), "-o".to_string()],
            placeholder: Some("OPT".to_string()),
            desc: Some("desc".to_string()),
            required: false,
        };

        assert_eq!(opt.usage(), "[--opt OPT]");
    }

    #[test]
    fn test_syntaxoptarg_required_usage() {
        let opt = SyntaxOptArg {
            name: "--opt".to_string(),
            alt_names: vec!["--opt2".to_string(), "-o".to_string()],
            placeholder: Some("OPT".to_string()),
            desc: Some("desc".to_string()),
            required: true,
        };

        assert_eq!(opt.usage(), "--opt OPT");
    }

    #[test]
    fn test_syntaxoptarg_positional_usage() {
        let opt = SyntaxOptArg {
            name: "opt".to_string(),
            alt_names: vec![],
            placeholder: None,
            desc: Some("desc".to_string()),
            required: true,
        };

        assert_eq!(opt.usage(), "<opt>");
    }
}
