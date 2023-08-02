use std::collections::HashSet;
use std::path::PathBuf;

use git_url_parse::GitUrl;
use git_url_parse::Scheme;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use requestty;
use strsim::normalized_damerau_levenshtein;
use url::Url;
use walkdir::WalkDir;

use crate::config::config;
use crate::config::OrgConfig;
use crate::env::ENV;
use crate::git::format_path;
use crate::git::safe_git_url_parse;
use crate::git::safe_normalize_url;
use crate::user_interface::colors::StringColor;
use crate::omni_print;

lazy_static! {
        #[derive(Debug)]
        pub static ref ORG_LOADER: OrgLoader = OrgLoader::new();
}

#[derive(Debug, Clone)]
pub struct OrgLoader {
    pub orgs: Vec<Org>,
}

impl OrgLoader {
    pub fn new() -> Self {
        let mut orgs = vec![];

        for org in ENV.omni_org.iter() {
            if let Ok(org) = Org::new(org.clone()) {
                orgs.push(org);
            }
        }

        for org in config(".").org.iter() {
            if let Ok(org) = Org::new(org.clone()) {
                orgs.push(org);
            }
        }

        Self { orgs: orgs }
    }

    pub fn first(&self) -> Option<&Org> {
        self.orgs.first()
    }

    pub fn orgs(&self) -> &Vec<Org> {
        &self.orgs
    }

    pub fn complete(&self, repo: &str) -> Vec<String> {
        let mut worktrees = HashSet::new();
        worktrees.insert(config(".").worktree().into());
        for org in self.orgs.iter() {
            let path = PathBuf::from(org.worktree());
            if path.is_dir() {
                worktrees.insert(path);
            }
        }

        let mut matches = HashSet::new();
        let find_match = format!("/{}", repo);
        for worktree in worktrees.iter() {
            for entry in WalkDir::new(worktree).follow_links(true) {
                if let Ok(entry) = entry {
                    let filetype = entry.file_type();
                    let filepath = entry.path();

                    // We only want places where there's a `.git` directory, since it generally
                    // indicates that we are in a git repository
                    if !filetype.is_dir()
                        || !filepath.file_name().is_some()
                        || filepath.file_name().unwrap() != ".git"
                    {
                        continue;
                    }

                    // Take the parent
                    let filepath = filepath.parent().unwrap();

                    // Remove worktree from the path
                    let filepath = filepath.strip_prefix(worktree).unwrap();

                    // Convert to a string
                    let filepath_str = filepath.to_str().unwrap();

                    if filepath_str.starts_with(repo) {
                        matches.insert(filepath_str.to_string());
                    }

                    if repo != "" {
                        if let Some(index) = filepath_str.find(&find_match) {
                            matches.insert(filepath_str[(index + 1)..].to_string());
                        }
                    }
                }
            }
        }

        let mut results = matches.into_iter().collect::<Vec<_>>();
        results.sort();
        results
    }

    pub fn find_repo(&self, repo: &str) -> Option<PathBuf> {
        if let Some(path) = self.basic_naive_lookup(repo) {
            return Some(path);
        }
        if let Some(path) = self.file_system_lookup(repo) {
            return Some(path);
        }
        None
    }

    fn basic_naive_lookup(&self, repo: &str) -> Option<PathBuf> {
        for org in self.orgs.iter() {
            if let Some(path) = org.get_repo_path(repo) {
                if path.is_dir() {
                    return Some(path);
                }
            }
        }
        None
    }

    fn file_system_lookup(&self, repo: &str) -> Option<PathBuf> {
        let repo_url = Repo::parse(repo);
        if repo_url.is_err() {
            dbg!("repo_url.is_err()");
            return None;
        }
        let repo_url = repo_url.unwrap();
        let rel_path = repo_url.rel_path();

        // Get worktrees
        let mut worktrees = Vec::new();
        let mut seen = HashSet::new();
        for org in self.orgs.iter() {
            if seen.insert(org.worktree()) {
                worktrees.push(org.worktree());
            }
        }

        let worktree = config(".").worktree();
        if seen.insert(worktree.clone()) {
            worktrees.push(worktree.clone());
        }

        // Prepare a spinner for the research
        let spinner = if ENV.interactive_shell {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg:.green}")
                    .unwrap(),
            );
            spinner.set_message("Searching repositories...");
            Some(spinner)
        } else {
            None
        };

        // Start a timer
        let start = std::time::Instant::now();

        // Walk worktrees to try and find the repo
        let slash_rel_path = format!("/{}", rel_path);
        let mut all_repos = Vec::new();
        for worktree in worktrees.iter() {
            for entry in WalkDir::new(worktree).follow_links(true) {
                if let Ok(entry) = entry {
                    let filetype = entry.file_type();
                    let filepath = entry.path();

                    // We only want places where there's a `.git` directory, since it generally
                    // indicates that we are in a git repository
                    if !filetype.is_dir()
                        || !filepath.file_name().is_some()
                        || filepath.file_name().unwrap() != ".git"
                    {
                        continue;
                    }

                    // Take the parent
                    let filepath = filepath.parent().unwrap();

                    spinner
                        .clone()
                        .map(|s| s.set_message(format!("{}", filepath.to_str().unwrap())));

                    // Check if it ends with the expected rel_path
                    if filepath
                        .to_str()
                        .unwrap()
                        .ends_with(slash_rel_path.as_str())
                    {
                        spinner.clone().map(|s| s.finish_and_clear());

                        if start.elapsed() > std::time::Duration::from_secs(1) {
                            omni_print!(format!("{} Setting up your organizations will make repository lookup much faster.", "Did you know?".to_string().bold()));
                        }

                        return Some(filepath.to_path_buf());
                    }

                    all_repos.push(PathScore {
                        score: 0.0,
                        abspath: filepath.to_path_buf(),
                        relpath: filepath
                            .strip_prefix(worktree)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_string(),
                    });
                }
                spinner.clone().map(|s| s.tick());
            }
        }

        spinner.clone().map(|s| s.finish_and_clear());

        let mut with_score = all_repos
            .iter_mut()
            .map(|found| {
                let absscore =
                    normalized_damerau_levenshtein(repo, found.abspath.to_str().unwrap());
                let relscore = normalized_damerau_levenshtein(repo, found.relpath.as_str());
                found.score = absscore.max(relscore);
                found
            })
            .filter(|found| found.score > config(".").cd.path_match_min_score)
            .collect::<Vec<_>>();

        if with_score.is_empty() {
            return None;
        }

        with_score.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());
        with_score.reverse();

        if config(".").cd.path_match_skip_prompt_if.enabled
            && with_score[0].score >= config(".").cd.path_match_skip_prompt_if.first_min
            && (with_score.len() < 2
                || with_score[1].score <= config(".").cd.path_match_skip_prompt_if.second_max)
        {
            return Some(with_score[0].abspath.to_path_buf());
        }

        if ENV.interactive_shell {
            let page_size = 7;
            let question = if with_score.len() > 1 {
                requestty::Question::select("did_you_mean_repo")
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(format!(
                        "{} {}",
                        "omni:".to_string().light_cyan(),
                        "Did you mean?".to_string().yellow()
                    ))
                    .choices(
                        with_score
                            .iter()
                            .map(|found| found.abspath.to_str().unwrap().to_string()),
                    )
                    .should_loop(false)
                    .page_size(page_size)
                    .build()
            } else {
                requestty::Question::confirm("did_you_mean_repo")
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(format!(
                        "{} {} {} {}",
                        "omni:".to_string().light_cyan(),
                        "Did you mean?".to_string().yellow(),
                        "·".to_string().light_black(),
                        with_score[0].abspath.to_str().unwrap().to_string().normal(),
                    ))
                    .default(true)
                    .build()
            };

            match requestty::prompt_one(question) {
                Ok(answer) => {
                    match answer {
                        requestty::Answer::ListItem(listitem) => {
                            // return Some(with_score[listitem.index].abspath.to_path_buf());
                            return Some(listitem.text.into());
                        }
                        requestty::Answer::Bool(confirmed) => {
                            if confirmed {
                                // println!("{}", format!("[✔] {}", with_score[0].abspath.to_str().unwrap()).green());
                                return Some(with_score[0].abspath.to_path_buf());
                            }
                        }
                        _ => {}
                    }
                }
                Err(err) => {
                    if page_size < with_score.len() {
                        print!("\x1B[1A\x1B[2K"); // This clears the line, so there's no artifact left
                    }
                    println!("{}", format!("[✘] {:?}", err).red());
                }
            }
        }

        None
    }
}

#[derive(Debug)]
struct PathScore {
    score: f64,
    abspath: PathBuf,
    relpath: String,
}

impl Into<String> for PathScore {
    fn into(self) -> String {
        self.abspath.to_str().unwrap().to_string()
    }
}

impl<'a> Into<String> for &'a mut PathScore {
    fn into(self) -> String {
        self.abspath.to_str().unwrap().to_string()
    }
}

#[derive(Debug, Clone)]
pub enum OrgError {
    InvalidHandle(&'static str),
}

#[derive(Debug, Clone)]
pub struct Org {
    pub config: OrgConfig,
    url: Url,
    owner: Option<String>,
    repo: Option<String>,
    enforce_scheme: bool,
    enforce_user: bool,
    enforce_password: bool,
}

impl Org {
    fn new(config: OrgConfig) -> Result<Self, OrgError> {
        let parsed_url = safe_normalize_url(&config.handle);
        if parsed_url.is_err() {
            return Err(OrgError::InvalidHandle(
                "Invalid org handle: url parsing failed",
            ));
        }
        let mut parsed_url = parsed_url.unwrap();
        if parsed_url.scheme().is_empty() || parsed_url.scheme() == "file" {
            parsed_url.set_scheme("https").unwrap();
        }

        if let Some(host) = parsed_url.host_str() {
            if !config.handle.starts_with(parsed_url.scheme()) && !host.contains(".") {
                return Err(OrgError::InvalidHandle("Invalid org handle: invalid host"));
            }
        } else {
            return Err(OrgError::InvalidHandle("Invalid org handle: no host"));
        }

        let mut owner = None;
        let mut repo = None;

        if let Some(path) = parsed_url.path_segments() {
            let mut path_segments = path.collect::<Vec<_>>();
            path_segments.retain(|segment| !segment.is_empty());
            if path_segments.len() > 1 {
                repo = Some(path_segments.pop().unwrap().to_string());
            }
            if path_segments.len() > 0 {
                owner = Some(path_segments.pop().unwrap().to_string());
            }
        }

        let enforce_scheme = parsed_url.scheme() == "ssh" || config.handle.contains("://");
        let enforce_user = !parsed_url.username().is_empty();
        let enforce_password = !parsed_url.password().unwrap_or("").is_empty();

        Ok(Self {
            config: config,
            url: parsed_url,
            owner: owner,
            repo: repo,
            enforce_scheme: enforce_scheme,
            enforce_user: enforce_user,
            enforce_password: enforce_password,
        })
    }

    pub fn worktree(&self) -> String {
        if let Some(worktree) = self.config.worktree.clone() {
            worktree
        } else {
            config(".").worktree()
        }
    }

    pub fn get_repo_path(&self, repo: &str) -> Option<PathBuf> {
        // Get the repo git url
        let git_url = self.get_repo_git_url(repo);
        if git_url.is_none() {
            return None;
        }
        let git_url = git_url.unwrap();

        Some(format_path(&self.worktree(), &git_url))
    }

    pub fn hosts_repo(&self, repo: &str) -> bool {
        if let Ok(url) = safe_git_url_parse(repo) {
            return (!self.enforce_scheme || self.url.scheme() == url.scheme.to_string())
                && (self.url.port() == url.port || self.url.port_or_known_default() == url.port)
                && (!self.enforce_user
                    || self.url.username() == url.user.as_deref().unwrap_or(""))
                && (!self.enforce_password || self.url.password() == url.token.as_deref())
                && self.url.host_str() == url.host.as_ref().map(|s| s.as_str())
                && (self.owner.is_none() || self.owner == url.owner)
                && (self.repo.is_none() || self.repo == Some(url.name));
        }
        false
    }

    // pub fn get_repo_url(&self, repo: &str) -> Option<String> {
    // if let Some(git_url) = self.get_repo_git_url(repo) {
    // Some(git_url.to_string())
    // } else {
    // None
    // }
    // }

    pub fn get_repo_git_url(&self, repo: &str) -> Option<GitUrl> {
        if let Ok(repo) = Repo::parse(repo) {
            // If the repo has a scheme, we need to make sure it matches the org's scheme
            if repo.scheme_prefix && self.enforce_scheme {
                let scheme = repo.scheme.unwrap();
                let org_scheme = self.url.scheme();
                if scheme != org_scheme {
                    return None;
                }
            }

            // If the repo has a host, we need to make sure it matches the org's host
            if repo.host.is_some() {
                let host = repo.host.unwrap();
                let org_host = self.url.host_str().unwrap();
                if host != org_host {
                    return None;
                }
            }

            // If the repo has a user, we need to make sure it matches the org's user (if any)
            if repo.user.is_some() && self.enforce_user {
                let user = repo.user.unwrap();
                let org_user = self.url.username();
                if user != org_user {
                    return None;
                }
            }

            // If the repo has a password, we need to make sure it matches the org's password (if any)
            if repo.password.is_some() && self.enforce_password {
                let token = repo.password.unwrap();
                if let Some(org_token) = self.url.password() {
                    if token != org_token {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            // If the repo has a port, we need to make sure it matches the org's port (if any)
            if repo.port.is_some() {
                let port = repo.port.unwrap();
                if let Some(org_port) = self.url.port() {
                    if port != org_port {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            // If we don't have an owner at all, we can't match here
            if repo.owner.is_none() && self.owner.is_none() {
                return None;
            }

            // If the repo has an owner, we need to make sure it matches the org's owner (if any)
            if repo.owner.is_some() && self.owner.is_some() {
                let owner = repo.owner.clone().unwrap();
                let org_owner = self.owner.clone().unwrap();
                if owner != org_owner {
                    return None;
                }
            }

            // If the repo has a repo name, we need to make sure it matches the org's repo name (if any)
            if self.repo.is_some() {
                let name = repo.name.to_string();
                let org_name = self.repo.clone().unwrap();
                if name != org_name {
                    return None;
                }
            }

            let owner = self
                .owner
                .clone()
                .unwrap_or(repo.owner.unwrap_or("".to_string()));
            let name = self.repo.clone().unwrap_or(repo.name.to_string());
            let user = if self.url.username().is_empty() {
                None
            } else {
                Some(self.url.username().to_string())
            };
            let scheme = if self.url.scheme() == "ssh" {
                Scheme::Ssh
            } else {
                Scheme::Https
            };

            let git_url = GitUrl {
                host: self.url.host_str().map(|h| h.to_string()),
                name: name,
                owner: Some(owner.clone()),
                organization: None,
                fullname: format!("{}/{}", owner.clone(), repo.name.to_string()),
                scheme: scheme,
                user: user,
                token: self.url.password().map(|t| t.to_string()),
                port: self.url.port(),
                path: format!(
                    "{}{}/{}",
                    if self.url.scheme() == "ssh" { "" } else { "/" },
                    owner.clone(),
                    repo.name.to_string()
                ),
                git_suffix: repo.git_suffix,
                scheme_prefix: self.url.scheme() != "ssh" || self.url.port().is_some(),
            };

            return Some(git_url);
        }
        None
    }
}

#[derive(Debug, Clone)]
enum RepoError {
    ParseError,
}

#[derive(Debug, Clone)]
struct Repo {
    name: String,
    owner: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    scheme: Option<String>,
    scheme_prefix: bool,
    git_suffix: bool,
    user: Option<String>,
    password: Option<String>,
    rel_path: OnceCell<String>,
}

impl Repo {
    pub fn parse(repo: &str) -> Result<Self, RepoError> {
        if let Ok(url) = safe_git_url_parse(repo) {
            if url.host.is_some() && url.owner.is_some() && !url.name.is_empty() {
                return Ok(Self {
                    name: url.name,
                    owner: url.owner,
                    host: url.host,
                    port: url.port,
                    scheme: Some(url.scheme.to_string()),
                    scheme_prefix: url.scheme_prefix,
                    git_suffix: url.git_suffix,
                    user: url.user,
                    password: url.token,
                    rel_path: OnceCell::new(),
                });
            } else {
                let mut parts = repo.split("/").collect::<Vec<&str>>();

                let mut name = parts.pop().unwrap().to_string();
                let mut git_suffix = false;
                if name.ends_with(".git") {
                    git_suffix = true;
                    name = name[..name.len() - 4].to_string();
                }

                let owner = if parts.len() > 0 {
                    Some(parts.pop().unwrap().to_string())
                } else {
                    None
                };

                return Ok(Self {
                    name: name,
                    owner: owner,
                    host: None,
                    port: None,
                    scheme: None,
                    scheme_prefix: false,
                    git_suffix: git_suffix,
                    user: None,
                    password: None,
                    rel_path: OnceCell::new(),
                });
            }
        }
        Err(RepoError::ParseError)
    }

    pub fn rel_path(&self) -> String {
        self.rel_path
            .get_or_init(|| {
                // Get the configured path format
                let mut path_format = config(".").repo_path_format.clone();

                // Replace %{host}, #{owner}, and %{repo} with the actual values
                if self.host.is_some() {
                    path_format = path_format.replace("%{host}", &self.host.clone().unwrap());
                }
                if self.owner.is_some() {
                    path_format = path_format.replace("%{org}", &self.owner.clone().unwrap());
                }
                path_format = path_format.replace("%{repo}", &self.name.clone());

                // Split the path, and keep only the particles toward the end that DO NOT
                // have any missing placeholder
                let mut parts = path_format.split("/").collect::<Vec<&str>>();
                parts.reverse();

                let mut path_parts = Vec::new();
                for part in parts.iter() {
                    if part.contains("%{host}") || part.contains("%{org}") {
                        break;
                    }
                    path_parts.push(part.to_string());
                }
                path_parts.reverse();

                // Reverse path_parts and join with / to generate the string
                let mut path = String::new();
                for part in path_parts.iter() {
                    path.push_str(part);
                    path.push_str("/");
                }

                // Remove the trailing slash
                if path.ends_with("/") {
                    path = path[..path.len() - 1].to_string();
                }

                path
            })
            .clone()
    }
}
