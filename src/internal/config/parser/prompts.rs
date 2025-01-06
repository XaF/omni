use serde::Deserialize;
use serde::Serialize;

use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::cache::PromptsCache;
use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::config::ConfigValue;
use crate::internal::git_env;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

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
    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        if let Some(config_value) = config_value {
            if let Some(array) = config_value.as_array() {
                let prompts = array
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, config_value)| {
                        PromptConfig::from_config_value(
                            config_value,
                            &error_handler.with_index(idx),
                        )
                    })
                    .collect();

                return Self { prompts };
            } else {
                error_handler
                    .with_expected("array")
                    .with_actual(config_value)
                    .error(ConfigErrorKind::InvalidValueType);
            }
        }

        Self::default()
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
    pub fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        // We need to have an id and a prompt:
        // - id is used to identify the answer to the prompt
        // - prompt is the message that will be displayed to the user

        let id = match config_value
            .get_as_str_or_none("id", &error_handler.with_key("id"))
            .map(|id| id.trim().to_string())
        {
            Some(id) if id.is_empty() => {
                error_handler
                    .with_key("id")
                    .error(ConfigErrorKind::EmptyKey);

                None
            }
            Some(id) => Some(id),
            None => {
                error_handler
                    .with_key("id")
                    .error(ConfigErrorKind::MissingKey);

                None
            }
        }?;

        let prompt = match config_value
            .get_as_str_or_none("prompt", &error_handler.with_key("prompt"))
            .map(|prompt| prompt.trim().to_string())
        {
            Some(prompt) if prompt.is_empty() => {
                error_handler
                    .with_key("prompt")
                    .error(ConfigErrorKind::EmptyKey);

                None
            }
            Some(prompt) => Some(prompt),
            None => {
                error_handler
                    .with_key("prompt")
                    .error(ConfigErrorKind::MissingKey);

                None
            }
        }?;

        let prompt_type = PromptType::from_config_value(config_value, error_handler)?;

        // if is used to conditionally prompt the user.
        let if_condition = config_value.get_as_str_forced("if");

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
        let scope = PromptScope::from_config_value(config_value, error_handler);

        Some(Self {
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
                let config_value = match ConfigValue::from_str(&value) {
                    Ok(value) => value,
                    Err(err) => {
                        return Err(format!(
                            "failed to parse prompt {} as yaml: {}",
                            &self.id, err
                        ))
                    }
                };

                let error_handler = ConfigErrorHandler::new();
                match Self::from_config_value(&config_value, &error_handler) {
                    Some(prompt) => Ok(prompt),
                    None => Err(format!(
                        "failed to parse prompt {} from rendered template: {}",
                        &self.id,
                        error_handler
                            .last_error()
                            .map(|err| err.message().to_string())
                            .unwrap_or("unknown error".to_string())
                    )),
                }
            }
            Err(err) => Err(tera_render_error_message(err)),
        }
    }

    pub fn prompt(&self) -> bool {
        self.prompt_type.prompt(
            self.id.as_str(),
            self.prompt.as_str(),
            self.default.clone(),
            self.scope,
        )
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, Copy)]
pub enum PromptScope {
    #[default]
    #[serde(rename = "repo", alias = "repository")]
    Repository,
    #[serde(rename = "org", alias = "organization")]
    Organization,
}

impl PromptScope {
    pub fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let scope = match config_value.get_as_str_or_none("scope", &error_handler.with_key("scope"))
        {
            Some(scope) => scope.trim().to_lowercase(),
            None => return Self::default(),
        };

        match scope.as_str() {
            "repo" | "repository" => Self::Repository,
            "org" | "organization" => Self::Organization,
            _ => {
                error_handler
                    .with_key("scope")
                    .with_expected("repo or org")
                    .with_actual(scope)
                    .error(ConfigErrorKind::InvalidValue);

                Self::default()
            }
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
    Choice { choices: PromptChoicesConfig },
    #[serde(rename = "multichoice", alias = "choices", alias = "multiselect")]
    MultiChoice { choices: PromptChoicesConfig },
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
    pub fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let prompt_type = match config_value
            .get_as_str_or_none("type", &error_handler.with_key("type"))
            .map(|s| s.trim().to_lowercase())
        {
            Some(prompt_type) if prompt_type.is_empty() => {
                error_handler
                    .with_key("type")
                    .error(ConfigErrorKind::EmptyKey);

                return Some(Self::default());
            }
            Some(prompt_type) => prompt_type,
            None => {
                error_handler
                    .with_key("type")
                    .error(ConfigErrorKind::MissingKey);

                return Some(Self::default());
            }
        };

        match prompt_type.as_str() {
            "text" => Some(Self::Text),
            "password" => Some(Self::Password),
            "confirm" | "boolean" => Some(Self::Confirm),
            "choice" | "select" | "choices" | "multichoice" | "multiselect" => {
                if let Some(choices) = config_value.get("choices") {
                    let choices = PromptChoicesConfig::from_config_value(
                        &choices,
                        &error_handler.with_key("choices"),
                    )?;

                    return match prompt_type.as_str() {
                        "choice" | "select" => Some(Self::Choice { choices }),
                        "choices" | "multichoice" | "multiselect" => {
                            Some(Self::MultiChoice { choices })
                        }
                        _ => unreachable!("invalid prompt type for choices"),
                    };
                }

                error_handler
                    .with_key("choices")
                    .error(ConfigErrorKind::MissingKey);

                None
            }
            "int" => {
                let min =
                    config_value.get_as_integer_or_none("min", &error_handler.with_key("min"));
                let max =
                    config_value.get_as_integer_or_none("max", &error_handler.with_key("max"));

                Some(Self::Int { min, max })
            }
            "float" => {
                let min = config_value.get_as_float_or_none("min", &error_handler.with_key("min"));
                let max = config_value.get_as_float_or_none("max", &error_handler.with_key("max"));

                Some(Self::Float { min, max })
            }
            _ => {
                error_handler
                    .with_key("type")
                    .with_expected("text, password, confirm, choice, multichoice, int, or float")
                    .with_actual(prompt_type)
                    .error(ConfigErrorKind::InvalidValue);

                None
            }
        }
    }

    pub fn is_default(&self) -> bool {
        matches!(self, Self::Text)
    }

    pub fn prompt(
        &self,
        id: &str,
        prompt: &str,
        default: serde_yaml::Value,
        scope: PromptScope,
    ) -> bool {
        // Override the default value with the cached answer if there is one
        // for the current scope; otherwise, use the default value.
        let default = match PromptsCache::get().answers(".").get(id) {
            Some(answer) => answer.clone(),
            None => default,
        };

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
                let choices = match choices.choices() {
                    Ok(choices) => choices,
                    Err(err) => {
                        omni_warning!(format!(
                            "failed to parse choices for prompt {}: {}",
                            id, err
                        ));
                        return false;
                    }
                };

                let mut question = requestty::Question::select(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt)
                    .choices(choices.clone());

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
                let choices = match choices.choices() {
                    Ok(choices) => choices,
                    Err(err) => {
                        omni_warning!(format!(
                            "failed to parse choices for prompt {}: {}",
                            id, err
                        ));
                        return false;
                    }
                };

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
                        // Make sure that min and max are cloned since the
                        // closure will outlive the current block
                        #[allow(clippy::clone_on_copy)]
                        let min = min.clone();
                        #[allow(clippy::clone_on_copy)]
                        let max = max.clone();

                        let errmsg = match (min, max) {
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

                        if let Some(min) = min {
                            if answer < min {
                                return Err(errmsg.clone());
                            }
                        }

                        if let Some(max) = max {
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
                        // Make sure that min and max are cloned since the
                        // closure will outlive the current block
                        #[allow(clippy::clone_on_copy)]
                        let min = min.clone();
                        #[allow(clippy::clone_on_copy)]
                        let max = max.clone();

                        let errmsg = match (min, max) {
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

                        if let Some(min) = min {
                            if answer < min {
                                return Err(errmsg.clone());
                            }
                        }

                        if let Some(max) = max {
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

        let git = git_env(".");
        let (scope_org, scope_repo) = match git.url() {
            Some(url) => (
                url.owner,
                match scope {
                    PromptScope::Repository => Some(url.name),
                    PromptScope::Organization => None,
                },
            ),
            None => {
                // TODO: make it work for any workdir by storing for the workdir id
                //       instead of the org and repo
                omni_warning!("prompts are not available outside of a git repository");
                return false;
            }
        };

        let scope_org = match scope_org {
            Some(org) => org,
            None => {
                omni_warning!("unable to determine the organization of the repository");
                return false;
            }
        };

        let serde_yaml_answer = match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::String(answer) => serde_yaml::to_value(answer),
                requestty::Answer::Bool(answer) => serde_yaml::to_value(answer),
                requestty::Answer::Int(answer) => serde_yaml::to_value(answer),
                requestty::Answer::Float(answer) => serde_yaml::to_value(answer),
                requestty::Answer::ListItem(answer) => {
                    let choices = match self {
                        Self::Choice { choices } => match choices.choices() {
                            Ok(choices) => choices,
                            Err(_err) => return false,
                        },
                        _ => {
                            omni_warning!("invalid prompt type");
                            return false;
                        }
                    };

                    let selected_choice = match choices.get(answer.index) {
                        Some(choice) => choice.id.to_string(),
                        None => {
                            omni_warning!("invalid choice index");
                            return false;
                        }
                    };

                    serde_yaml::to_value(selected_choice)
                }
                requestty::Answer::ListItems(answers) => {
                    let choices = match self {
                        Self::MultiChoice { choices } => match choices.choices() {
                            Ok(choices) => choices,
                            Err(_err) => return false,
                        },
                        _ => {
                            omni_warning!("invalid prompt type");
                            return false;
                        }
                    };

                    let selected_choices = answers
                        .iter()
                        .filter_map(|answer| choices.get(answer.index))
                        .map(|choice| choice.id.to_string())
                        .collect::<Vec<_>>();

                    serde_yaml::to_value(selected_choices)
                }
                _ => unimplemented!(),
            },
            Err(err) => {
                println!("{}", format!("[✘] {:?}", err).red());
                return false;
            }
        };

        let serde_yaml_answer = match serde_yaml_answer {
            Ok(serde_yaml_answer) => serde_yaml_answer,
            Err(err) => {
                omni_warning!(format!("failed to serialize answer: {}", err));
                return false;
            }
        };

        if let Err(err) =
            PromptsCache::get().add_answer(id, scope_org, scope_repo, serde_yaml_answer)
        {
            omni_warning!(format!("failed to update cache: {}", err));
            false
        } else {
            true
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub enum PromptChoicesConfig {
    ChoicesAsArray(Vec<PromptChoiceConfig>),
    ChoicesAsString(String),
}

impl PromptChoicesConfig {
    pub fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        if let Some(array) = config_value.as_array() {
            let choices = array
                .iter()
                .enumerate()
                .filter_map(|(idx, value)| {
                    PromptChoiceConfig::from_config_value(value, &error_handler.with_index(idx))
                })
                .collect::<Vec<PromptChoiceConfig>>();

            if choices.is_empty() {
                error_handler.error(ConfigErrorKind::EmptyKey);
                None
            } else {
                Some(Self::ChoicesAsArray(choices))
            }
        } else if let Some(string) = config_value.as_str_forced() {
            Some(Self::ChoicesAsString(string.to_string()))
        } else {
            error_handler
                .with_expected("array or template of array")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);
            None
        }
    }

    pub fn choices(&self) -> Result<Vec<PromptChoiceConfig>, String> {
        match self {
            Self::ChoicesAsArray(choices) => Ok(choices.clone()),
            Self::ChoicesAsString(template) => match ConfigValue::from_str(template) {
                Ok(config_value) => {
                    let choices = match config_value.as_array() {
                        Some(choices) => choices,
                        None => {
                            return Err("choices template must be an array".to_string());
                        }
                    };

                    let choices = choices
                        .iter()
                        .filter_map(|value| {
                            PromptChoiceConfig::from_config_value(
                                value,
                                &ConfigErrorHandler::noop(),
                            )
                        })
                        .collect::<Vec<PromptChoiceConfig>>();

                    if choices.is_empty() {
                        Err("choices template must be a non-empty array".to_string())
                    } else {
                        Ok(choices)
                    }
                }
                Err(err) => Err(format!("failed to parse choices template as yaml: {}", err)),
            },
        }
    }
}

impl Serialize for PromptChoicesConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            Self::ChoicesAsArray(choices) => choices.serialize(serializer),
            Self::ChoicesAsString(template) => template.serialize(serializer),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptChoiceConfig {
    pub id: String,
    pub choice: String,
}

impl PromptChoiceConfig {
    pub fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
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
                _ => {
                    error_handler
                        .with_expected("id or choice")
                        .with_actual(config_value)
                        .error(ConfigErrorKind::MissingKey);

                    None
                }
            }
        } else if let Some(choice) = config_value.as_str_forced() {
            Some(Self {
                id: choice.to_string(),
                choice: choice.to_string(),
            })
        } else {
            error_handler
                .with_expected("table or string")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            None
        }
    }
}

impl From<PromptChoiceConfig> for String {
    fn from(choice: PromptChoiceConfig) -> String {
        choice.choice
    }
}

impl From<&PromptChoiceConfig> for String {
    fn from(choice: &PromptChoiceConfig) -> String {
        choice.choice.clone()
    }
}
