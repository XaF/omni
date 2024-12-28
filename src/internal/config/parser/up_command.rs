use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpCommandConfig {
    pub auto_bootstrap: bool,
    pub notify_workdir_config_updated: bool,
    pub notify_workdir_config_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferred_tools: Vec<String>,
    pub mise_version: String,
    pub upgrade: bool,
    pub operations: Vec<UpCommandOperationConfig>,
}

impl Default for UpCommandConfig {
    fn default() -> Self {
        Self {
            auto_bootstrap: Self::DEFAULT_AUTO_BOOTSTRAP,
            notify_workdir_config_updated: Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED,
            notify_workdir_config_available: Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE,
            preferred_tools: Vec::new(),
            mise_version: Self::DEFAULT_MISE_VERSION.to_string(),
            upgrade: Self::DEFAULT_UPGRADE,
        }
    }
}

impl UpCommandConfig {
    const DEFAULT_AUTO_BOOTSTRAP: bool = true;
    const DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED: bool = true;
    const DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE: bool = true;
    const DEFAULT_MISE_VERSION: &str = "latest";
    const DEFAULT_UPGRADE: bool = false;

    pub(super) fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        // For the values that we don't support overriding in the workdir
        let config_value_global = config_value
            .reject_scope(&ConfigScope::Workdir)
            .unwrap_or_default();

        let preferred_tools =
            if let Some(preferred_tools) = config_value_global.get_as_array("preferred_tools") {
                preferred_tools
                    .iter()
                    .filter_map(|value| value.as_str().map(|value| value.to_string()))
                    .collect()
            } else if let Some(preferred_tool) =
                config_value_global.get_as_str_forced("preferred_tools")
            {
                vec![preferred_tool]
            } else {
                Vec::new()
            };

        Self {
            auto_bootstrap: config_value_global
                .get_as_bool_forced("auto_bootstrap")
                .unwrap_or(Self::DEFAULT_AUTO_BOOTSTRAP),
            notify_workdir_config_updated: config_value_global
                .get_as_bool_forced("notify_workdir_config_updated")
                .unwrap_or(Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_UPDATED),
            notify_workdir_config_available: config_value_global
                .get_as_bool_forced("notify_workdir_config_available")
                .unwrap_or(Self::DEFAULT_NOTIFY_WORKDIR_CONFIG_AVAILABLE),
            preferred_tools,
            mise_version: config_value_global
                .get_as_str_forced("mise_version")
                .unwrap_or(Self::DEFAULT_MISE_VERSION.to_string()),
            // The upgrade option is fine to handle as a workdir option too
            upgrade: config_value
                .get_as_bool_forced("upgrade")
                .unwrap_or(Self::DEFAULT_UPGRADE),
            operations: UpCommandOperationConfig::from_config_value(config_value),
        }
    }
}

struct UpCommandOperationConfig {
    pub allowed: Vec<String>,
    pub sources: Vec<String>,
    // pub mise: UpCommandOperationMiseConfig,
    // pub cargo_install: UpCommandOperationCargoConfig,
    // pub go_install: UpCommandOperationGoConfig,
    // pub github_release: UpCommandOperationGithubReleaseConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UrlPattern {
    scheme: Option<String>,
    host: Option<String>,
    port: Option<String>,
    username: Option<String>,
    password: Option<String>,
    path: Option<String>,
    query: Option<String>,
    fragment: Option<String>,
}

impl UrlPattern {
    fn parse(url_str: &str) -> Result<Self, url::ParseError> {
        match url::Url::parse(url_str) {
            Ok(url) if url.host_str().is_none() || url.cannot_be_a_base() => {
                let prefixed_url = format!("https://{}", url_str);
                match Self::parse(&prefixed_url) {
                    Ok(mut url) => {
                        url.scheme = None;
                        Ok(url)
                    }
                    Err(err) => Err(err),
                }
            }
            Ok(url) => return Ok(url.into()),
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                let prefixed_url = format!("https://{}", url_str);
                match Self::parse(&prefixed_url) {
                    Ok(mut url) => {
                        url.scheme = None;
                        Ok(url)
                    }
                    Err(err) => Err(err),
                }
            }
            Err(url::ParseError::InvalidPort) => {
                let (cleared_url, port) = Self::_remove_url_port(url_str);
                let port = match port {
                    Some(port) => port,
                    None => return Err(url::ParseError::InvalidPort),
                };

                match Self::parse(&cleared_url) {
                    Ok(mut url) => {
                        url.port = Some(port);
                        Ok(url)
                    }
                    Err(err) => Err(err),
                }
            }
            Err(err) => Err(err),
        }
    }

    /// This identifies if there is a port specified in the URL and
    /// removes it if it's the case, this supports any port even if
    /// it would be considered as an invalid port, which allows to
    /// handle glob patterns
    fn _remove_url_port(url: &str) -> (String, Option<String>) {
        // (A) Find scheme end
        let after_scheme = url.find("://").map(|pos| pos + 3).unwrap_or(0);

        // (B) Find path start
        let path_start = url[after_scheme..]
            .find('/')
            .map(|pos| pos + after_scheme)
            .unwrap_or(url.len());

        // (C) Find auth end
        let auth_end = url[after_scheme..path_start]
            .find('@')
            .map(|pos| pos + after_scheme)
            .unwrap_or(0);

        // (D) Find last colon before path
        if let Some(colon_pos) = url[auth_end..path_start].rfind(':') {
            let port_start = auth_end + colon_pos;
            let port_end = path_start;

            let port = url[port_start + 1..port_end].to_string();
            if !port.is_empty() {
                let url = url[..port_start].to_string() + &url[port_end..];
                return (url, Some(port));
            }
        }

        (url.to_string(), None)
    }

    fn matches(&self, other_url: &UrlPattern) -> bool {
        eprintln!("\tpattern: {:?}", self);
        for param in &[
            (self.scheme.as_deref(), other_url.scheme.as_deref()),
            (self.host.as_deref(), other_url.host.as_deref()),
            (self.port.as_deref(), other_url.port.as_deref()),
            (self.username.as_deref(), other_url.username.as_deref()),
            (self.password.as_deref(), other_url.password.as_deref()),
            (self.path.as_deref(), other_url.path.as_deref()),
            (self.query.as_deref(), other_url.query.as_deref()),
            (self.fragment.as_deref(), other_url.fragment.as_deref()),
        ] {
            let (pattern, component) = param;
            if !Self::_matches_pattern(*component, *pattern) {
                eprintln!("\t\tfailed on {:?}", param);
                return false;
            }
        }
        eprintln!("\t\tmatched");
        true
    }

    fn _matches_pattern(component: Option<&str>, pattern: Option<&str>) -> bool {
        match (component, pattern) {
            (_, None) => true,
            (c, Some(p)) => glob::Pattern::new(p).map_or(false, |pat| pat.matches(c.unwrap_or(""))),
        }
    }
}

impl From<url::Url> for UrlPattern {
    fn from(url: url::Url) -> Self {
        Self {
            scheme: match url.scheme() {
                "" => None,
                scheme => Some(scheme.to_string()),
            },
            host: url.host_str().map(|h| h.to_string()).into(),
            port: url.port().map(|p| p.to_string()).into(),
            username: match url.username() {
                "" => None,
                username => Some(username.to_string()),
            },
            password: url.password().map(|p| p.to_string()).into(),
            path: match url.path().strip_prefix('/').unwrap_or(url.path()) {
                "" => None,
                path => Some(path.to_string()),
            },
            query: url.query().map(|q| q.to_string()).into(),
            fragment: url.fragment().map(|f| f.to_string()).into(),
        }
    }
}

fn check_url_allowed(url_str: &str, patterns: &[String]) -> bool {
    let url = match UrlPattern::parse(url_str) {
        Ok(url) => url,
        Err(_) => return false, // Invalid URL
    };

    for pattern in patterns {
        let is_deny = pattern.starts_with('!');
        let pattern_str = if is_deny { &pattern[1..] } else { pattern };

        let pattern_url = match UrlPattern::parse(pattern_str) {
            Ok(url) => url,
            Err(_err) => continue, // Skip invalid patterns
        };

        let matches = pattern_url.matches(&url);
        if matches {
            return !is_deny; // Early return on match
        }
    }

    // Get the last pattern's deny status (if any) for the default case
    let default = patterns.last().map_or(true, |p| p.starts_with('!'));
    default
}

fn check_allowed(value: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        let is_deny = pattern.starts_with('!');
        let pattern_str = if is_deny { &pattern[1..] } else { pattern };

        let matches = glob::Pattern::new(pattern_str).map_or(false, |pat| pat.matches(value));
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

    #[test]
    fn test_basic_matching() {
        let patterns = vec![
            "!example1.com/org/forbidden".to_string(),
            "example1.com/org/*".to_string(),
        ];

        // Should match with or without protocol
        assert!(check_url_allowed("example1.com/org/allowed", &patterns));
        assert!(check_url_allowed(
            "https://example1.com/org/allowed",
            &patterns
        ));
        assert!(check_url_allowed(
            "http://example1.com/org/allowed",
            &patterns
        ));

        // No protocol in URL should match any of the two
        assert!(!check_url_allowed("example1.com/org/forbidden", &patterns));
        assert!(!check_url_allowed(
            "http://example1.com/org/forbidden",
            &patterns
        ));

        // Should not match because different address than allowed
        assert!(!check_url_allowed("example2.com/org/repo", &patterns));
    }

    #[test]
    fn test_protocol_matching() {
        let patterns = vec![
            "https://example1.com/*".to_string(),
            "!http://example1.com/*".to_string(),
        ];

        assert!(check_url_allowed(
            "https://example1.com/org/repo",
            &patterns
        ));
        assert!(!check_url_allowed(
            "http://example1.com/org/repo",
            &patterns
        ));

        // No protocol in URL should not match any of the two, hence
        // match since ending with a deny pattern
        assert!(check_url_allowed("example1.com/org/repo", &patterns));
    }

    #[test]
    fn test_port_matching() {
        let patterns = vec![
            "!example1.com:8123/*".to_string(),
            "example1.com:8*/*".to_string(),
            "example2.com:*/*".to_string(),
        ];

        assert!(check_url_allowed("example1.com:8080/repo", &patterns));
        assert!(check_url_allowed("example1.com:80/repo", &patterns));
        assert!(check_url_allowed("example2.com:1234/repo", &patterns));
        assert!(check_url_allowed("example2.com/repo", &patterns));
        assert!(!check_url_allowed("example1.com:8123/repo", &patterns));
        assert!(!check_url_allowed("example1.com:9090/repo", &patterns));
    }

    #[test]
    fn test_auth_matching() {
        let patterns = vec![
            "user@example1.com/*".to_string(),
            "user:pass@example2.com/*".to_string(),
            "!baduser@example1.com/*".to_string(),
            "!*:*@example2.com/*".to_string(),
            "*".to_string(),
        ];

        assert!(check_url_allowed("user@example1.com/repo", &patterns));
        assert!(check_url_allowed("user:pass@example2.com/repo", &patterns));
        assert!(!check_url_allowed(
            "user:otherpass@example2.com/repo",
            &patterns
        ));
        assert!(check_url_allowed("example1.com/repo", &patterns)); // No auth specified
        assert!(!check_url_allowed("baduser@example1.com/repo", &patterns));
    }

    #[test]
    fn test_path_matching() {
        let patterns = vec![
            "example1.com/org/*/src".to_string(),
            "example1.com/org/repo/**/test".to_string(),
            "!example1.com/org/*/docs".to_string(),
        ];

        assert!(check_url_allowed("example1.com/org/repo/src", &patterns));
        assert!(check_url_allowed(
            "example1.com/org/repo/deep/test",
            &patterns
        ));
        assert!(!check_url_allowed("example1.com/org/repo/docs", &patterns));
    }

    #[test]
    fn test_query_matching() {
        let patterns = vec![
            "example1.com/*?branch=main".to_string(),
            "example2.com/*?branch=*".to_string(),
            "!example1.com/*?branch=dev".to_string(),
        ];

        assert!(check_url_allowed(
            "example1.com/repo?branch=main",
            &patterns
        ));
        assert!(check_url_allowed(
            "example2.com/repo?branch=anything",
            &patterns
        ));
        assert!(check_url_allowed("example2.com/repo", &patterns)); // No query specified
        assert!(!check_url_allowed(
            "example1.com/repo?branch=dev",
            &patterns
        ));
    }

    #[test]
    fn test_fragment_matching() {
        let patterns = vec![
            "example1.com/*#readme".to_string(),
            "example2.com/*#*".to_string(),
            "!*.com/*#private".to_string(),
        ];

        assert!(check_url_allowed("example1.com/repo#readme", &patterns));
        assert!(check_url_allowed("example2.com/repo#anything", &patterns));
        assert!(check_url_allowed("example2.com/repo", &patterns)); // No fragment specified
        assert!(check_url_allowed("example2.com/repo#private", &patterns));
        assert!(!check_url_allowed("example1.com/repo#private", &patterns));
    }

    #[test]
    fn test_default_behavior() {
        // Empty pattern list
        assert!(check_url_allowed("example1.com/repo", &[]));

        // Last pattern determines default
        let allow_patterns = vec![
            "example1.com/allowed/*".to_string(),
            "example2.com/*".to_string(),
        ];
        assert!(!check_url_allowed("example3.org/repo", &allow_patterns));

        let deny_patterns = vec![
            "example1.com/allowed/*".to_string(),
            "!example2.com/*".to_string(),
        ];
        assert!(check_url_allowed("example3.org/repo", &deny_patterns));
    }

    #[test]
    fn test_invalid_urls() {
        let patterns = vec!["example1.com/*".to_string()];

        assert!(!check_url_allowed("not a url", &patterns));
        assert!(!check_url_allowed("http://", &patterns));
        assert!(!check_url_allowed("://invalid", &patterns));
    }

    #[test]
    fn test_invalid_patterns() {
        let patterns = vec!["not a url".to_string(), "example1.com/*".to_string()];

        // Should ignore invalid pattern and match against valid one
        assert!(check_url_allowed("example1.com/repo", &patterns));
    }
}
