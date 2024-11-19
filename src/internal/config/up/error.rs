use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("execution error: {0}")]
    Exec(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("cache error: {0}")]
    Cache(String),
    #[error("tap in use")]
    HomebrewTapInUse,
    #[error("{}", match .1 {
        Some((step, total)) => format!("step {}/{} '{}' failed", step, total, .0),
        None => format!("step '{}' failed", .0)
    })]
    StepFailed(String, Option<(usize, usize)>),
}

impl UpError {
    pub fn message(&self) -> String {
        match self {
            UpError::Config(message) => message.clone(),
            UpError::Exec(message) => message.clone(),
            UpError::Timeout(message) => message.clone(),
            UpError::Cache(message) => message.clone(),
            UpError::HomebrewTapInUse => "tap in use".to_string(),
            UpError::StepFailed(message, _) => message.clone(),
        }
    }

    // fn error_type(&self) -> String {
    // match self {
    // UpError::Config(_) => "configuration error".to_string(),
    // UpError::Exec(_) => "execution error".to_string(),
    // UpError::Timeout(_) => "timeout".to_string(),
    // UpError::HomebrewTapInUse => "tap in use".to_string(),
    // }
    // }
}
