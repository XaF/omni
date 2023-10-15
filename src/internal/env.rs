use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use blake3::Hasher;
use git2::Repository;
use is_terminal::IsTerminal;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use petname;

use crate::internal::config::OrgConfig;
use crate::internal::git::safe_git_url_parse;
use crate::internal::user_interface::StringColor;
use crate::omni_warning;

extern crate machine_uid;

lazy_static! {
    #[derive(Debug)]
    pub static ref ENV: Env = Env::new();

    #[derive(Debug)]
    pub static ref GIT_ENV: Mutex<GitRepoEnvByPath> = Mutex::new(GitRepoEnvByPath::new());

    #[derive(Debug)]
    static ref WORKDIR_ENV: Mutex<WorkDirEnvByPath> = Mutex::new(WorkDirEnvByPath::new());

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

pub fn workdir(path: &str) -> WorkDirEnv {
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut workdir_env = WORKDIR_ENV.lock().unwrap();
    workdir_env.get(&path).clone()
}

pub fn workdir_or_init(path: &str) -> Result<WorkDirEnv, String> {
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut workdir_env = WORKDIR_ENV.lock().unwrap();

    let wd = workdir_env.get(&path).clone();
    if wd.in_workdir() && wd.has_id() {
        return Ok(wd);
    }

    let wd_root = if let Some(wd_root) = wd.root() {
        wd_root
    } else {
        &path
    };

    workdir_env.remove(&path);

    let local_config_dir = PathBuf::from(wd_root).join(".omni");
    if let Err(err) = std::fs::create_dir_all(local_config_dir.clone()) {
        return Err(format!(
            "failed to create directory '{}': {}",
            local_config_dir.display(),
            err
        ));
    }

    // Open the 'id' file in the local config directory in write/create mode
    // and write a uuid to it
    let id_file = local_config_dir.join("id");
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(id_file.clone())
    {
        Ok(mut file) => {
            let id = WorkDirEnv::generate_id();
            if let Err(err) = file.write_all(id.as_bytes()) {
                return Err(format!(
                    "failed to write to '{}': {}",
                    id_file.display(),
                    err,
                ));
            }

            omni_warning!(format!(
                "generated workdir id {}",
                id.to_string().light_yellow()
            ));
        }
        Err(err) => {
            return Err(format!("failed to open '{}': {}", id_file.display(), err,));
        }
    }

    let wd = workdir_env.get(&path).clone();

    if !wd.in_workdir() {
        return Err(format!("failed to create workdir for '{}'", path));
    }

    Ok(wd)
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

            interactive_shell: std::io::stdout().is_terminal(),
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

        git_repo_env.in_repo = true;
        git_repo_env.root = git_repo_root;

        match repository.find_remote("origin") {
            Ok(remote) => {
                if let Some(url) = remote.url() {
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

    pub fn has_origin(&self) -> bool {
        self.in_repo && self.origin.is_some()
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

#[derive(Debug, Clone)]
pub struct WorkDirEnvByPath {
    env_by_path: HashMap<String, WorkDirEnv>,
}

impl WorkDirEnvByPath {
    fn new() -> Self {
        Self {
            env_by_path: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &WorkDirEnv {
        if !self.env_by_path.contains_key(path) {
            self.env_by_path
                .insert(path.to_string(), WorkDirEnv::new(path));
        }
        self.env_by_path.get(path).unwrap()
    }

    pub fn remove(&mut self, path: &str) {
        self.env_by_path.remove(path);
    }
}

#[derive(Debug, Clone)]
pub struct WorkDirEnv {
    in_workdir: bool,
    root: Option<String>,
    id: OnceCell<Option<String>>,
}

impl WorkDirEnv {
    fn new(path: &str) -> Self {
        let mut workdir_env = Self {
            in_workdir: false,
            root: None,
            id: OnceCell::new(),
        };

        let git = git_env(&path);
        if git.in_repo() {
            workdir_env.in_workdir = true;
            workdir_env.root = git.root().map(|s| s.to_string());
        } else {
            // Start from `path` and go up until finding a `.omni/id` file
            let mut path = PathBuf::from(path.clone());
            loop {
                if let Some(id) = Self::read_id_file(&path.to_str().unwrap().to_string()) {
                    workdir_env.in_workdir = true;
                    workdir_env.root = Some(path.to_str().unwrap().to_string());
                    if let Err(_) = workdir_env.id.set(Some(id)) {
                        unreachable!();
                    }
                    break;
                }
                if !path.pop() {
                    break;
                }
            }
        }

        workdir_env
    }

    pub fn in_workdir(&self) -> bool {
        self.in_workdir
    }

    pub fn root(&self) -> Option<&str> {
        match &self.root {
            Some(root) => Some(root.as_str()),
            None => None,
        }
    }

    pub fn reldir(&self, path: &str) -> Option<String> {
        if let Some(root) = &self.root {
            if let Ok(path) = std::fs::canonicalize(path) {
                if let Ok(path) = path.strip_prefix(root) {
                    let mut path = path.to_str().unwrap().to_string();
                    while path.starts_with('/') {
                        path = path[1..].to_string();
                    }
                    while path.ends_with('/') {
                        path = path[..path.len() - 1].to_string();
                    }
                    return Some(path);
                }
            }
        }
        None
    }

    pub fn id(&self) -> Option<String> {
        self.id
            .get_or_init(|| {
                if self.root.is_none() {
                    return None;
                }

                if let Some(id) = Self::read_id_file(self.root.as_ref().unwrap()) {
                    return Some(id);
                }

                if let Some(id) = git_env(&self.root.as_ref().unwrap()).id() {
                    return Some(id);
                }

                None
            })
            .clone()
    }

    pub fn has_id(&self) -> bool {
        self.id().is_some()
    }

    fn read_id_file(path: &str) -> Option<String> {
        let id_file = PathBuf::from(path).join(".omni/id");
        if id_file.exists() {
            if let Ok(id) = std::fs::read_to_string(id_file) {
                // if the id is valid, then we can use it, otherwise ignore it
                let id = id.trim();
                if Self::verify_id(&id) {
                    return Some(id.to_string());
                }
            }
        }
        None
    }

    fn generate_id() -> String {
        let petname_id = petname::petname(3, "-");
        format!("{}:{:016x}", petname_id, Self::machine_id_hash(&petname_id))
    }

    fn verify_id(id: &str) -> bool {
        // Split id over ':'
        let id_parts: Vec<&str> = id.split(':').collect();

        // Check if id has 2 parts
        if id_parts.len() != 2 {
            return false;
        }

        // Check if first part is words with lowercase letters separated by '-'
        if !id_parts[0]
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-')
        {
            return false;
        }

        // Check if second part is 16 characters long
        if id_parts[1].len() != 16 {
            return false;
        }

        // Check if second part is a hexadecimal u64
        if let Ok(hash_u64) = u64::from_str_radix(id_parts[1], 16) {
            // Compare hash_u64 with machine_id_hash
            return hash_u64 == WorkDirEnv::machine_id_hash(id_parts[0]);
        }

        false
    }

    fn machine_id_hash(uuid: &str) -> u64 {
        let machine_id = match machine_uid::get() {
            Ok(machine_id) => machine_id,
            // TODO: If unable to fetch the machine id, we use an empty string, which makes things
            //      less secure; using the hostname would be better but the gethostname crate is
            //      panicking if not working, not allowing us to fallback to an empty string; we
            //      might
            Err(_) => "".to_string(),
        };

        let mut hasher = Hasher::new();
        hasher.update(machine_id.as_bytes());
        hasher.update(uuid.as_bytes());

        let hash_bytes = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash_bytes.as_bytes()[..8].try_into().unwrap());

        hash_u64
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
