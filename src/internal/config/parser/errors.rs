use std::fmt;

use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ConfigErrorKind {
    #[error("Value for key '{key}' should be a {expected} but found {actual:?}")]
    InvalidValueType {
        key: String,
        expected: String,
        actual: serde_yaml::Value,
    },
    #[error("Value for key '{key}' should be one of {expected:?} but found {actual:?}")]
    InvalidValue {
        key: String,
        expected: Vec<String>,
        actual: serde_yaml::Value,
    },
    #[error("Value for key '{key}' should define a valid range, but found [{min}, {max}[ instead")]
    InvalidRange { key: String, min: usize, max: usize },
    #[error("Value for key '{key}' should be a valid package, but found '{package}'")]
    InvalidPackage { key: String, package: String },
    #[error("Value for key '{key}' is missing")]
    MissingKey { key: String },
    #[error("Value for key '{key}' is empty")]
    EmptyKey { key: String },
    #[error(
        "Value for key '{key}' should be a table with a single key-value pair but actual: {found:?}"
    )]
    NotExactlyOneKeyInTable {
        key: String,
        actual: serde_yaml::Value,
    },
    #[error("Value {actual:?} for '{key}' is not supported in this context")]
    UnsupportedValueInContext {
        key: String,
        actual: serde_yaml::Value,
    },
    #[error("Parsing error for value {actual:?} for key '{key}': {error}")]
    ParsingError {
        key: String,
        actual: serde_yaml::Value,
        error: String,
    },
    #[error("Missing subkey for key '{key}' in metadata header at line {lineno}")]
    MetadataHeaderMissingSubkey { key: String, lineno: usize },
    #[error("Line {lineno} in metadata header is a 'continue' but there is no current key")]
    MetadataHeaderContinueWithoutKey { lineno: usize },
    #[error("Unknown key {key} in metadata header at line {lineno}")]
    MetadataHeaderUnknownKey { key: String, lineno: usize },
    #[error("No syntax provided")]
    MetadataHeaderMissingSyntax,
    #[error("No help provided")]
    MetadataHeaderMissingHelp,
    #[error("Empty part in the definition of group or parameter {name}")]
    MetadataHeaderGroupOrParamEmptyPart { name: String },
    #[error("Unknown configuration key {key} in the definition of group or parameter {name}")]
    MetadataHeaderUnknownGroupOrParamConfigKey { name: String, key: String },
    #[error("Invalid part '{part}' in the definition of group or parameter {name}")]
    MetadataHeaderGroupOrParamInvalidPart { name: String, part: String },
    #[error(
        "Invalid value '{value}' for key '{key}' in the definition of group or parameter {name}"
    )]
    MetadataHeaderParamInvalidKeyValue {
        name: String,
        key: String,
        value: String,
    },
    #[error("Missing description for parameter {name}")]
    MetadataHeaderParamMissingDescription { name: String },
    #[error("Group {name} does not have any parameters")]
    MetadataHeaderGroupMissingParameters { name: String },
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
