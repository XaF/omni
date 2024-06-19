use std::collections::HashSet;
use std::hash::Hash;
use std::path::PathBuf;

use git_url_parse::GitUrl;
use git_url_parse::Scheme;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use strsim::normalized_damerau_levenshtein;
use url::Url;
use walkdir::WalkDir;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::path::omnipath_entries;
use crate::internal::config::config;
use crate::internal::config::OrgConfig;
use crate::internal::env::omni_org_env;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::package_path_from_handle;
use crate::internal::git::package_root_path;
use crate::internal::git::safe_git_url_parse;
use crate::internal::git::safe_normalize_url;
use crate::internal::git::utils::format_path_with_template;
use crate::internal::git::utils::format_path_with_template_and_data;
use crate::internal::git_env;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_print;

lazy_static! {
        #[derive(Debug)]
        pub static ref ORG_LOADER: OrgLoader = OrgLoader::new();
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
enum SearchEntryMatchStart {
    Host,
    Owner,
    Repo,
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct ResultEntry {
    path: String,
    rel_path: String,
    match_name: String,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct SearchEntry {
    worktree: String,
    match_start: SearchEntryMatchStart,
    regex_path: PathBuf,
    glob_path: PathBuf,
}

impl SearchEntry {
    fn extract(&self) -> HashSet<ResultEntry> {
        let mut results = HashSet::new();

        let glob_path = self.glob_path.join(".git");
        let glob_path = if let Some(glob_path) = glob_path.to_str() {
            glob_path
        } else {
            return results;
        };

        let regex_path = self.regex_path.join(".git");
        let regex = if let Some(regex_path) = regex_path.to_str() {
            let regex_str = format!("^{}$", regex_path);
            if let Ok(regex) = regex::Regex::new(regex_str.as_str()) {
                regex
            } else {
                return results;
            }
        } else {
            return results;
        };

        let entries = if let Ok(entries) = glob::glob(glob_path) {
            entries
        } else {
            return results;
        };

        for path in entries.into_iter().flatten() {
            let parent = if let Some(parent) = path.parent() {
                parent
            } else {
                continue;
            };

            let rel_path = if let Ok(rel_path) = parent.strip_prefix(&self.worktree) {
                rel_path
            } else {
                continue;
            };

            let (parent, rel_path) =
                if let (Some(parent), Some(rel_path)) = (parent.to_str(), rel_path.to_str()) {
                    (parent, rel_path)
                } else {
                    continue;
                };

            let entry_str = if let Some(entry_str) = path.to_str() {
                entry_str
            } else {
                continue;
            };

            let captures = if let Some(captures) = regex.captures(entry_str) {
                captures
            } else {
                continue;
            };

            let match_name = match (
                self.match_start,
                captures.name("host"),
                captures.name("owner"),
                captures.name("repo"),
            ) {
                (SearchEntryMatchStart::Host, Some(host), owner, repo) => {
                    let mut result = host.as_str().to_string();
                    if let Some(owner) = owner {
                        result.push('/');
                        result.push_str(owner.as_str());
                        if let Some(repo) = repo {
                            result.push('/');
                            result.push_str(repo.as_str());
                        }
                    }
                    result
                }
                (SearchEntryMatchStart::Owner, _, Some(owner), repo) => {
                    let mut result = owner.as_str().to_string();
                    if let Some(repo) = repo {
                        result.push('/');
                        result.push_str(repo.as_str());
                    }
                    result
                }
                (SearchEntryMatchStart::Repo, _, _, Some(repo)) => repo.as_str().to_string(),
                _ => continue,
            };

            results.insert(ResultEntry {
                path: parent.to_string(),
                rel_path: rel_path.to_string(),
                match_name: match_name.to_string(),
            });
        }

        results
    }
}

#[derive(Debug, Clone)]
pub struct OrgLoader {
    pub orgs: Vec<Org>,
}

impl Empty for OrgLoader {
    fn is_empty(&self) -> bool {
        // Empty here means we have only the default org
        self.orgs.len() < 2
    }
}

impl OrgLoader {
    pub fn new() -> Self {
        let mut orgs = vec![];

        // Get all the orgs from the environment
        for org in omni_org_env() {
            if let Ok(org) = Org::new(org.clone()) {
                orgs.push(org);
            }
        }

        // Get all the orgs from the config
        for org in config(".").org.iter() {
            if let Ok(org) = Org::new(org.clone()) {
                orgs.push(org);
            }
        }

        // Add a default org that has _no_ parameter and
        // that is not trusted; it will be used when trying
        // to go over the find repositories in order
        orgs.push(Org::default());

        Self { orgs }
    }

    pub fn first(&self) -> Option<&Org> {
        self.orgs.first()
    }

    pub fn orgs(&self) -> &Vec<Org> {
        &self.orgs
    }

    pub fn printable_orgs(&self) -> Vec<Org> {
        self.orgs
            .iter()
            .filter(|org| !org.is_default())
            .cloned()
            .collect()
    }

    pub fn search_glob_org(&self, repo: &str) -> HashSet<ResultEntry> {
        if let Ok(parsed) = Repo::parse(repo) {
            let cfg = config(".");

            let mut searches = HashSet::new();
            for org in self.orgs.iter() {
                // Prepare a regex for the path so we can extract the host, owner,
                // and repo from a matching path
                let regex_path = format_path_with_template_and_data(
                    &org.worktree(),
                    "(?P<host>[^/]+)",
                    "(?P<owner>[^/]+)",
                    "(?P<repo>[^/]+)",
                    &org.repo_path_format(),
                );

                match (&parsed.host, &parsed.owner) {
                    (Some(search_host), Some(search_owner)) => {
                        if cfg.repo_path_format_repo() {
                            searches.insert(SearchEntry {
                                worktree: org.worktree(),
                                match_start: SearchEntryMatchStart::Host,
                                regex_path: regex_path.clone(),
                                glob_path: format_path_with_template_and_data(
                                    &org.worktree(),
                                    search_host,
                                    search_owner,
                                    format!("{}*", parsed.name).as_str(),
                                    &org.repo_path_format(),
                                ),
                            });
                        }
                    }
                    (None, Some(search_owner)) => {
                        if cfg.repo_path_format_org() {
                            searches.insert(SearchEntry {
                                worktree: org.worktree(),
                                match_start: SearchEntryMatchStart::Host,
                                regex_path: regex_path.clone(),
                                glob_path: format_path_with_template_and_data(
                                    &org.worktree(),
                                    search_owner,
                                    format!("{}*", parsed.name).as_str(),
                                    "*",
                                    &org.repo_path_format(),
                                ),
                            });
                        }
                        if cfg.repo_path_format_repo() {
                            searches.insert(SearchEntry {
                                worktree: org.worktree(),
                                match_start: SearchEntryMatchStart::Owner,
                                regex_path: regex_path.clone(),
                                glob_path: format_path_with_template_and_data(
                                    &org.worktree(),
                                    "*",
                                    search_owner,
                                    format!("{}*", parsed.name).as_str(),
                                    &org.repo_path_format(),
                                ),
                            });
                        }
                    }
                    (None, None) => {
                        if cfg.repo_path_format_host() {
                            searches.insert(SearchEntry {
                                worktree: org.worktree(),
                                match_start: SearchEntryMatchStart::Host,
                                regex_path: regex_path.clone(),
                                glob_path: format_path_with_template_and_data(
                                    &org.worktree(),
                                    format!("{}*", parsed.name).as_str(),
                                    "*",
                                    "*",
                                    &org.repo_path_format(),
                                ),
                            });
                        }
                        if cfg.repo_path_format_org() {
                            searches.insert(SearchEntry {
                                worktree: org.worktree(),
                                match_start: SearchEntryMatchStart::Owner,
                                regex_path: regex_path.clone(),
                                glob_path: format_path_with_template_and_data(
                                    &org.worktree(),
                                    "*",
                                    format!("{}*", parsed.name).as_str(),
                                    "*",
                                    &org.repo_path_format(),
                                ),
                            });
                        }
                        if cfg.repo_path_format_repo() {
                            searches.insert(SearchEntry {
                                worktree: org.worktree(),
                                match_start: SearchEntryMatchStart::Repo,
                                regex_path: regex_path.clone(),
                                glob_path: format_path_with_template_and_data(
                                    &org.worktree(),
                                    "*",
                                    "*",
                                    format!("{}*", parsed.name).as_str(),
                                    &org.repo_path_format(),
                                ),
                            });
                        }
                    }
                    _ => unreachable!(),
                }
            }

            let mut results = HashSet::new();
            for search_entry in searches.iter() {
                results.extend(search_entry.extract());
            }

            return results;
        }

        HashSet::new()
    }

    pub fn complete(&self, repo: &str) -> Vec<String> {
        let results = self.search_glob_org(repo);
        if !results.is_empty() {
            return results
                .into_iter()
                .map(|result| result.match_name)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
        }

        if config(".").cd.fast_search {
            return vec![];
        }

        let mut worktrees = HashSet::new();
        worktrees.insert(config(".").worktree().into());
        for org in self.orgs.iter() {
            let path = PathBuf::from(org.worktree());
            if path.is_dir() {
                worktrees.insert(path.clone());
            }
        }

        let mut visited = HashSet::new();
        let mut matches = HashSet::new();
        let find_match = format!("/{}", repo);
        for worktree in worktrees.iter() {
            if !visited.insert(worktree.to_owned()) {
                continue;
            }

            for entry in WalkDir::new(worktree)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| {
                    if let Ok(entry) = e {
                        if entry.file_type().is_dir()
                            && entry.file_name() == ".git"
                            && visited.insert(entry.path().to_owned())
                        {
                            return Some(entry);
                        }
                    }
                    None
                })
            {
                let filepath = entry.path();

                // Take the parent
                let filepath = filepath.parent().unwrap();

                // Remove worktree from the path
                let filepath = filepath.strip_prefix(worktree).unwrap();

                // Convert to a string
                let filepath_str = filepath.to_str().unwrap();

                if filepath_str.starts_with(repo) {
                    matches.insert(filepath_str.to_string());
                }

                if !repo.is_empty() {
                    if let Some(index) = filepath_str.find(&find_match) {
                        matches.insert(filepath_str[(index + 1)..].to_string());
                    }
                }
            }
        }

        let mut results = matches.into_iter().collect::<Vec<_>>();
        results.sort();
        results
    }

    pub fn find_repo(
        &self,
        repo: &str,
        include_packages: bool,
        allow_interactive: bool,
    ) -> Option<PathBuf> {
        if let Some(path) = self.omnipath_lookup(repo, include_packages) {
            return Some(path);
        }
        if let Some(path) = self.basic_naive_lookup(repo, include_packages) {
            return Some(path);
        }
        if let Some(path) = self.file_system_lookup(repo, include_packages, allow_interactive) {
            return Some(path);
        }
        None
    }

    fn omnipath_lookup(&self, repo: &str, include_packages: bool) -> Option<PathBuf> {
        if let Ok(repo) = Repo::parse(repo) {
            for path in omnipath_entries() {
                let (path_repo, path_root) = if let Some(package) = &path.package {
                    if !include_packages {
                        continue;
                    }

                    (
                        Repo::parse(package),
                        package_path_from_handle(package).unwrap(),
                    )
                } else {
                    let git_env = git_env(&path.full_path);
                    if let (Some(id), Some(root)) = (git_env.id(), git_env.root()) {
                        (Repo::parse(&id), PathBuf::from(root))
                    } else {
                        continue;
                    }
                };

                if let Ok(path_repo) = path_repo {
                    // TODO: this is very basic checking that ignores user, password, port, scheme;
                    // TODO: maybe we'll want to consider those for more security when doing a
                    // TODO: repository lookup, but that does not seem necessary since this
                    // TODO: considers elements of the omnipath. Maybe there's a security risk I'm
                    // TODO: not seeing yet though.

                    if repo.name != path_repo.name {
                        continue;
                    }

                    if repo.owner.is_some() && path_repo.owner != repo.owner {
                        continue;
                    }

                    if repo.host.is_some() && path_repo.host != repo.host {
                        continue;
                    }

                    return Some(path_root);
                }
            }
        }
        None
    }

    pub fn basic_naive_lookup(&self, repo: &str, include_packages: bool) -> Option<PathBuf> {
        for org in self.orgs.iter() {
            if let Some(path) = org.get_repo_path(repo) {
                if path.is_dir() {
                    return Some(path);
                }
            }
        }

        if include_packages {
            if let Ok(parsed) = Repo::parse(repo) {
                let glob_path = PathBuf::from(&package_root_path())
                    .join(parsed.host.as_deref().unwrap_or("*"))
                    .join(parsed.owner.as_deref().unwrap_or("*"))
                    .join(parsed.name.as_str());

                if let Some(glob_path) = glob_path.to_str() {
                    if let Ok(entries) = glob::glob(glob_path) {
                        for path in entries.into_iter().flatten() {
                            if path.is_dir() {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn file_system_lookup(
        &self,
        repo: &str,
        include_packages: bool,
        allow_interactive: bool,
    ) -> Option<PathBuf> {
        let interactive = allow_interactive && shell_is_interactive();

        let repo_url = if let Ok(repo_url) = Repo::parse(repo) {
            repo_url
        } else {
            return None;
        };
        let rel_path = repo_url.rel_path();

        let mut all_repos = Vec::new();

        if config(".").cd.fast_search {
            self.search_glob_org("")
                .into_iter()
                .map(|result| (result.path.clone(), result.rel_path.clone()))
                .collect::<HashSet<_>>()
                .into_iter()
                .for_each(|(path, rel_path)| {
                    all_repos.push(PathScore {
                        score: 0.0,
                        abspath: PathBuf::from(path),
                        relpath: rel_path,
                    });
                });
        } else {
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

            if include_packages {
                worktrees.push(package_root_path());
            }

            // Prepare a spinner for the research
            let spinner = if interactive {
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
            for worktree in worktrees.iter() {
                for entry in WalkDir::new(worktree).follow_links(true) {
                    if let Ok(entry) = entry {
                        let filetype = entry.file_type();
                        let filepath = entry.path();

                        // We only want places where there's a `.git` directory, since it generally
                        // indicates that we are in a git repository
                        if !filetype.is_dir()
                            || filepath.file_name().is_none()
                            || filepath.file_name().unwrap() != ".git"
                        {
                            continue;
                        }

                        // Take the parent
                        let filepath = filepath.parent().unwrap();

                        if let Some(s) = spinner.clone() {
                            s.set_message(filepath.to_str().unwrap().to_string())
                        }

                        // Check if it ends with the expected rel_path
                        if filepath
                            .to_str()
                            .unwrap()
                            .ends_with(slash_rel_path.as_str())
                        {
                            if let Some(s) = spinner.clone() {
                                s.finish_and_clear()
                            }

                            if start.elapsed() > std::time::Duration::from_secs(1) {
                                omni_print!(format!("{} Setting up your organizations will make repository lookup much faster.", "Did you know?".bold()));
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
                    if let Some(s) = spinner.clone() {
                        s.tick()
                    }
                }
            }

            if let Some(s) = spinner.clone() {
                s.finish_and_clear()
            }
        }

        // Check if any matching value is a perfect match
        for found in all_repos.iter() {
            let git = git_env(&found.abspath.to_string_lossy());

            let origin = match git.origin() {
                Some(origin) => origin,
                None => continue,
            };

            let found_repo = match Repo::parse(origin) {
                Ok(found_repo) => found_repo,
                Err(_) => continue,
            };

            if repo_url.matches(&found_repo) {
                return Some(found.abspath.to_path_buf());
            }
        }

        // Otherwise, we need to score the results and ask the user
        // if there's a match that they want to use
        let mut with_score = all_repos
            .iter_mut()
            .map(|found| {
                // TODO: set scores related to <host>/<org>/<repo>, <org>/<repo>, and <repo> formats too
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

        if interactive {
            let page_size = 7;
            let question = if with_score.len() > 1 {
                requestty::Question::select("did_you_mean_repo")
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(format!(
                        "{} {}",
                        "omni:".light_cyan(),
                        "Did you mean?".yellow()
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
                        "omni:".light_cyan(),
                        "Did you mean?".yellow(),
                        "·".light_black(),
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

impl From<PathScore> for String {
    fn from(val: PathScore) -> Self {
        val.abspath.to_str().unwrap().to_string()
    }
}

impl<'a> From<&'a mut PathScore> for String {
    fn from(val: &'a mut PathScore) -> Self {
        val.abspath.to_str().unwrap().to_string()
    }
}

#[derive(Debug, Clone)]
pub enum OrgError {
    #[allow(dead_code)]
    InvalidHandle(&'static str),
}

#[derive(Default, Debug, Clone)]
pub struct Org {
    pub config: OrgConfig,
    url: Option<Url>,
    pub owner: Option<String>,
    repo: Option<String>,
    enforce_scheme: bool,
    enforce_user: bool,
    enforce_password: bool,
}

impl Org {
    pub fn new(config: OrgConfig) -> Result<Self, OrgError> {
        let parsed_url = safe_normalize_url(&config.handle);
        if parsed_url.is_err() {
            return Err(OrgError::InvalidHandle("url parsing failed"));
        }
        let mut parsed_url = parsed_url.unwrap();
        if parsed_url.scheme().is_empty() || parsed_url.scheme() == "file" {
            parsed_url.set_scheme("https").unwrap();
        }

        if let Some(host) = parsed_url.host_str() {
            if !config.handle.starts_with(parsed_url.scheme()) && !host.contains('.') {
                return Err(OrgError::InvalidHandle("invalid host"));
            }
        } else {
            return Err(OrgError::InvalidHandle("no host"));
        }

        let mut owner = None;
        let mut repo = None;

        if let Some(path) = parsed_url.path_segments() {
            let mut path_segments = path.collect::<Vec<_>>();
            path_segments.retain(|segment| !segment.is_empty());
            if path_segments.len() > 1 {
                repo = Some(path_segments.pop().unwrap().to_string());
            }
            if !path_segments.is_empty() {
                owner = Some(path_segments.pop().unwrap().to_string());
            }
        }

        let enforce_scheme = parsed_url.scheme() == "ssh" || config.handle.contains("://");
        let enforce_user = !parsed_url.username().is_empty();
        let enforce_password = !parsed_url.password().unwrap_or("").is_empty();

        Ok(Self {
            config,
            url: Some(parsed_url),
            owner,
            repo,
            enforce_scheme,
            enforce_user,
            enforce_password,
        })
    }

    pub fn worktree(&self) -> String {
        match &self.config.worktree {
            Some(worktree) => worktree.clone(),
            None => config(".").worktree(),
        }
    }

    pub fn repo_path_format(&self) -> String {
        match &self.config.repo_path_format {
            Some(repo_path_format) => repo_path_format.clone(),
            None => config(".").repo_path_format,
        }
    }

    pub fn get_repo_path(&self, repo: &str) -> Option<PathBuf> {
        // Get the repo git url
        let git_url = self.get_repo_git_url(repo)?;

        Some(format_path_with_template(
            &self.worktree(),
            &git_url,
            &self.repo_path_format(),
        ))
    }

    pub fn is_default(&self) -> bool {
        self.url.is_none()
    }

    pub fn hosts_repo(&self, repo: &str) -> bool {
        if let Ok(url) = safe_git_url_parse(repo) {
            let self_url = match self.url.as_ref() {
                Some(self_url) => self_url,

                // If url is None, it means it's the default, org,
                // and the default org matches all as long as the
                // parsed repository has at least host, owner and
                // name
                None => return url.host.is_some() && url.owner.is_some() && !url.name.is_empty(),
            };

            return (!self.enforce_scheme || self_url.scheme() == url.scheme.to_string())
                && (self_url.port() == url.port || self_url.port_or_known_default() == url.port)
                && (!self.enforce_user
                    || self_url.username() == url.user.as_deref().unwrap_or(""))
                && (!self.enforce_password || self_url.password() == url.token.as_deref())
                && self_url.host_str() == url.host.as_deref()
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
            if let Some(self_url) = self.url.as_ref() {
                // If the repo has a scheme, we need to make sure it matches the org's scheme
                if repo.scheme_prefix && self.enforce_scheme {
                    if let Some(scheme) = &repo.scheme {
                        let org_scheme = self_url.scheme();
                        if scheme != org_scheme {
                            return None;
                        }
                    }
                }

                // If the repo has a host, we need to make sure it matches the org's host
                if let Some(host) = &repo.host {
                    let org_host = self_url.host_str().unwrap();
                    if host != org_host {
                        return None;
                    }
                }

                // If the repo has a user, we need to make sure it matches the org's user (if any)
                if self.enforce_user {
                    if let Some(user) = &repo.user {
                        let org_user = self_url.username();
                        if user != org_user {
                            return None;
                        }
                    }
                }

                // If the repo has a password, we need to make sure it matches the org's password (if any)
                if repo.password.is_some() && self.enforce_password {
                    let token = repo.password.clone().unwrap();
                    if let Some(org_token) = self_url.password() {
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
                    if let Some(org_port) = self_url.port() {
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
            }

            let host = match (&self.url, repo.host) {
                (Some(self_url), _) => self_url.host_str().map(|h| h.to_string()),
                (_, Some(host)) => Some(host),
                (None, None) => return None,
            };

            let owner = match (&self.owner, &repo.owner) {
                (Some(owner), _) => owner,
                (_, Some(owner)) => owner,
                (None, None) => return None,
            };

            let name = if let Some(name) = &self.repo {
                name.clone()
            } else {
                repo.name
            };

            let scheme = match (&self.url, repo.scheme) {
                (Some(self_url), _) => self_url.scheme().to_string(),
                (_, Some(scheme)) => scheme,
                (None, None) => "https".to_string(),
            };
            let scheme = if scheme == "ssh" {
                Scheme::Ssh
            } else {
                Scheme::Https
            };

            let user = match (&self.url, repo.user) {
                (Some(self_url), _) => {
                    if self_url.username().is_empty() {
                        None
                    } else {
                        Some(self_url.username().to_string())
                    }
                }
                (_, Some(user)) => Some(user),
                (None, None) => None,
            };

            let password = match (&self.url, repo.password) {
                (Some(self_url), _) => self_url.password().map(|t| t.to_string()),
                (_, Some(password)) => Some(password),
                (None, None) => None,
            };

            let port = match (&self.url, repo.port) {
                (Some(self_url), _) => self_url.port(),
                (_, Some(port)) => Some(port),
                (None, None) => None,
            };

            let git_url = GitUrl {
                host,
                name: name.clone(),
                owner: Some(owner.clone()),
                organization: None,
                fullname: format!("{}/{}", owner.clone(), name.clone()),
                scheme,
                user,
                token: password,
                port,
                path: format!(
                    "{}{}/{}",
                    if scheme == Scheme::Ssh { "" } else { "/" },
                    owner.clone(),
                    name,
                ),
                git_suffix: repo.git_suffix,
                scheme_prefix: scheme != Scheme::Ssh || port.is_some(),
            };

            return Some(git_url);
        }
        None
    }
}

#[derive(Debug, Clone)]
pub enum RepoError {
    ParseError,
}

#[derive(Debug, Clone)]
pub struct Repo {
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
                let mut parts = repo.split('/').collect::<Vec<&str>>();

                let mut name = parts.pop().unwrap().to_string();
                let mut git_suffix = false;
                if name.ends_with(".git") {
                    git_suffix = true;
                    name = name[..name.len() - 4].to_string();
                }

                let owner = if !parts.is_empty() {
                    Some(parts.pop().unwrap().to_string())
                } else {
                    None
                };

                let host = if !parts.is_empty() && parts[0].contains('.') {
                    Some(parts[0].to_string())
                } else {
                    None
                };

                return Ok(Self {
                    name,
                    owner,
                    host,
                    port: None,
                    scheme: None,
                    scheme_prefix: false,
                    git_suffix,
                    user: None,
                    password: None,
                    rel_path: OnceCell::new(),
                });
            }
        }
        Err(RepoError::ParseError)
    }

    pub fn matches(&self, other: &Self) -> bool {
        if self.name != other.name {
            return false;
        }

        if self.owner.is_some() && other.owner.is_some() && self.owner != other.owner {
            return false;
        }

        if self.host.is_some() && other.host.is_some() && self.host != other.host {
            return false;
        }

        if self.port.is_some() && other.port.is_some() && self.port != other.port {
            return false;
        }

        if self.scheme.is_some() && other.scheme.is_some() && self.scheme != other.scheme {
            return false;
        }

        if self.user.is_some() && other.user.is_some() && self.user != other.user {
            return false;
        }

        if self.password.is_some() && other.password.is_some() && self.password != other.password {
            return false;
        }

        true
    }

    pub fn partial_resolve(&self, repo: &str) -> Result<Self, RepoError> {
        let parsed = Repo::parse(repo)?;
        let mut resolved = self.clone();

        if parsed.name != self.name {
            resolved.name = parsed.name;
            resolved.git_suffix = parsed.git_suffix;
        } else if parsed.git_suffix != resolved.git_suffix {
            resolved.git_suffix = parsed.git_suffix;
        }

        if parsed.owner.is_some() {
            resolved.owner = parsed.owner;
        }

        if parsed.host.is_some() {
            resolved.host = parsed.host;
        }

        if parsed.port.is_some() {
            resolved.port = parsed.port;
        }

        if parsed.scheme.is_some() {
            resolved.scheme = parsed.scheme;
            resolved.scheme_prefix = parsed.scheme_prefix;
        }

        if parsed.user.is_some() {
            resolved.user = parsed.user;
        }

        if parsed.password.is_some() {
            resolved.password = parsed.password;
        }

        Ok(resolved)
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
                let mut parts = path_format.split('/').collect::<Vec<&str>>();
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
                    path.push('/');
                }

                // Remove the trailing slash
                if path.ends_with('/') {
                    path = path[..path.len() - 1].to_string();
                }

                path
            })
            .clone()
    }
}

impl std::fmt::Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.host.is_none() || self.owner.is_none() || self.name.is_empty() {
            return write!(f, "");
        }

        let mut repo = String::new();

        if self.scheme_prefix {
            if let Some(scheme) = &self.scheme {
                repo.push_str(scheme.as_str());
                repo.push_str("://");
            }
        }

        if self.user.is_some() || self.password.is_some() {
            if let Some(user) = &self.user {
                repo.push_str(user.as_str());
            }
            if let Some(password) = &self.password {
                repo.push(':');
                repo.push_str(password.as_str());
            }
            repo.push('@');
        }

        repo.push_str(self.host.clone().unwrap().as_str());
        if let Some(port) = &self.port {
            repo.push(':');
            repo.push_str(port.to_string().as_str());
        }

        if !self.scheme_prefix && self.scheme.clone().unwrap_or_default() == "ssh" {
            repo.push(':');
        } else {
            repo.push('/');
        }
        repo.push_str(self.owner.clone().unwrap().as_str());

        repo.push('/');
        repo.push_str(self.name.as_str());

        if self.git_suffix {
            repo.push_str(".git");
        }

        write!(f, "{}", repo)
    }
}
