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
    HomebrewTapInUse,
}

impl UpError {
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
            UpError::HomebrewTapInUse => write!(f, "tap in use"),
        }
    }
}
