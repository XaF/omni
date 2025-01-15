use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use humantime::parse_duration;

use crate::internal::config::parser::ConfigErrorHandler;
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
    error_handler: &ConfigErrorHandler,
) -> u64 {
    if let Some(value) = value {
        if let Some(value) = value.as_unsigned_integer() {
            return value;
        } else if let Some(value) = value.as_str() {
            if let Ok(value) = parse_duration(&value) {
                return value.as_secs();
            } else {
                error_handler
                    .with_expected("duration")
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);
            }
        } else {
            error_handler
                .with_expected("duration")
                .with_actual(value)
                .error(ConfigErrorKind::InvalidValueType);
        }
    }
    default
}

pub fn is_executable(path: &std::path::Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Check if a value is allowed by a list of glob patterns.
///
/// The patterns are checked in order, and if no patterns match, the
/// last pattern's deny status is returned (e.g. if the last pattern is
/// a deny pattern, the default is to allow).
///
/// If the list of patterns is empty, all values are allowed.
/// If a pattern starts with `!`, it is a deny pattern.
/// If a pattern does not start with `!`, it is an allow pattern.
pub fn check_allowed(value: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return true; // No patterns means allow all
    }

    for pattern in patterns {
        let is_deny = pattern.starts_with('!');
        let pattern_str = if is_deny { &pattern[1..] } else { pattern };

        let matches = glob::Pattern::new(pattern_str).is_ok_and(|pat| pat.matches(value));
        if matches {
            return !is_deny; // Early return on match
        }

        if pattern_str.ends_with('*') {
            continue;
        }

        // If the pattern does not end with a wildcard, try checking
        // for the pattern as a directory that should prefix the file
        let dir_pattern = format!("{}/**", pattern_str.trim_end_matches('/'));
        let matches = glob::Pattern::new(&dir_pattern).is_ok_and(|pat| pat.matches(value));
        if matches {
            return !is_deny; // Early return on match
        }
    }

    // Get the last pattern's deny status (if any) for the default case
    let default = patterns.last().map_or(true, |p| p.starts_with('!'));
    default
}

#[cfg(test)]
mod tests {
    use super::*;

    mod check_allowed {
        use super::*;

        #[test]
        fn test_empty_patterns() {
            assert!(check_allowed("any/path", &[]));
            assert!(check_allowed("", &[]));
        }

        #[test]
        fn test_exact_matches() {
            // Test exact path matching
            assert!(check_allowed("src/main.rs", &["src/main.rs".to_string()]));

            // Test exact path with deny
            assert!(!check_allowed("src/main.rs", &["!src/main.rs".to_string()]));
        }

        #[test]
        fn test_wildcard_patterns() {
            // Test with * wildcard
            assert!(check_allowed("src/test.rs", &["src/*.rs".to_string()]));

            // Test with multiple patterns including wildcard
            assert!(check_allowed(
                "src/lib.rs",
                &["src/*.rs".to_string(), "!src/test.rs".to_string()]
            ));

            // Test with ** wildcard
            assert!(check_allowed(
                "src/subfolder/test.rs",
                &["src/**/*.rs".to_string()]
            ));
        }

        #[test]
        fn test_directory_prefix_matching() {
            // Test directory prefix without trailing slash
            assert!(check_allowed(
                "src/subfolder/file.rs",
                &["src/subfolder".to_string()]
            ));

            // Test directory prefix with trailing slash
            assert!(check_allowed(
                "src/subfolder/file.rs",
                &["src/subfolder/".to_string()]
            ));

            // Test nested directory matching
            assert!(check_allowed(
                "src/deep/nested/file.rs",
                &["src/deep".to_string()]
            ));
        }

        #[test]
        fn test_multiple_patterns() {
            let patterns = vec![
                "src/secret/public.rs".to_string(),
                "!src/secret/**".to_string(),
                "src/**".to_string(),
            ];

            // Should match general src pattern
            assert!(check_allowed("src/main.rs", &patterns));

            // Should be denied by secret pattern
            assert!(!check_allowed("src/secret/private.rs", &patterns));

            // Should be explicitly allowed despite being in secret dir
            assert!(check_allowed("src/secret/public.rs", &patterns));
        }

        #[test]
        fn test_default_behavior() {
            // Test default allow (last pattern is negative)
            assert!(check_allowed(
                "random.txt",
                &["src/**".to_string(), "!tests/**".to_string()]
            ));

            // Test default deny (last pattern is positive)
            assert!(!check_allowed(
                "random.txt",
                &["!src/**".to_string(), "src/allowed.rs".to_string()]
            ));
        }

        #[test]
        fn test_pattern_priority() {
            let patterns = vec![
                "docs/internal/public/**".to_string(),
                "!docs/internal/**".to_string(),
                "docs/**".to_string(),
            ];

            // Should match third pattern
            assert!(check_allowed("docs/api.md", &patterns));

            // Should be denied by second pattern
            assert!(!check_allowed("docs/internal/secret.md", &patterns));

            // Should be allowed by first pattern
            assert!(check_allowed("docs/internal/public/readme.md", &patterns));
        }

        #[test]
        fn test_directory_prefix() {
            let patterns = vec!["src".to_string()];

            assert!(check_allowed("src", &patterns));
            assert!(check_allowed("src/test", &patterns));
            assert!(check_allowed("src/another/test", &patterns));
        }

        #[test]
        fn test_edge_cases() {
            // Test root pattern
            assert!(check_allowed("any/path", &["**".to_string()]));

            // Test single negative pattern
            assert!(!check_allowed("any/path", &["!**".to_string()]));

            // Test pattern with just trailing wildcard
            assert!(check_allowed("src/anything", &["src/*".to_string()]));
        }
    }
}
