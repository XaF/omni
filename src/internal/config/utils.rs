use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use humantime::parse_duration;

use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::ConfigValue;

pub fn sort_serde_yaml(value: &serde_yaml::Value) -> serde_yaml::Value {
    match value {
        serde_yaml::Value::Sequence(seq) => {
            let sorted_seq: Vec<serde_yaml::Value> = seq.iter().map(sort_serde_yaml).collect();
            serde_yaml::Value::Sequence(sorted_seq)
        }
        serde_yaml::Value::Mapping(mapping) => {
            let sorted_mapping: BTreeMap<String, serde_yaml::Value> = mapping
                .iter()
                .map(|(k, v)| (k.as_str().unwrap().to_owned(), sort_serde_yaml(v)))
                .collect();
            let sorted_mapping: serde_yaml::Mapping = sorted_mapping
                .into_iter()
                .map(|(k, v)| (serde_yaml::Value::String(k), v))
                .collect();
            serde_yaml::Value::Mapping(sorted_mapping)
        }
        _ => value.clone(),
    }
}

pub fn parse_duration_or_default(
    value: Option<&ConfigValue>,
    default: u64,
    error_key: &str,
    errors: &mut Vec<ConfigErrorKind>,
) -> u64 {
    if let Some(value) = value {
        if let Some(value) = value.as_unsigned_integer() {
            return value;
        } else if let Some(value) = value.as_str() {
            if let Ok(value) = parse_duration(&value) {
                return value.as_secs();
            } else {
                errors.push(ConfigErrorKind::InvalidValueType {
                    key: error_key.to_string(),
                    expected: "duration".to_string(),
                    actual: serde_yaml::Value::String(value.to_string()),
                });
            }
        } else {
            errors.push(ConfigErrorKind::InvalidValueType {
                key: error_key.to_string(),
                expected: "duration".to_string(),
                actual: value.as_serde_yaml(),
            });
        }
    }
    default
}

pub fn is_executable(path: &std::path::Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
