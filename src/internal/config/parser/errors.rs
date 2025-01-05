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
        "Value for key '{key}' should be a table with a single key-value pair but found {actual:?}"
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
    #[error("Unable to parse value {actual:?} for key '{key}': {error}")]
    ParsingError {
        key: String,
        actual: serde_yaml::Value,
        error: String,
    },
    #[error("Missing subkey for key '{key}' in metadata header at line {lineno}")]
    MetadataHeaderMissingSubkey { key: String, lineno: usize },
    #[error("Line {lineno} in metadata header is a 'continue' but there is no current key")]
    MetadataHeaderContinueWithoutKey { lineno: usize },
    #[error("Unknown key '{key}' in metadata header at line {lineno}")]
    MetadataHeaderUnknownKey { key: String, lineno: usize },
    #[error(
        "Key '{key}' in metadata header at line {lineno} previously defined at line {prev_lineno}"
    )]
    MetadataHeaderDuplicateKey {
        key: String,
        lineno: usize,
        prev_lineno: usize,
    },
    #[error("No syntax provided")]
    MetadataHeaderMissingSyntax,
    #[error("No help provided")]
    MetadataHeaderMissingHelp,
    #[error("Empty part in the definition of group '{group}'")]
    MetadataHeaderGroupEmptyPart { group: String },
    #[error("Invalid part '{part}' in the definition of group '{group}'")]
    MetadataHeaderGroupInvalidPart { group: String, part: String },
    #[error("Unknown configuration key '{key}' in the definition of group '{group}'")]
    MetadataHeaderGroupUnknownConfigKey { group: String, key: String },
    #[error("Group '{group}' does not have any parameters")]
    MetadataHeaderGroupMissingParameters { group: String },
    #[error("Empty part in the definition of parameter '{parameter}'")]
    MetadataHeaderParameterEmptyPart { parameter: String },
    #[error("Invalid part '{part}' in the definition of parameter '{parameter}'")]
    MetadataHeaderParameterInvalidPart { parameter: String, part: String },
    #[error("Unknown configuration key '{key}' in the definition of parameter '{parameter}'")]
    MetadataHeaderParameterUnknownConfigKey { parameter: String, key: String },
    #[error(
        "Invalid value '{value}' for key '{key}' in the definition of group or parameter {parameter}"
    )]
    MetadataHeaderParameterInvalidKeyValue {
        parameter: String,
        key: String,
        value: String,
    },
    #[error("Missing description for parameter '{parameter}'")]
    MetadataHeaderParameterMissingDescription { parameter: String },
    #[error("File '{path}' is not executable")]
    OmniPathFileNotExecutable { path: String },
    #[error("Failed to load metadata for file '{path}'")]
    OmniPathFileFailedToLoadMetadata { path: String },
}

impl ConfigErrorKind {
    pub fn path(&self) -> Option<&str> {
        match self {
            ConfigErrorKind::OmniPathFileNotExecutable { path } => Some(path),
            ConfigErrorKind::OmniPathFileFailedToLoadMetadata { path } => Some(path),
            _ => None,
        }
    }

    pub fn lineno(&self) -> Option<usize> {
        match self {
            ConfigErrorKind::MetadataHeaderMissingSubkey { lineno, .. } => Some(*lineno),
            ConfigErrorKind::MetadataHeaderContinueWithoutKey { lineno } => Some(*lineno),
            ConfigErrorKind::MetadataHeaderUnknownKey { lineno, .. } => Some(*lineno),
            ConfigErrorKind::MetadataHeaderDuplicateKey { lineno, .. } => Some(*lineno),
            _ => None,
        }
    }

    pub fn errorcode(&self) -> &str {
        match self {
            //  Cxxx for configuration errors
            //    C0xx for key errors
            ConfigErrorKind::MissingKey { .. } => "C001",
            ConfigErrorKind::EmptyKey { .. } => "C002",
            ConfigErrorKind::NotExactlyOneKeyInTable { .. } => "C003",
            //    C1xx for value errors
            ConfigErrorKind::InvalidValueType { .. } => "C101",
            ConfigErrorKind::InvalidValue { .. } => "C102",
            ConfigErrorKind::InvalidRange { .. } => "C103",
            ConfigErrorKind::InvalidPackage { .. } => "C104",
            ConfigErrorKind::UnsupportedValueInContext { .. } => "C105",
            ConfigErrorKind::ParsingError { .. } => "C106",
            //  MDxx for metadata errors
            //    MD0x for larger missing errors
            ConfigErrorKind::MetadataHeaderMissingHelp => "MD01",
            ConfigErrorKind::MetadataHeaderMissingSyntax => "MD02",
            //    MD1x for key or subkey errors
            ConfigErrorKind::MetadataHeaderContinueWithoutKey { .. } => "MD12",
            ConfigErrorKind::MetadataHeaderDuplicateKey { .. } => "MD13",
            ConfigErrorKind::MetadataHeaderMissingSubkey { .. } => "MD11",
            ConfigErrorKind::MetadataHeaderUnknownKey { .. } => "MD10",
            //    MD2x for group errors
            ConfigErrorKind::MetadataHeaderGroupEmptyPart { .. } => "MD28",
            ConfigErrorKind::MetadataHeaderGroupInvalidPart { .. } => "MD27",
            ConfigErrorKind::MetadataHeaderGroupMissingParameters { .. } => "MD21",
            ConfigErrorKind::MetadataHeaderGroupUnknownConfigKey { .. } => "MD29",
            //    MD3x for parameter errors
            ConfigErrorKind::MetadataHeaderParameterEmptyPart { .. } => "MD38",
            ConfigErrorKind::MetadataHeaderParameterInvalidKeyValue { .. } => "MD31",
            ConfigErrorKind::MetadataHeaderParameterInvalidPart { .. } => "MD37",
            ConfigErrorKind::MetadataHeaderParameterMissingDescription { .. } => "MD32",
            ConfigErrorKind::MetadataHeaderParameterUnknownConfigKey { .. } => "MD39",
            //  Pxxx for path errors
            //    P001 is "Path not found"
            ConfigErrorKind::OmniPathFileNotExecutable { .. } => "P002",
            ConfigErrorKind::OmniPathFileFailedToLoadMetadata { .. } => "P003",
        }
    }
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
