use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptsConfig {
    pub prompts: Vec<PromptConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptConfig {
    pub id: String,
    pub prompt: String,
    pub default: Option<String>,
    pub prompt_type: PromptType,
    pub scope: PromptScope,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub enum PromptScope {
    #[default]
    Repository,
    Organization,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PromptType {
    Text,
    Password,
    Confirm,
    Choice(Vec<PromptChoiceConfig>),
    Int(Option<i64>, Option<i64>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptChoiceConfig {
    pub id: String,
    pub text: String,
}
