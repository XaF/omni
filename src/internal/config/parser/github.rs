use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GithubConfig {
    #[serde(default, rename = "auth", skip_serializing_if = "Vec::is_empty")]
    auth_list: Vec<GithubAuthConfigWithFilters>,
}

impl Empty for GithubConfig {
    fn is_empty(&self) -> bool {
        self.auth_list.is_empty()
    }
}

impl GithubConfig {
    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        Self {
            auth_list: GithubAuthConfigWithFilters::from_config_value_multi(
                config_value.get("auth"),
            ),
        }
    }

    pub fn auth_for(&self, repo: &str, api_hostname: &str) -> GithubAuthConfig {
        self.auth_list
            .iter()
            .find(|auth| auth.matches(repo, api_hostname))
            .map(|auth| auth.auth.clone())
            .unwrap_or(GithubAuthConfig::default())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GithubAuthConfigWithFilters {
    #[serde(
        default,
        with = "serde_yaml::with::singleton_map",
        skip_serializing_if = "StringFilter::is_default"
    )]
    pub repo: StringFilter,
    #[serde(
        default,
        with = "serde_yaml::with::singleton_map",
        skip_serializing_if = "StringFilter::is_default"
    )]
    pub hostname: StringFilter,
    #[serde(flatten)]
    pub auth: GithubAuthConfig,
}

impl GithubAuthConfigWithFilters {
    pub fn matches(&self, repo: &str, api_hostname: &str) -> bool {
        self.repo.matches(repo) && self.hostname.matches(api_hostname)
    }

    pub(super) fn from_config_value_multi(config_value: Option<ConfigValue>) -> Vec<Self> {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return vec![],
        };

        if let Some(array) = config_value.as_array() {
            array
                .iter()
                .map(|config_value| GithubAuthConfigWithFilters::from_config_value(config_value))
                .collect()
        } else {
            vec![GithubAuthConfigWithFilters::from_config_value(
                &config_value,
            )]
        }
    }

    fn from_config_value(config_value: &ConfigValue) -> Self {
        Self {
            repo: StringFilter::from_config_value(config_value.get("repo")),
            hostname: StringFilter::from_config_value(config_value.get("hostname")),
            auth: GithubAuthConfig::from_config_value(Some(config_value.clone())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GithubAuthConfig {
    Token(String),
    TokenEnvVar(String),
    #[serde(rename = "gh")]
    GhCli {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hostname: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user: Option<String>,
    },
    Skip(bool),
}

impl Default for GithubAuthConfig {
    fn default() -> Self {
        GithubAuthConfig::GhCli {
            hostname: None,
            user: None,
        }
    }
}

impl GithubAuthConfig {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub(in crate::internal::config) fn from_config_value(
        config_value: Option<ConfigValue>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        if let Some(string) = config_value.as_str() {
            return match string.as_str() {
                "skip" => Self::Skip(true),
                "gh" => Self::default(),
                _ => {
                    // If all caps and underscores, consider it's an environment variable
                    if string.chars().all(|c| c.is_uppercase() || c == '_') {
                        Self::TokenEnvVar(string.to_string())
                    } else {
                        Self::Token(string.to_string())
                    }
                }
            };
        } else if let Some(table) = config_value.as_table() {
            if let Some(skip) = table.get("skip") {
                if skip.as_bool().unwrap_or(false) {
                    return Self::Skip(true);
                }
            }

            if let Some(token_env_var) = table.get("token_env_var") {
                if let Some(token_env_var) = token_env_var.as_str_forced() {
                    return Self::TokenEnvVar(token_env_var.to_string());
                }
            }

            if let Some(token) = table.get("token") {
                if let Some(token) = token.as_str_forced() {
                    return Self::Token(token.to_string());
                }
            }

            if let Some(gh_value) = table.get("gh") {
                let mut hostname = None;
                let mut user = None;

                if let Some(gh_table) = gh_value.as_table() {
                    if let Some(hostname_value) = gh_table.get("hostname") {
                        hostname = hostname_value.as_str_forced();
                    }
                    if let Some(user_value) = gh_table.get("user") {
                        user = user_value.as_str_forced();
                    }
                } else if let Some(gh_string) = gh_value.as_str_forced() {
                    hostname = Some(gh_string.to_string());
                }

                return Self::GhCli { hostname, user };
            }
        }

        Self::default()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StringFilter {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Regex(String),
    Glob(String),
    Exact(String),
    #[default]
    Any,
}

impl StringFilter {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn matches(&self, value: &str) -> bool {
        match self {
            StringFilter::Any => true,
            StringFilter::Contains(pattern) => {
                value.to_lowercase().contains(&pattern.to_lowercase())
            }
            StringFilter::StartsWith(pattern) => {
                value.to_lowercase().starts_with(&pattern.to_lowercase())
            }
            StringFilter::EndsWith(pattern) => {
                value.to_lowercase().ends_with(&pattern.to_lowercase())
            }
            StringFilter::Regex(pattern) => match regex::Regex::new(pattern) {
                Ok(regex) => regex.is_match(value),
                Err(_) => false,
            },
            StringFilter::Glob(pattern) => match glob::Pattern::new(&pattern.to_lowercase()) {
                Ok(glob) => glob.matches(&value.to_lowercase()),
                Err(_) => false,
            },
            StringFilter::Exact(pattern) => value.to_lowercase() == pattern.to_lowercase(),
        }
    }

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        if let Some(string) = config_value.as_str() {
            // If a string is provided, use it as a glob pattern by default
            StringFilter::Glob(string.to_string())
        } else if let Some(table) = config_value.as_table() {
            if let Some(entry) = table.get("contains") {
                if let Some(value) = entry.as_str_forced() {
                    StringFilter::Contains(value)
                } else {
                    Self::default()
                }
            } else if let Some(entry) = table.get("starts_with") {
                if let Some(value) = entry.as_str_forced() {
                    StringFilter::StartsWith(value)
                } else {
                    Self::default()
                }
            } else if let Some(entry) = table.get("ends_with") {
                if let Some(value) = entry.as_str_forced() {
                    StringFilter::EndsWith(value)
                } else {
                    Self::default()
                }
            } else if let Some(entry) = table.get("regex") {
                if let Some(value) = entry.as_str_forced() {
                    StringFilter::Regex(value)
                } else {
                    Self::default()
                }
            } else if let Some(entry) = table.get("glob") {
                if let Some(value) = entry.as_str_forced() {
                    StringFilter::Glob(value)
                } else {
                    Self::default()
                }
            } else if let Some(entry) = table.get("exact") {
                if let Some(value) = entry.as_str_forced() {
                    StringFilter::Exact(value)
                } else {
                    Self::default()
                }
            } else {
                Self::default()
            }
        } else {
            Self::default()
        }
    }
}
