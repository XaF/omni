use core::fmt::Display;
use core::fmt::Error;
use core::fmt::Formatter;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpError {
    Config(String),
    Exec(String),
    Timeout(String),
    Cache(String),
    HomebrewTapInUse,
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

impl Display for UpError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            UpError::Config(message) => write!(f, "configuration error: {}", message),
            UpError::Exec(message) => write!(f, "execution error: {}", message),
            UpError::Timeout(message) => write!(f, "timeout: {}", message),
            UpError::Cache(message) => write!(f, "cache error: {}", message),
            UpError::HomebrewTapInUse => write!(f, "tap in use"),
            UpError::StepFailed(name, progress) => {
                if let Some((step, total)) = progress {
                    write!(f, "step {}/{} '{}' failed", step, total, name)
                } else {
                    write!(f, "step '{}' failed", name)
                }
            }
        }
    }
}
