use serde::Deserialize;
use serde::Serialize;

use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::colors::StringColor;

#[derive(Default, Debug, Deserialize, Clone)]
pub struct PromptsConfig {
    #[serde(flatten)]
    pub prompts: Vec<PromptConfig>,
}

impl Empty for PromptsConfig {
    fn is_empty(&self) -> bool {
        self.prompts.is_empty()
    }
}

impl Serialize for PromptsConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.prompts.serialize(serializer)
    }
}

impl PromptsConfig {
    pub fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        if let Some(config_value) = config_value {
            if let Some(array) = config_value.as_array() {
                let prompts = array
                    .iter()
                    .filter_map(|config_value| PromptConfig::from_config_value(config_value).ok())
                    .collect();

                return Self { prompts };
            }
        }

        Self::default()
    }

    pub fn prompt_all(&self) -> bool {
        for prompt in &self.prompts {
            let continue_prompting = prompt.prompt();
            if !continue_prompting {
                return false;
            }
        }
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &PromptConfig> {
        self.prompts.iter()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptConfig {
    pub id: String,
    pub prompt: String,
    #[serde(
        skip_serializing_if = "serde_yaml::Value::is_null",
        default = "serde_yaml::Value::default"
    )]
    pub default: serde_yaml::Value,
    #[serde(
        flatten,
        skip_serializing_if = "PromptType::is_default",
        default = "PromptType::default"
    )]
    pub prompt_type: PromptType,
    #[serde(
        skip_serializing_if = "PromptScope::is_default",
        default = "PromptScope::default"
    )]
    pub scope: PromptScope,
    #[serde(skip_serializing_if = "Option::is_none", rename = "if")]
    pub if_condition: Option<String>,
}

impl PromptConfig {
    pub fn from_config_value(config_value: &ConfigValue) -> Result<Self, String> {
        let id = match config_value.get_as_str_forced("id") {
            Some(id) => id.trim().to_string(),
            None => return Err("prompt id is required".to_string()),
        };

        let prompt = match config_value.get_as_str_forced("prompt") {
            Some(prompt) => prompt.trim().to_string(),
            None => return Err("prompt message is required".to_string()),
        };

        // We need to have an id and a prompt:
        // - id is used to identify the answer to the prompt
        // - prompt is the message that will be displayed to the user
        if id.is_empty() || prompt.is_empty() {
            return Err("prompt id and prompt message are required".to_string());
        }

        let prompt_type = match PromptType::from_config_value(config_value) {
            Some(prompt_type) => prompt_type,
            None => return Err("prompt type is required".to_string()),
        };

        // if is used to conditionally prompt the user.
        let if_condition = match config_value.get_as_str_forced("if") {
            Some(if_condition) => Some(if_condition),
            None => None,
        };

        // We keep the default value as a serde_yaml::Value so that we can
        // serialize it as a string if it's a string, or as a boolean if it's a
        // boolean, etc. and interpret it as the correct type when we use it
        // as default value for the prompt.
        let default = match config_value.get("default") {
            Some(default) => default.as_serde_yaml(),
            None => serde_yaml::Value::Null,
        };

        // Scope is used to determine how prompts answers are stored. For
        // example, if the scope is "repo", the prompt will be considered
        // as answered only for the current repository. If the scope is
        // "org", the prompt will be considered as answered for the whole
        // organization. If a repository has a prompt with the same id as
        // an organization prompt, the repository prompt will take
        // precedence and be re-asked, but won't override the organization
        // answer.
        let scope = PromptScope::from_config_value(config_value);

        Ok(Self {
            id,
            prompt,
            default,
            prompt_type,
            scope,
            if_condition,
        })
    }

    pub fn should_prompt(&self) -> bool {
        match &self.if_condition {
            Some(if_condition) => {
                let if_condition = if_condition.trim().to_lowercase();

                matches!(if_condition.as_str(), "true" | "yes" | "on" | "1")
            }
            None => true,
        }
    }

    pub fn in_context(&self) -> Result<Self, String> {
        let template_context = config_template_context(".");
        eprintln!("template_context: {:?}", template_context); // DEBUG

        // Dump self as yaml string using serde_yaml
        let yaml = match serde_yaml::to_string(self) {
            Ok(yaml) => yaml,
            Err(err) => {
                return Err(format!(
                    "failed to dump prompt {} as yaml: {}",
                    &self.id, err
                ))
            }
        };

        let mut template = Tera::default();
        let prompt_key = format!("prompt.{}", &self.id);
        if let Err(err) = template.add_raw_template(&prompt_key, yaml.as_str()) {
            return Err(tera_render_error_message(err));
        }

        match render_config_template(&template, &template_context) {
            Ok(value) => {
                // Load the template as config value
                let config_value = ConfigValue::from_str(&value);
                eprintln!("config_value: {:?}", config_value.as_yaml()); // DEBUG
                match Self::from_config_value(&config_value) {
                    Ok(prompt) => Ok(prompt),
                    Err(err) => Err(format!(
                        "failed to parse prompt {} from rendered template: {}",
                        &self.id, err
                    )),
                }
            }
            Err(err) => Err(tera_render_error_message(err)),
        }
    }

    pub fn prompt(&self) -> bool {
        self.prompt_type
            .prompt(self.id.as_str(), self.prompt.as_str(), self.default.clone())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub enum PromptScope {
    #[default]
    #[serde(rename = "repo", alias = "repository")]
    Repository,
    #[serde(rename = "org", alias = "organization")]
    Organization,
}

impl PromptScope {
    pub fn from_config_value(config_value: &ConfigValue) -> Self {
        let scope = match config_value.get_as_str_forced("scope") {
            Some(scope) => scope.trim().to_lowercase(),
            None => return Self::default(),
        };

        match scope.as_str() {
            "repo" | "repository" => Self::Repository,
            "org" | "organization" => Self::Organization,
            _ => Self::default(),
        }
    }

    pub fn is_default(&self) -> bool {
        matches!(self, Self::Repository)
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum PromptType {
    #[default]
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "password")]
    Password,
    #[serde(rename = "confirm", alias = "boolean")]
    Confirm,
    #[serde(rename = "choice", alias = "select")]
    Choice {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        choices: Vec<PromptChoiceConfig>,
    },
    #[serde(rename = "multichoice", alias = "choices", alias = "multiselect")]
    MultiChoice {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        choices: Vec<PromptChoiceConfig>,
    },
    #[serde(rename = "int")]
    Int {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    #[serde(rename = "float")]
    Float {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
}

impl PromptType {
    pub fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
        let prompt_type = match config_value.get_as_str_forced("type") {
            Some(prompt_type) => prompt_type.trim().to_lowercase(),
            None => return Some(Self::default()),
        };

        match prompt_type.as_str() {
            "text" => Some(Self::Text),
            "password" => Some(Self::Password),
            "confirm" | "boolean" => Some(Self::Confirm),
            "choice" | "select" | "choices" | "multichoice" | "multiselect" => {
                if let Some(choices) = config_value.get("choices") {
                    if let Some(choices) = choices.as_array() {
                        let choices = choices
                            .iter()
                            .filter_map(PromptChoiceConfig::from_config_value)
                            .collect::<Vec<_>>();

                        if choices.is_empty() {
                            return None;
                        }

                        return match prompt_type.as_str() {
                            "choice" | "select" => Some(Self::Choice { choices }),
                            "choices" | "multichoice" | "multiselect" => {
                                Some(Self::MultiChoice { choices })
                            }
                            _ => None,
                        };
                    }
                }

                None
            }
            "int" => {
                let min = config_value.get_as_integer("min");
                let max = config_value.get_as_integer("max");

                Some(Self::Int { min, max })
            }
            "float" => {
                let min = config_value.get_as_float("min");
                let max = config_value.get_as_float("max");

                Some(Self::Float { min, max })
            }
            _ => None,
        }
    }

    pub fn is_default(&self) -> bool {
        matches!(self, Self::Text)
    }

    pub fn prompt(&self, id: &str, prompt: &str, default: serde_yaml::Value) -> bool {
        let question = match self {
            Self::Text => {
                let mut question = requestty::Question::input(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_str().map(|s| s.to_string()) {
                        question = question.default(default);
                    }
                }

                question.build()
            }
            Self::Password => requestty::Question::password(id)
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message(prompt)
                .build(),
            Self::Confirm => {
                let mut question = requestty::Question::confirm(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_bool() {
                        question = question.default(default);
                    }
                }

                question.build()
            }
            Self::Choice { choices } => {
                let mut question = requestty::Question::select(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt)
                    .choices(choices.iter().map(|choice| choice.choice.as_str()));

                if !default.is_null() {
                    if let Some(default) = default.as_i64() {
                        let default_index = default as usize;
                        if default_index < choices.len() {
                            question = question.default(default_index);
                        }
                    }

                    if let Some(default) = default.as_str().map(|s| s.to_string()) {
                        // Find the index of the default choice
                        if let Some(index) = choices.iter().position(|choice| choice.id == default)
                        {
                            question = question.default(index);
                        }
                    }
                }

                question.build()
            }
            Self::MultiChoice { choices } => {
                let mut choices_with_default = choices
                    .iter()
                    .map(|choice| (choice, false))
                    .collect::<Vec<_>>();

                if !default.is_null() {
                    let defaults = match default.clone() {
                        serde_yaml::Value::Sequence(defaults) => defaults,
                        serde_yaml::Value::String(_) => vec![default],
                        serde_yaml::Value::Number(ref number) if number.is_i64() => vec![default],
                        _ => vec![],
                    };

                    for default in defaults {
                        if let Some(default) = default.as_i64() {
                            let default_index = default as usize;
                            if default_index < choices.len() {
                                choices_with_default[default_index].1 = true;
                                continue;
                            }
                        }

                        if let Some(default) = default.as_str().map(|s| s.to_string()) {
                            // Find the index of the default choice
                            if let Some(index) =
                                choices.iter().position(|choice| choice.id == default)
                            {
                                choices_with_default[index].1 = true;
                                continue;
                            }
                        }
                    }
                }

                requestty::Question::multi_select(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt)
                    .choices_with_default(choices_with_default)
                    .build()
            }
            Self::Int { min, max } => {
                let mut question = requestty::Question::int(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_i64() {
                        question = question.default(default);
                    }
                }

                if min.is_some() || max.is_some() {
                    question = question.validate(|answer, _previous_answers| {
                        let errmsg = match (min.clone(), max.clone()) {
                            (Some(min), Some(max)) => {
                                format!("Answer must be between {} and {}", min, max)
                            }
                            (Some(min), None) => {
                                format!("Answer must be greater than or equal to {}", min)
                            }
                            (None, Some(max)) => {
                                format!("Answer must be lower than or equal to {}", max)
                            }
                            _ => unreachable!(),
                        };

                        if let Some(min) = min.clone() {
                            if answer < min {
                                return Err(errmsg.clone());
                            }
                        }

                        if let Some(max) = max.clone() {
                            if answer > max {
                                return Err(errmsg.clone());
                            }
                        }

                        Ok(())
                    });
                }

                question.build()
            }
            Self::Float { min, max } => {
                let mut question = requestty::Question::float(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_f64() {
                        question = question.default(default);
                    }
                }

                if min.is_some() || max.is_some() {
                    question = question.validate(|answer, _previous_answers| {
                        let errmsg = match (min.clone(), max.clone()) {
                            (Some(min), Some(max)) => {
                                format!("Answer must be between {} and {}", min, max)
                            }
                            (Some(min), None) => {
                                format!("Answer must be greater than or equal to {}", min)
                            }
                            (None, Some(max)) => {
                                format!("Answer must be lower than or equal to {}", max)
                            }
                            _ => unreachable!(),
                        };

                        if let Some(min) = min.clone() {
                            if answer < min {
                                return Err(errmsg.clone());
                            }
                        }

                        if let Some(max) = max.clone() {
                            if answer > max {
                                return Err(errmsg.clone());
                            }
                        }

                        Ok(())
                    });
                }

                question.build()
            }
        };

        match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                // TODO: password, add default if not provided
                _ => {
                    eprintln!("Unhandled answer type: {:?}", answer);
                    true
                }
            },
            Err(err) => {
                println!("{}", format!("[✘] {:?}", err).red());
                false
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptChoiceConfig {
    pub id: String,
    pub choice: String,
}

impl PromptChoiceConfig {
    pub fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
        if let Some(table) = config_value.as_table() {
            let id = table.get("id").and_then(|id| id.as_str());
            let choice = table.get("choice").and_then(|choice| choice.as_str());

            match (id, choice) {
                (Some(id), Some(choice)) => Some(Self { id, choice }),
                (Some(id), None) => Some(Self {
                    id: id.clone(),
                    choice: id,
                }),
                (None, Some(choice)) => Some(Self {
                    id: choice.clone(),
                    choice,
                }),
                _ => None,
            }
        } else if let Some(choice) = config_value.as_str_forced() {
            Some(Self {
                id: choice.to_string(),
                choice: choice.to_string(),
            })
        } else {
            None
        }
    }
}

impl Into<String> for &PromptChoiceConfig {
    fn into(self) -> String {
        self.choice.clone()
    }
}
