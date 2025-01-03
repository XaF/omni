use std::collections::HashMap;

use lazy_static::lazy_static;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::parser::AskPassConfig;
use crate::internal::config::parser::CacheConfig;
use crate::internal::config::parser::CdConfig;
use crate::internal::config::parser::CloneConfig;
use crate::internal::config::parser::CommandDefinition;
use crate::internal::config::parser::ConfigCommandsConfig;
use crate::internal::config::parser::EnvConfig;
use crate::internal::config::parser::GithubConfig;
use crate::internal::config::parser::MakefileCommandsConfig;
use crate::internal::config::parser::MatchSkipPromptIfConfig;
use crate::internal::config::parser::PathConfig;
use crate::internal::config::parser::PathRepoUpdatesConfig;
use crate::internal::config::parser::PromptsConfig;
use crate::internal::config::parser::ShellAliasesConfig;
use crate::internal::config::parser::SuggestCloneConfig;
use crate::internal::config::parser::SuggestConfig;
use crate::internal::config::parser::UpCommandConfig;
use crate::internal::config::up::UpConfig;
use crate::internal::config::ConfigLoader;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;
use crate::internal::config::OrgConfig;
use crate::internal::env::omni_git_env;
use crate::internal::env::user_home;

lazy_static! {
    #[derive(Debug, Serialize, Deserialize, Clone)]
    static ref DEFAULT_WORKTREE: String = {
        let home = user_home();
        let mut default_worktree_path = format!("{}/git", home);
        if !std::path::Path::new(&default_worktree_path).is_dir() {
            // Check if GOPATH is set and GOPATH/src exists and is a directory
            let gopath = std::env::var("GOPATH").unwrap_or_else(|_| "".to_string());
            if !gopath.is_empty() {
                let gopath_src = format!("{}/src", gopath);
                if std::path::Path::new(&gopath_src).is_dir() {
                    default_worktree_path = gopath_src;
                }
            }
        }
        default_worktree_path
    };
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniConfig {
    pub askpass: AskPassConfig,
    pub cache: CacheConfig,
    pub cd: CdConfig,
    pub clone: CloneConfig,
    pub command_match_min_score: f64,
    pub command_match_skip_prompt_if: MatchSkipPromptIfConfig,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub commands: HashMap<String, CommandDefinition>,
    pub config_commands: ConfigCommandsConfig,
    #[serde(skip_serializing_if = "EnvConfig::is_empty")]
    pub env: EnvConfig,
    #[serde(skip_serializing_if = "GithubConfig::is_empty")]
    pub github: GithubConfig,
    pub makefile_commands: MakefileCommandsConfig,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub org: Vec<OrgConfig>,
    pub path: PathConfig,
    pub path_repo_updates: PathRepoUpdatesConfig,
    #[serde(skip_serializing_if = "PromptsConfig::is_empty")]
    pub prompts: PromptsConfig,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub repo_path_format: String,
    #[serde(skip_serializing_if = "ShellAliasesConfig::is_empty")]
    pub shell_aliases: ShellAliasesConfig,
    #[serde(skip_serializing_if = "SuggestCloneConfig::is_empty")]
    pub suggest_clone: SuggestCloneConfig,
    #[serde(skip_serializing_if = "SuggestConfig::is_empty")]
    pub suggest_config: SuggestConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up: Option<UpConfig>,
    pub up_command: UpCommandConfig,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub worktree: String,

    #[serde(skip)]
    pub config_errors: Vec<ConfigErrorKind>,
}

impl OmniConfig {
    const DEFAULT_COMMAND_MATCH_MIN_SCORE: f64 = 0.12;
    const DEFAULT_REPO_PATH_FORMAT: &'static str = "%{host}/%{org}/%{repo}";

    pub fn from_config_value(config_value: &ConfigValue) -> Self {
        let mut errors = Vec::new();

        let mut commands_config = HashMap::new();
        if let Some(value) = config_value.get("commands") {
            if let Some(table) = value.as_table() {
                for (key, value) in table {
                    commands_config.insert(
                        key.to_string(),
                        CommandDefinition::from_config_value(
                            &value,
                            &format!("commands.{}", key),
                            &mut errors,
                        ),
                    );
                }
            } else {
                errors.push(ConfigErrorKind::InvalidValueType {
                    key: "commands".to_string(),
                    expected: "table".to_string(),
                    actual: value.as_serde_yaml(),
                });
            }
        }

        let mut org_config = Vec::new();
        if let Some(value) = config_value.get("org") {
            // TODO: instead of rejecting scope, we should support protected parameters
            // in the config_value object so that those parameters would never be overwritten
            // by any work directory specific configuration.
            if let Some(value) = value.reject_scope(&ConfigScope::Workdir) {
                if let Some(array) = value.as_array() {
                    for (_idx, value) in array.iter().enumerate() {
                        // TODO
                        org_config.push(OrgConfig::from_config_value(&value));
                    }
                } else {
                    errors.push(ConfigErrorKind::InvalidValueType {
                        key: "org".to_string(),
                        expected: "array".to_string(),
                        actual: value.as_serde_yaml(),
                    });
                }
            }
        }

        let askpass =
            AskPassConfig::from_config_value(config_value.get("askpass"), "askpass", &mut errors);
        let cache = CacheConfig::from_config_value(config_value.get("cache"), "cache", &mut errors);
        let cd = CdConfig::from_config_value(config_value.get("cd"), "cd", &mut errors);
        let clone = CloneConfig::from_config_value(config_value.get("clone"), "clone", &mut errors);
        let command_match_min_score = config_value.get_as_float_or_default(
            "command_match_min_score",
            Self::DEFAULT_COMMAND_MATCH_MIN_SCORE,
            "command_match_min_score",
            &mut errors,
        );
        let command_match_skip_prompt_if = MatchSkipPromptIfConfig::from_config_value(
            config_value.get("command_match_skip_prompt_if"),
            "command_match_skip_prompt_if",
            &mut errors,
        );
        let config_commands = ConfigCommandsConfig::from_config_value(
            config_value.get("config_commands"),
            "config_commands",
            &mut errors,
        );
        let env = EnvConfig::from_config_value(config_value.get("env"), "env", &mut errors);
        let github =
            GithubConfig::from_config_value(config_value.get("github"), "github", &mut errors);
        let makefile_commands = MakefileCommandsConfig::from_config_value(
            config_value.get("makefile_commands"),
            "makefile_commands",
            &mut errors,
        );
        let path = PathConfig::from_config_value(config_value.get("path"), "path", &mut errors);
        let path_repo_updates = PathRepoUpdatesConfig::from_config_value(
            config_value.get("path_repo_updates"),
            "path_repo_updates",
            &mut errors,
        );
        let prompts =
            PromptsConfig::from_config_value(config_value.get("prompts"), "prompts", &mut errors);
        let repo_path_format = config_value.get_as_str_or_default(
            "repo_path_format",
            Self::DEFAULT_REPO_PATH_FORMAT,
            "repo_path_format",
            &mut errors,
        );
        let shell_aliases = ShellAliasesConfig::from_config_value(
            config_value.get("shell_aliases"),
            "shell_aliases",
            &mut errors,
        );
        let suggest_clone = SuggestCloneConfig::from_config_value(
            config_value.get("suggest_clone"),
            "suggest_clone",
            &mut errors,
        );
        let suggest_config = SuggestConfig::from_config_value(
            config_value.get("suggest_config"),
            "suggest_config",
            &mut errors,
        );
        let up = UpConfig::from_config_value(config_value.get("up"), "up", &mut errors);
        let up_command = UpCommandConfig::from_config_value(
            config_value.get("up_command"),
            "up_command",
            &mut errors,
        );

        let worktree = config_value.get_as_str_or_default(
            "worktree",
            &*DEFAULT_WORKTREE,
            "worktree",
            &mut errors,
        );

        Self {
            askpass,
            cache,
            cd,
            clone,
            command_match_min_score,
            command_match_skip_prompt_if,
            commands: commands_config,
            config_commands,
            env,
            github,
            makefile_commands,
            org: org_config,
            path,
            path_repo_updates,
            prompts,
            repo_path_format,
            shell_aliases,
            suggest_clone,
            suggest_config,
            up,
            up_command,
            worktree,
            config_errors: errors,
        }
    }

    pub fn worktree(&self) -> String {
        if let Some(omni_git) = omni_git_env() {
            return omni_git;
        }

        self.worktree.clone()
    }

    pub fn repo_path_format_host(&self) -> bool {
        self.repo_path_format.contains("%{host}")
    }

    pub fn repo_path_format_org(&self) -> bool {
        self.repo_path_format.contains("%{org}")
    }

    pub fn repo_path_format_repo(&self) -> bool {
        self.repo_path_format.contains("%{repo}")
    }

    /// Returns a hash of the configuration used for setting up a repository,
    /// so that we can inform the user if they should call `omni up` again.
    ///
    /// This includes the following configuration parameters:
    /// - up
    /// - suggest_config
    /// - suggest_clone
    /// - env
    pub fn up_hash(&self) -> String {
        let mut config_hasher = blake3::Hasher::new();

        if let Some(up) = &self.up {
            if let Ok(up_str) = serde_yaml::to_string(&up) {
                config_hasher.update(up_str.as_bytes());
            }
        }

        if let Ok(suggest_config_str) = serde_yaml::to_string(&self.suggest_config) {
            config_hasher.update(suggest_config_str.as_bytes());
        }

        if let Ok(suggest_clone_str) = serde_yaml::to_string(&self.suggest_clone) {
            config_hasher.update(suggest_clone_str.as_bytes());
        }

        if let Ok(env_str) = serde_yaml::to_string(&self.env) {
            config_hasher.update(env_str.as_bytes());
        }

        config_hasher.finalize().to_hex()[..16].to_string()
    }
}

impl From<ConfigLoader> for OmniConfig {
    fn from(config_loader: ConfigLoader) -> Self {
        OmniConfig::from_config_value(&config_loader.raw_config)
    }
}
