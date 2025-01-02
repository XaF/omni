use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::MatchSkipPromptIfConfig;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CdConfig {
    pub fast_search: bool,
    pub path_match_min_score: f64,
    pub path_match_skip_prompt_if: MatchSkipPromptIfConfig,
}

impl CdConfig {
    const DEFAULT_FAST_SEARCH: bool = true;
    const DEFAULT_PATH_MATCH_MIN_SCORE: f64 = 0.12;

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => {
                return Self {
                    fast_search: Self::DEFAULT_FAST_SEARCH,
                    path_match_min_score: Self::DEFAULT_PATH_MATCH_MIN_SCORE,
                    path_match_skip_prompt_if: MatchSkipPromptIfConfig::default(),
                }
            }
        };

        Self {
            fast_search: config_value.get_as_bool_or_default(
                "fast_search",
                Self::DEFAULT_FAST_SEARCH,
                &format!("{}.fast_search", error_key),
                errors,
            ),
            path_match_min_score: config_value.get_as_float_or_default(
                "path_match_min_score",
                Self::DEFAULT_PATH_MATCH_MIN_SCORE,
                &format!("{}.path_match_min_score", error_key),
                errors,
            ),
            path_match_skip_prompt_if: MatchSkipPromptIfConfig::from_config_value(
                config_value.get("path_match_skip_prompt_if"),
                &format!("{}.path_match_skip_prompt_if", error_key),
                errors,
            ),
        }
    }
}
