use std::fmt;

use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ConfigErrorKind {
    #[error("Value for key {key} should be a {expected} but found {found:?}")]
    ValueType {
        key: String,
        expected: String,
        found: serde_yaml::Value,
    },
    #[error("Value for key {key} should be one of {expected:?} but found {found:?}")]
    InvalidValue {
        key: String,
        expected: Vec<String>,
        found: serde_yaml::Value,
    },
    #[error("Value for key {key} is missing")]
    MissingKey { key: String },
    #[error(
        "Value for key {key} should be a table with a single key-value pair but found: {found:?}"
    )]
    NotExactlyOneKeyInTable {
        key: String,
        found: serde_yaml::Value,
    },
    #[error("Value {found:?} for {key} is not supported in this context")]
    UnsupportedValueInContext {
        key: String,
        found: serde_yaml::Value,
    },
}

/// This is the error type for the `parse_args` function
#[derive(Debug)]
pub enum ParseArgsErrorKind {
    ParserBuildError(String),
    ArgumentParsingError(clap::Error),
}

impl ParseArgsErrorKind {
    #[cfg(test)]
    pub fn simple(&self) -> String {
        match self {
            ParseArgsErrorKind::ParserBuildError(e) => e.clone(),
            ParseArgsErrorKind::ArgumentParsingError(e) => {
                // Return the first block until the first empty line
                let err_str = e
                    .to_string()
                    .split('\n')
                    .map(|line| line.trim())
                    .take_while(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                let err_str = err_str.trim_start_matches("error: ");
                err_str.to_string()
            }
        }
    }
}

impl PartialEq for ParseArgsErrorKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ParseArgsErrorKind::ParserBuildError(a), ParseArgsErrorKind::ParserBuildError(b)) => {
                a == b
            }
            (
                ParseArgsErrorKind::ArgumentParsingError(a),
                ParseArgsErrorKind::ArgumentParsingError(b),
            ) => a.to_string() == b.to_string(),
            _ => false,
        }
    }
}

impl fmt::Display for ParseArgsErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseArgsErrorKind::ParserBuildError(e) => write!(f, "{}", e),
            ParseArgsErrorKind::ArgumentParsingError(e) => write!(f, "{}", e),
        }
    }
}
