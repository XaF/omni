use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;

use git2::Repository;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;

use crate::internal::config::OrgConfig;
use crate::internal::git::safe_git_url_parse;

lazy_static! {
    #[derive(Debug)]
    pub static ref ENV: Env = Env::new();

    #[derive(Debug)]
    pub static ref GIT_ENV: Mutex<GitRepoEnvByPath> = Mutex::new(GitRepoEnvByPath::new());

    #[derive(Debug)]
    pub static ref HOME: String = std::env::var("HOME").expect("Failed to determine user's home directory");

    #[derive(Debug)]
    pub static ref OMNI_GIT: Option<String> = {
        if let Ok(omni_git) = std::env::var("OMNI_GIT") {
            if !omni_git.is_empty() && omni_git.starts_with('/') {
                return Some(omni_git);
            }
        }
        None
    };
}

pub fn git_env(path: &str) -> GitRepoEnv {
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut git_env = GIT_ENV.lock().unwrap();
    git_env.get(&path).clone()
}

#[derive(Debug, Clone)]
pub struct Env {
    pub cache_home: String,
    pub config_home: String,
    pub data_home: String,

    pub xdg_cache_home: String,
    pub xdg_config_home: String,
    pub xdg_data_home: String,

    pub git_by_path: GitRepoEnvByPath,

    pub interactive_shell: bool,
    pub shell: String,

    // pub omnidir: String,
    // pub omni_located: bool,
    pub omnipath: Vec<String>,
    pub omni_cmd_file: Option<String>,
    pub omni_org: Vec<OrgConfig>,
    // pub omni_skip_update: bool,
    // pub omni_force_update: bool,
    // pub omni_subcommand: Option<String>,
    // pub omni_uuid: Option<String>,
}

impl Env {
    fn new() -> Self {
        // Find XDG_CONFIG_HOME
        let xdg_config_home = match std::env::var("XDG_CONFIG_HOME") {
            Ok(xdg_config_home)
                if !xdg_config_home.is_empty() && xdg_config_home.starts_with('/') =>
            {
                xdg_config_home
            }
            _ => {
                format!("{}/.config", *HOME)
            }
        };

        // Resolve omni's config home
        let config_home = match std::env::var("OMNI_CONFIG_HOME") {
            Ok(config_home)
                if !config_home.is_empty()
                    && (config_home.starts_with('/') || config_home.starts_with("~/")) =>
            {
                if config_home.starts_with("~/") {
                    format!("{}/{}", *HOME, &config_home[2..])
                } else {
                    config_home
                }
            }
            _ => {
                format!("{}/omni", xdg_config_home)
            }
        };

        // Find XDG_DATA_HOME
        let xdg_data_home = match std::env::var("XDG_DATA_HOME") {
            Ok(xdg_data_home) if !xdg_data_home.is_empty() && xdg_data_home.starts_with('/') => {
                xdg_data_home
            }
            _ => {
                format!("{}/.local/share", *HOME)
            }
        };

        // Resolve omni's data home
        let data_home = match std::env::var("OMNI_DATA_HOME") {
            Ok(data_home)
                if !data_home.is_empty()
                    && (data_home.starts_with('/') || data_home.starts_with("~/")) =>
            {
                if data_home.starts_with("~/") {
                    format!("{}/{}", *HOME, &data_home[2..])
                } else {
                    data_home
                }
            }
            _ => {
                format!("{}/omni", xdg_data_home)
            }
        };

        // Find XDG_CACHE_HOME
        let xdg_cache_home = match std::env::var("XDG_CACHE_HOME") {
            Ok(xdg_cache_home) if !xdg_cache_home.is_empty() && xdg_cache_home.starts_with('/') => {
                xdg_cache_home
            }
            _ => {
                format!("{}/.cache", *HOME)
            }
        };

        // Resolve omni's cache home
        let cache_home = match std::env::var("OMNI_CACHE_HOME") {
            Ok(cache_home)
                if !(cache_home.is_empty()
                    || (!cache_home.starts_with('/') && !cache_home.starts_with("~/"))) =>
            {
                if cache_home.starts_with("~/") {
                    format!("{}/{}", *HOME, &cache_home[2..])
                } else {
                    cache_home
                }
            }
            _ => {
                let xdg_cache_home =
                    std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{}/.cache", *HOME));
                format!("{}/omni", xdg_cache_home)
            }
        };

        // Load the omni path while deduplicating
        let mut omnipath = Vec::new();
        let mut omnipath_seen = HashSet::new();
        if let Ok(omnipath_str) = std::env::var("OMNIPATH") {
            for path in omnipath_str.split(':') {
                if !path.is_empty() && omnipath_seen.insert(path.to_string()) {
                    omnipath.push(path.to_string());
                }
            }
        }

        // Load the omni org
        let mut omni_org = Vec::new();
        if let Ok(omni_org_str) = std::env::var("OMNI_ORG") {
            for path in omni_org_str.split(',') {
                if !path.is_empty() {
                    omni_org.push(OrgConfig::from_str(&path.to_string()));
                }
            }
        }

        // Load the command file
        let mut omni_cmd_file = None;
        if let Ok(omni_cmd_file_str) = std::env::var("OMNI_CMD_FILE") {
            if !omni_cmd_file_str.is_empty() {
                omni_cmd_file = Some(omni_cmd_file_str);
            }
        }

        Env {
            cache_home: cache_home,
            config_home: config_home,
            data_home: data_home,

            xdg_cache_home: xdg_cache_home,
            xdg_config_home: xdg_config_home,
            xdg_data_home: xdg_data_home,

            interactive_shell: atty::is(atty::Stream::Stdout),
            shell: determine_shell(),

            git_by_path: GitRepoEnvByPath::new(),

            omnipath: omnipath,
            omni_org: omni_org,
            omni_cmd_file: omni_cmd_file,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitRepoEnvByPath {
    env_by_path: HashMap<String, GitRepoEnv>,
}

impl GitRepoEnvByPath {
    fn new() -> Self {
        Self {
            env_by_path: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &GitRepoEnv {
        if !self.env_by_path.contains_key(path) {
            self.env_by_path
                .insert(path.to_string(), GitRepoEnv::new(path));
        }
        self.env_by_path.get(path).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct GitRepoEnv {
    in_repo: bool,
    root: Option<String>,
    origin: Option<String>,
    id: OnceCell<Option<String>>,
}

impl GitRepoEnv {
    fn new(path: &str) -> Self {
        let mut git_repo_env = Self {
            in_repo: false,
            root: None,
            origin: None,
            id: OnceCell::new(),
        };

        let repository = match Repository::discover(path) {
            Ok(repository) => repository,
            Err(_) => return git_repo_env,
        };

        let mut git_repo_root = None;
        if let Some(workdir) = repository.workdir() {
            if let Some(root_dir) = workdir.to_str() {
                if root_dir.ends_with('/') {
                    git_repo_root = Some(root_dir[..root_dir.len() - 1].to_string());
                } else {
                    git_repo_root = Some(root_dir.to_string());
                }
            }
        }

        match repository.find_remote("origin") {
            Ok(remote) => {
                if let Some(url) = remote.url() {
                    git_repo_env.in_repo = true;
                    git_repo_env.root = git_repo_root;
                    git_repo_env.origin = Some(url.to_string());
                    return git_repo_env;
                }
            }
            Err(_) => {}
        }

        // loop over main, master, current
        for mut branch_name in &["main", "master", "__current"] {
            let mut string_branch_name = branch_name.to_string();
            if string_branch_name == "__current" {
                match repository.head() {
                    Ok(head) => match head.shorthand() {
                        Some(shorthand) => {
                            if shorthand != "HEAD" {
                                string_branch_name = shorthand.to_string();
                            }
                        }
                        None => {}
                    },
                    Err(_) => {}
                }
                if string_branch_name == "__current" {
                    continue;
                }
            }
            let str_branch_name = string_branch_name.as_str();
            branch_name = &str_branch_name;

            match repository.find_branch(branch_name, git2::BranchType::Local) {
                Ok(branch) => match branch.upstream() {
                    Ok(upstream) => match upstream.name() {
                        Ok(upstream_name) => {
                            if let Some(upstream_name) = upstream_name {
                                let upstream_name = upstream_name.split('/').next().unwrap();
                                match repository.find_remote(upstream_name) {
                                    Ok(remote) => {
                                        if let Some(url) = remote.url() {
                                            git_repo_env.in_repo = true;
                                            git_repo_env.root = git_repo_root;
                                            git_repo_env.origin = Some(url.to_string());
                                            return git_repo_env;
                                        }
                                    }
                                    Err(_) => {}
                                }
                            }
                        }
                        Err(_) => {}
                    },
                    Err(_) => {}
                },
                Err(_) => {}
            }
        }

        match repository.remotes() {
            Ok(remotes) => {
                for remote in remotes.iter() {
                    match repository.find_remote(remote.unwrap()) {
                        Ok(remote) => {
                            if let Some(url) = remote.url() {
                                git_repo_env.in_repo = true;
                                git_repo_env.root = git_repo_root;
                                git_repo_env.origin = Some(url.to_string());
                                return git_repo_env;
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
            Err(_) => {}
        }

        git_repo_env
    }

    pub fn in_repo(&self) -> bool {
        self.in_repo
    }

    pub fn root(&self) -> Option<&str> {
        match &self.root {
            Some(root) => Some(root.as_str()),
            None => None,
        }
    }

    pub fn origin(&self) -> Option<&str> {
        match &self.origin {
            Some(origin) => Some(origin.as_str()),
            None => None,
        }
    }

    pub fn id(&self) -> Option<String> {
        self.id
            .get_or_init(|| {
                if let Some(origin) = &self.origin {
                    if let Ok(url) = safe_git_url_parse(origin) {
                        if let (Some(host), Some(owner), name) = (url.host, url.owner, url.name) {
                            if !name.is_empty() {
                                return Some(format!("{}:{}/{}", host, owner, name));
                            }
                        }
                    }
                }
                None
            })
            .clone()
    }
}

pub fn determine_shell() -> String {
    for var in &["OMNI_SHELL", "SHELL"] {
        if let Some(shell) = std::env::var_os(var) {
            let shell = shell.to_str().unwrap();
            if !shell.is_empty() {
                if shell.contains('/') {
                    let shell = shell.split('/').last().unwrap();
                    return shell.to_string();
                }
                return shell.to_string();
            }
        }
    }

    "bash".to_string()
}
