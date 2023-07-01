use std::collections::HashMap;
use std::sync::Mutex;

use lazy_static::lazy_static;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::config_loader;
use crate::internal::config::up::UpConfig;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;
use crate::internal::env::git_env;
use crate::internal::env::ENV;
use crate::internal::env::HOME;
use crate::internal::env::OMNI_GIT;
use crate::internal::git::update_git_repo;

lazy_static! {
    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub static ref CONFIG_PER_PATH: Mutex<OmniConfigPerPath> = Mutex::new(OmniConfigPerPath::new());

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub static ref CONFIG: OmniConfig = {
        let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
        config_per_path.get(".").clone()
    };

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub static ref DEFAULT_WORKTREE: String = {
        let home = HOME.clone();
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

pub fn config(path: &str) -> OmniConfig {
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
    config_per_path.get(&path).clone()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniConfigPerPath {
    config: HashMap<String, OmniConfig>,
}

impl OmniConfigPerPath {
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &OmniConfig {
        let mut key = "/";

        // Get the git root path, if any
        let git_repo = git_env(path);
        if git_repo.in_repo() {
            key = git_repo.root().unwrap();
        }

        // Get the config for the path
        if !self.config.contains_key(key) {
            let config_loader = config_loader(key);
            let new_config = OmniConfig::from_config_value(&config_loader.raw_config);
            self.config.insert(key.to_owned(), new_config);
        }

        self.config.get(key).unwrap()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniConfig {
    pub worktree: String,
    pub cache: CacheConfig,
    pub commands: HashMap<String, CommandDefinition>,
    pub command_match_min_score: f64,
    pub command_match_skip_prompt_if: MatchSkipPromptIfConfig,
    pub config_commands: ConfigCommandsConfig,
    pub makefile_commands: MakefileCommandsConfig,
    pub org: Vec<OrgConfig>,
    pub path: PathConfig,
    pub path_repo_updates: PathRepoUpdatesConfig,
    pub repo_path_format: String,
    pub env: HashMap<String, String>,
    pub cd: CdConfig,
    pub clone: CloneConfig,
    pub up: Option<UpConfig>,
}

impl OmniConfig {
    pub fn from_config_value(config_value: &ConfigValue) -> Self {
        let mut commands_config = HashMap::new();
        match config_value.get("commands") {
            Some(value) => {
                for (key, value) in value.as_table().unwrap() {
                    commands_config.insert(
                        key.to_string(),
                        CommandDefinition::from_config_value(&value),
                    );
                }
            }
            None => {}
        }

        let mut org_config = Vec::new();
        match config_value.get("org") {
            Some(value) => {
                for value in value.as_array().unwrap() {
                    org_config.push(OrgConfig::from_config_value(&value));
                }
            }
            None => {}
        }

        let mut env_config = HashMap::new();
        match config_value.get("env") {
            Some(value) => {
                for (key, value) in value.as_table().unwrap() {
                    env_config.insert(key.to_string(), value.as_str().unwrap().to_string());
                }
            }
            None => {}
        }

        Self {
            worktree: config_value
                .get_as_str("worktree")
                .unwrap_or_else(|| format!("{}", *DEFAULT_WORKTREE)),
            cache: CacheConfig::from_config_value(&config_value.get("cache").unwrap()),
            commands: commands_config,
            command_match_min_score: config_value
                .get_as_float("command_match_min_score")
                .unwrap_or(0.12),
            command_match_skip_prompt_if: MatchSkipPromptIfConfig::from_config_value(
                config_value.get("command_match_skip_prompt_if"),
            ),
            config_commands: ConfigCommandsConfig::from_config_value(
                &config_value.get("config_commands").unwrap(),
            ),
            makefile_commands: MakefileCommandsConfig::from_config_value(
                &config_value.get("makefile_commands").unwrap(),
            ),
            org: org_config,
            path: PathConfig::from_config_value(&config_value.get("path").unwrap()),
            path_repo_updates: PathRepoUpdatesConfig::from_config_value(
                &config_value.get("path_repo_updates").unwrap(),
            ),
            repo_path_format: config_value
                .get_as_str("repo_path_format")
                .unwrap()
                .to_string(),
            env: env_config,
            cd: CdConfig::from_config_value(config_value.get("cd")),
            clone: CloneConfig::from_config_value(config_value.get("clone")),
            up: match config_value.get("up") {
                Some(value) => UpConfig::from_config_value(&value),
                None => None,
            },
        }
    }

    pub fn worktree(&self) -> String {
        if let Some(omni_git) = OMNI_GIT.clone() {
            return omni_git;
        }

        self.worktree.clone()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheConfig {
    pub path: String,
}

impl CacheConfig {
    fn from_config_value(config_value: &ConfigValue) -> Self {
        Self {
            path: match config_value.get("path") {
                Some(value) => value.as_str().unwrap().to_string(),
                None => format!("{}", ENV.cache_home.clone()),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandDefinition {
    pub desc: Option<String>,
    pub run: String,
    pub aliases: Vec<String>,
    pub syntax: Option<CommandSyntax>,
    pub category: Option<Vec<String>>,
    pub subcommands: Option<HashMap<String, CommandDefinition>>,
    pub source: ConfigSource,
}

impl CommandDefinition {
    fn from_config_value(config_value: &ConfigValue) -> Self {
        let syntax = match config_value.get("syntax") {
            Some(value) => CommandSyntax::from_config_value(&value),
            None => None,
        };

        let category = match config_value.get("category") {
            Some(value) => {
                let mut category = Vec::new();
                if value.is_array() {
                    for value in value.as_array().unwrap() {
                        category.push(value.as_str().unwrap().to_string());
                    }
                } else {
                    category.push(value.as_str().unwrap().to_string());
                }
                Some(category)
            }
            None => None,
        };

        let subcommands = match config_value.get("subcommands") {
            Some(value) => {
                let mut subcommands = HashMap::new();
                for (key, value) in value.as_table().unwrap() {
                    subcommands.insert(
                        key.to_string(),
                        CommandDefinition::from_config_value(&value),
                    );
                }
                Some(subcommands)
            }
            None => None,
        };

        let aliases = match config_value.get_as_array("aliases") {
            Some(value) => value
                .iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect(),
            None => vec![],
        };

        Self {
            desc: match config_value.get("desc") {
                Some(value) => Some(value.as_str().unwrap().to_string()),
                None => None,
            },
            run: config_value
                .get_as_str("run")
                .unwrap_or("true".to_string())
                .to_string(),
            aliases: aliases,
            syntax: syntax,
            category: category,
            subcommands: subcommands,
            source: config_value.get_source().clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandSyntax {
    pub usage: Option<String>,
    pub arguments: Vec<SyntaxOptArg>,
    pub options: Vec<SyntaxOptArg>,
}

impl CommandSyntax {
    pub fn new() -> Self {
        CommandSyntax {
            usage: None,
            arguments: vec![],
            options: vec![],
        }
    }

    fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
        let mut usage = None;
        let mut arguments = vec![];
        let mut options = vec![];

        if config_value.is_table() {
            for key in ["arguments", "argument"] {
                if let Some(value) = config_value.get(key) {
                    if let Some(value) = value.as_array() {
                        arguments = value
                            .iter()
                            .map(|value| SyntaxOptArg::from_config_value(&value))
                            .collect();
                    } else {
                        arguments.push(SyntaxOptArg::from_config_value(&value));
                    }
                    break;
                }
            }

            for key in ["options", "option", "optional"] {
                if let Some(value) = config_value.get(key) {
                    if let Some(value) = value.as_array() {
                        options = value
                            .iter()
                            .map(|value| SyntaxOptArg::from_config_value(&value))
                            .collect();
                    } else {
                        options.push(SyntaxOptArg::from_config_value(&value));
                    }
                    break;
                }
            }
        } else {
            usage = Some(config_value.as_str().unwrap().to_string());
        }

        if arguments.len() == 0 && options.len() == 0 && usage.is_none() {
            return None;
        }

        Some(Self {
            usage: usage,
            arguments: arguments,
            options: options,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyntaxOptArg {
    pub name: String,
    pub desc: Option<String>,
}

impl SyntaxOptArg {
    pub fn new(name: String, desc: Option<String>) -> Self {
        Self {
            name: name,
            desc: match desc {
                Some(value) => Some(value),
                None => None,
            },
        }
    }

    fn from_config_value(config_value: &ConfigValue) -> Self {
        let mut name = "".to_string();
        let mut desc = None;
        if config_value.is_table() {
            for (key, value) in config_value.as_table().unwrap() {
                name = key;
                desc = Some(value.as_str().unwrap().to_string());
                break;
            }
        } else {
            name = config_value.as_str().unwrap();
        }

        Self {
            name: name,
            desc: desc,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MatchSkipPromptIfConfig {
    pub enabled: bool,
    pub first_min: f64,
    pub second_max: f64,
}

impl MatchSkipPromptIfConfig {
    const DEFAULT_FIRST_MIN: f64 = 0.80;
    const DEFAULT_SECOND_MAX: f64 = 0.60;

    fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        match config_value {
            Some(config_value) => Self {
                enabled: match config_value.get("enabled") {
                    Some(value) => value.as_bool().unwrap(),
                    None => {
                        config_value.get("first_min").is_some()
                            || config_value.get("second_max").is_some()
                    }
                },
                first_min: match config_value.get("first_min") {
                    Some(value) => value.as_float().unwrap(),
                    None => Self::DEFAULT_FIRST_MIN,
                },
                second_max: match config_value.get("second_max") {
                    Some(value) => value.as_float().unwrap(),
                    None => Self::DEFAULT_SECOND_MAX,
                },
            },
            None => Self {
                enabled: false,
                first_min: Self::DEFAULT_FIRST_MIN,
                second_max: Self::DEFAULT_SECOND_MAX,
            },
        }
    }

    fn default() -> Self {
        Self::from_config_value(None)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigCommandsConfig {
    pub split_on_dash: bool,
    pub split_on_slash: bool,
}

impl ConfigCommandsConfig {
    fn from_config_value(config_value: &ConfigValue) -> Self {
        Self {
            split_on_dash: match config_value.get("split_on_dash") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
            split_on_slash: match config_value.get("split_on_slash") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MakefileCommandsConfig {
    pub enabled: bool,
    pub split_on_dash: bool,
    pub split_on_slash: bool,
}

impl MakefileCommandsConfig {
    fn from_config_value(config_value: &ConfigValue) -> Self {
        Self {
            enabled: match config_value.get("enabled") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
            split_on_dash: match config_value.get("split_on_dash") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
            split_on_slash: match config_value.get("split_on_slash") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrgConfig {
    pub handle: String,
    pub trusted: bool,
    pub worktree: Option<String>,
}

impl OrgConfig {
    pub fn from_str(value_str: &str) -> Self {
        let mut split = value_str.split("=");
        let handle = split.next().unwrap().to_string();
        let worktree = match split.next() {
            Some(value) => Some(value.to_string()),
            None => None,
        };
        Self {
            handle: handle,
            trusted: true,
            worktree: worktree,
        }
    }

    pub fn from_config_value(config_value: &ConfigValue) -> Self {
        // If the config_value contains a value directly, we want to consider
        // it as the "handle=worktree", and not as a table.
        if config_value.is_str() {
            let value_str = config_value.as_str().unwrap();
            return OrgConfig::from_str(&value_str);
        }

        Self {
            handle: config_value.get_as_str("handle").unwrap().to_string(),
            trusted: match config_value.get_as_bool("trusted") {
                Some(value) => value,
                None => false,
            },
            worktree: match config_value.get_as_str("worktree") {
                Some(value) => Some(value),
                None => None,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathConfig {
    pub append: Vec<String>,
    pub prepend: Vec<String>,
}

impl PathConfig {
    fn from_config_value(config_value: &ConfigValue) -> Self {
        Self {
            append: config_value
                .get_as_array("append")
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect(),
            prepend: config_value
                .get_as_array("prepend")
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathRepoUpdatesConfig {
    pub enabled: bool,
    pub self_update: PathRepoUpdatesSelfUpdateEnum,
    pub interval: u64,
    pub ref_type: String,
    pub ref_match: Option<String>,
    pub per_repo_config: HashMap<String, PathRepoUpdatesPerRepoConfig>,
}

impl PathRepoUpdatesConfig {
    fn from_config_value(config_value: &ConfigValue) -> Self {
        let mut per_repo_config = HashMap::new();
        match config_value.get("per_repo_config") {
            Some(value) => {
                for (key, value) in value.as_table().unwrap() {
                    per_repo_config.insert(
                        key.to_string(),
                        PathRepoUpdatesPerRepoConfig::from_config_value(&value),
                    );
                }
            }
            None => {}
        };

        Self {
            enabled: match config_value.get("enabled") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
            self_update: match config_value.get_as_str("self_update") {
                Some(value) => match value.to_lowercase().as_str() {
                    "ask" => PathRepoUpdatesSelfUpdateEnum::Ask,
                    "true" => PathRepoUpdatesSelfUpdateEnum::True,
                    "false" => PathRepoUpdatesSelfUpdateEnum::False,
                    _ => PathRepoUpdatesSelfUpdateEnum::Ask,
                },
                None => PathRepoUpdatesSelfUpdateEnum::Ask,
            },
            interval: match config_value.get("interval") {
                Some(value) => value.as_integer().unwrap() as u64,
                None => 12 * 60 * 60,
            },
            ref_type: match config_value.get("ref_type") {
                Some(value) => value.as_str().unwrap().to_string(),
                None => "branch".to_string(),
            },
            ref_match: match config_value.get("ref_match") {
                Some(value) => Some(value.as_str().unwrap().to_string()),
                None => None,
            },
            per_repo_config: per_repo_config,
        }
    }

    pub fn update_config(&self, repo_id: &str) -> (bool, String, Option<String>) {
        match self.per_repo_config.get(repo_id) {
            Some(value) => (
                value.enabled,
                value.ref_type.clone(),
                value.ref_match.clone(),
            ),
            None => (self.enabled, self.ref_type.clone(), self.ref_match.clone()),
        }
    }

    pub fn update(&self, repo_id: &str) -> bool {
        let (enabled, ref_type, ref_match) = self.update_config(repo_id);

        if !enabled {
            return false;
        }

        update_git_repo(repo_id, ref_type, ref_match, None, None)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PathRepoUpdatesSelfUpdateEnum {
    True,
    False,
    #[serde(other)]
    Ask,
}

impl PathRepoUpdatesSelfUpdateEnum {
    pub fn is_true(&self) -> bool {
        match self {
            PathRepoUpdatesSelfUpdateEnum::True => true,
            _ => false,
        }
    }

    pub fn is_false(&self) -> bool {
        match self {
            PathRepoUpdatesSelfUpdateEnum::False => true,
            PathRepoUpdatesSelfUpdateEnum::Ask => !ENV.interactive_shell,
            _ => false,
        }
    }

    pub fn is_ask(&self) -> bool {
        match self {
            PathRepoUpdatesSelfUpdateEnum::Ask => ENV.interactive_shell,
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathRepoUpdatesPerRepoConfig {
    pub enabled: bool,
    pub ref_type: String,
    pub ref_match: Option<String>,
}

impl PathRepoUpdatesPerRepoConfig {
    fn from_config_value(config_value: &ConfigValue) -> Self {
        Self {
            enabled: match config_value.get("enabled") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
            ref_type: match config_value.get("ref_type") {
                Some(value) => value.as_str().unwrap().to_string(),
                None => "branch".to_string(),
            },
            ref_match: match config_value.get("ref_match") {
                Some(value) => Some(value.as_str().unwrap().to_string()),
                None => None,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CdConfig {
    pub path_match_min_score: f64,
    pub path_match_skip_prompt_if: MatchSkipPromptIfConfig,
}

impl CdConfig {
    const DEFAULT_PATH_MATCH_MIN_SCORE: f64 = 0.12;

    fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        if config_value.is_none() {
            return Self {
                path_match_min_score: Self::DEFAULT_PATH_MATCH_MIN_SCORE,
                path_match_skip_prompt_if: MatchSkipPromptIfConfig::default(),
            };
        }
        let config_value = config_value.unwrap();

        Self {
            path_match_min_score: match config_value.get("path_match_min_score") {
                Some(value) => value.as_float().unwrap(),
                None => Self::DEFAULT_PATH_MATCH_MIN_SCORE,
            },
            path_match_skip_prompt_if: MatchSkipPromptIfConfig::from_config_value(
                config_value.get("path_match_skip_prompt_if"),
            ),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CloneConfig {
    pub ls_remote_timeout_seconds: u64,
}

impl CloneConfig {
    const DEFAULT_LS_REMOTE_TIMEOUT_SECONDS: u64 = 5;

    fn from_config_value(config_value: Option<ConfigValue>) -> Self {
        if config_value.is_none() {
            return Self {
                ls_remote_timeout_seconds: Self::DEFAULT_LS_REMOTE_TIMEOUT_SECONDS,
            };
        }
        let config_value = config_value.unwrap();

        Self {
            ls_remote_timeout_seconds: match config_value
                .get_as_unsigned_integer("ls_remote_timeout_seconds")
            {
                Some(value) => value,
                None => Self::DEFAULT_LS_REMOTE_TIMEOUT_SECONDS,
            },
        }
    }
}
