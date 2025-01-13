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
    #[error("I/O error: {0}")]
    IOError(String),
}

impl From<std::io::Error> for UpError {
    fn from(error: std::io::Error) -> Self {
        UpError::IOError(error.to_string())
    }
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
            UpError::IOError(message) => message.clone(),
        }
    }
}
