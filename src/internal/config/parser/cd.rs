use serde::Deserialize;
use serde::Serialize;

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

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        if config_value.is_none() {
            return Self {
                fast_search: Self::DEFAULT_FAST_SEARCH,
                path_match_min_score: Self::DEFAULT_PATH_MATCH_MIN_SCORE,
                path_match_skip_prompt_if: MatchSkipPromptIfConfig::default(),
            };
        }
        let config_value = config_value.unwrap();

        Self {
            fast_search: config_value
                .get_as_bool("fast_search")
                .unwrap_or(Self::DEFAULT_FAST_SEARCH),
            path_match_min_score: config_value
                .get_as_float("path_match_min_score")
                .unwrap_or(Self::DEFAULT_PATH_MATCH_MIN_SCORE),
            path_match_skip_prompt_if: MatchSkipPromptIfConfig::from_config_value(
                config_value.get("path_match_skip_prompt_if"),
            ),
        }
    }
}
