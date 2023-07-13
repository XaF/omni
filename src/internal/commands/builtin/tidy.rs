use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;

use getopts::Options;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use once_cell::sync::OnceCell;
use walkdir::WalkDir;

use crate::internal::config::config;
use crate::internal::config::global_config_loader;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigSource;
use crate::internal::config::SyntaxOptArg;
use crate::internal::git::format_path;
use crate::internal::git::safe_git_url_parse;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::ConfigLoader;
use crate::internal::ENV;
use crate::internal::ORG_LOADER;
use crate::omni_error;
use crate::omni_info;

#[derive(Debug, Clone)]
struct TidyCommandArgs {
    yes: bool,
    search_paths: HashSet<String>,
    unparsed: Vec<String>,
}

impl TidyCommandArgs {
    fn default() -> Self {
        Self {
            yes: false,
            search_paths: HashSet::new(),
            unparsed: vec![],
        }
    }

    fn parse(argv: Vec<String>) -> Self {
        let mut opts = Options::new();
        opts.optflag("y", "yes", "-");
        opts.optmulti(
            "p",
            "search-path",
            "-",
            "Also search this path for git repositories",
        );

        let matches = match opts.parse(&argv) {
            Ok(m) => m,
            Err(e) => {
                omni_error!(e.to_string());
                exit(1);
            }
        };

        let mut search_paths = HashSet::new();
        for path in matches.opt_strs("search-path") {
            search_paths.insert(path);
        }

        Self {
            yes: matches.opt_present("yes"),
            search_paths: search_paths,
            unparsed: matches.free,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TidyCommand {
    cli_args: OnceCell<TidyCommandArgs>,
}

impl TidyCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &TidyCommandArgs {
        self.cli_args.get_or_init(|| TidyCommandArgs::default())
    }

    pub fn name(&self) -> Vec<String> {
        vec!["tidy".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Organize your git repositories using the configured format\n",
                "\n",
                "This will offer to organize your git repositories, moving them from their current ",
                "path to the path they should be at if they had been cloned using \x1B[3momni ",
                "clone\x1B[0m. This is useful if you have a bunch of repositories that you have ",
                "cloned manually, and you want to start using \x1B[3momni\x1B[0m, or if you changed ",
                "your mind on the repo path format you wish to use.",
            )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            arguments: vec![],
            options: vec![
                SyntaxOptArg {
                    name: "--yes".to_string(),
                    desc: Some(
                        "Do not ask for confirmation before organizing repositories".to_string(),
                    ),
                },
                SyntaxOptArg {
                    name: "--search-path".to_string(),
                    desc: Some(
                        concat!(
                            "Extra path to search git repositories to tidy up ",
                            "(repeat as many times as you need)",
                        )
                        .to_string(),
                    ),
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["Git commands".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if let Err(_) = self.cli_args.set(TidyCommandArgs::parse(argv)) {
            unreachable!();
        }

        if self.cli_args().unparsed.len() > 0 {
            omni_error!("too many arguments");
            exit(1);
        }

        let repositories = self.list_repositories();

        // Filter the repositories that are already organized
        let repositories = repositories
            .iter()
            .filter(|r| !r.organized)
            .collect::<Vec<_>>();

        if repositories.is_empty() {
            omni_info!("Everything is already tidied up! \u{1F389}"); // party popper emoji code
            exit(0);
        }

        let selected_repositories = if self.cli_args().yes {
            repositories
        } else if !ENV.interactive_shell {
            omni_info!(format!(
                "Found {} repositor{} to tidy up:",
                format!("{}", repositories.len()).underline(),
                if repositories.len() > 1 { "ies" } else { "y" }
            ));

            for repository in repositories.iter() {
                eprintln!("{}", repository.to_string());
            }

            omni_info!(format!(
                "use {} to organize them",
                "--yes".to_string().light_blue()
            ));
            exit(0);
        } else {
            let choices = repositories
                .iter()
                .map(|r| (r.to_string(), true))
                .collect::<Vec<_>>();

            let question = requestty::Question::multi_select("select_suggestions")
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message(format!(
                    "{} Found {} repositor{} to tidy up:",
                    "omni:".to_string().light_cyan(),
                    format!("{}", repositories.len()).underline(),
                    if repositories.len() > 1 { "ies" } else { "y" },
                ))
                .transform(|selected, _, backend| {
                    write!(backend, "{} selected", format!("{}", selected.len()).bold())
                })
                .choices_with_default(choices)
                .should_loop(false)
                .build();

            let mut selected_repositories = Vec::new();
            match requestty::prompt_one(question) {
                Ok(answer) => match answer {
                    requestty::Answer::ListItems(items) => {
                        for item in items {
                            selected_repositories.push(repositories[item.index]);
                        }
                    }
                    _ => unreachable!(),
                },
                Err(err) => {
                    println!("{}", format!("[✘] {:?}", err).red());
                    exit(0);
                }
            }
            selected_repositories
        };

        if selected_repositories.is_empty() {
            omni_info!("Nothing to do! \u{1F971}"); // yawning face emoji
            exit(0);
        }

        // Let's create a progress bar if the shell is interactive
        let progress_bar = if ENV.interactive_shell {
            let progress_bar = ProgressBar::new(selected_repositories.len() as u64);
            progress_bar.tick();
            Some(progress_bar)
        } else {
            None
        };
        let printstr = |s: String| {
            if let Some(progress_bar) = &progress_bar {
                progress_bar.println(s);
            } else {
                eprintln!("{}", s);
            }
        };

        // We go over the repositories and try to move them in their position;
        // since some repositories might be depending on other repositories being
        // moved first, we try looping until we can't move any more repositories
        let mut repos_to_organize = selected_repositories.clone();
        while !repos_to_organize.is_empty() {
            let organize_this_loop = repos_to_organize.clone();
            let mut moved = HashSet::new();
            for repository in organize_this_loop.iter() {
                let repo = repository.clone();
                if repo.organize(&printstr) {
                    moved.insert(repository.clone());
                    progress_bar.as_ref().map(|pb| pb.inc(1));
                }
            }

            if moved.is_empty() {
                for repository in repos_to_organize.iter() {
                    printstr(format!(
                        "{} Skipping {}",
                        "[✘]".to_string().light_red(),
                        format!("{}", repository.to_string())
                    ));
                    progress_bar.as_ref().map(|pb| pb.inc(1));
                }

                break;
            }

            repos_to_organize.retain(|r| !moved.contains(r));
        }

        // Clear the progress bar once we're finished
        progress_bar.map(|pb| pb.finish_and_clear());

        // TODO: should we offer to up the moved repositories ?
        // TODO: should we support --up-all which will offer to up _all_ the repositories ?

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        true
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {
        // TODO: if the last parameter before completion is `search-path`,
        // TODO: we should autocomplete with the file system paths
        println!("--search-path");
        println!("-y");
        println!("--yes");
        exit(0);
    }

    fn list_repositories(&self) -> Vec<TidyGitRepo> {
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

        let mut worktrees = HashSet::new();

        // We want to search for repositories in all our worktrees
        let config = config(".");
        worktrees.insert(config.worktree().into());
        for org in ORG_LOADER.orgs.iter() {
            let path = PathBuf::from(org.worktree());
            if path.is_dir() {
                worktrees.insert(path);
            }
        }

        // But also in any search path that was provided on the command line
        for search_path in self.cli_args().search_paths.iter() {
            let path = PathBuf::from(search_path);
            if path.is_dir() {
                worktrees.insert(path);
            }
        }

        // Cleanup the paths by removing each path for which
        // the parent is also in the list
        let mut worktrees = worktrees.into_iter().collect::<Vec<_>>();
        worktrees.sort_by(|a, b| a.cmp(b));
        let worktrees = worktrees
            .clone()
            .into_iter()
            .filter(|path| {
                !worktrees
                    .iter()
                    .any(|other| path != other && path.starts_with(format!("{}/", other.display())))
            })
            .collect::<Vec<_>>();

        let mut repositories = HashSet::new();
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

                    spinner.clone().map(|s| {
                        s.set_message(format!("Searching: {}", filepath.to_str().unwrap()))
                    });

                    // Convert to a string
                    let filepath_str = filepath.to_str().unwrap();

                    repositories.insert(filepath_str.to_string());
                }
                spinner.clone().map(|s| s.tick());
            }
        }

        spinner
            .clone()
            .map(|s| s.set_message("Analyzing repositories..."));

        let mut repositories = repositories.into_iter().collect::<Vec<_>>();
        repositories.sort();

        let mut tidy_repos = Vec::new();
        for repository in repositories.iter() {
            spinner
                .clone()
                .map(|s| s.set_message(format!("Analyzing: {}", repository)));

            if let Some(tidy_repo) = TidyGitRepo::new(repository) {
                tidy_repos.push(tidy_repo);
            }

            spinner.clone().map(|s| s.tick());
        }

        spinner.clone().map(|s| s.finish_and_clear());

        tidy_repos.into_iter().collect::<Vec<_>>()
    }
}

#[derive(Debug, Clone)]
struct TidyGitRepo {
    current_path: PathBuf,
    expected_path: PathBuf,
    organized: bool,
    organizable: bool,
}

impl TidyGitRepo {
    fn new(path: &str) -> Option<Self> {
        let git_env = git_env(path);
        if !git_env.in_repo() || git_env.origin().is_none() {
            return None;
        }
        let origin_url = git_env.origin().unwrap();

        let path = PathBuf::from(path);

        // Try and find the expected path
        let mut expected_path = None;

        // We check first among the orgs
        for org in ORG_LOADER.orgs.iter() {
            if let Some(repo_path) = org.get_repo_path(&origin_url) {
                expected_path = Some(PathBuf::from(repo_path));
                break;
            }
        }

        // If no match, check if the link is a full git url, in which case
        // we can clone to the default worktree
        if expected_path.is_none() {
            if let Ok(repo_url) = safe_git_url_parse(&origin_url) {
                if repo_url.scheme.to_string() != "file"
                    && repo_url.name != ""
                    && repo_url.owner.is_some()
                    && repo_url.host.is_some()
                {
                    let config = config(".");
                    let worktree = config.worktree();
                    let repo_path = format_path(&worktree, &repo_url);
                    expected_path = Some(PathBuf::from(repo_path));
                }
            }
        }

        if expected_path.is_none() {
            return None;
        }
        let expected_path = expected_path.unwrap();

        Some(Self {
            current_path: path.clone(),
            expected_path: expected_path.clone(),
            organized: path == expected_path,
            organizable: !expected_path.exists(),
        })
    }

    fn organize<T>(&self, println: T) -> bool
    where
        T: Fn(String),
    {
        if self.expected_path.exists() {
            return false;
        }

        // Create the parent directory prior to the move
        let expected_parent = Path::new(&self.expected_path).parent().unwrap();
        if let Err(_err) = std::fs::create_dir_all(expected_parent) {
            return false;
        }

        // Move the repository to the expected path
        if let Err(_err) = std::fs::rename(&self.current_path, &self.expected_path) {
            return false;
        }

        println(format!(
            "{} Moved {}",
            "[✔]".to_string().light_green(),
            self.to_string(),
        ));

        // TODO: edit OMNIPATH ?
        self.edit_config(&println);

        // Cleanup the parents as long as they are empty directories
        let mut parent = Path::new(&self.current_path);
        while let Some(path) = parent.parent() {
            if let Err(_err) = std::fs::remove_dir(path) {
                break;
            }
            parent = path;
        }

        true
    }

    // fn edit_environment<T>(&self, printenv: T) -> bool {
    // where
    // T: Fn(String),
    // {
    // // TODO: edit OMNIPATH ?
    // }

    fn edit_config<T>(&self, println: T) -> bool
    where
        T: Fn(String),
    {
        let config = global_config_loader().raw_config.clone();
        let exact_match = format!("{}", self.current_path.to_str().unwrap());
        let prefix_match = format!("{}/", self.current_path.to_str().unwrap());
        let mut files_to_edit = HashSet::new();
        if let Some(config_path) = config.get_as_table("path") {
            for key in config_path.keys() {
                if let Some(path_list) = config_path.get(key) {
                    if let Some(path_list) = path_list.as_array() {
                        for value in path_list {
                            if let Some(path_value) = value.as_str() {
                                if path_value == exact_match
                                    || path_value.starts_with(&prefix_match)
                                {
                                    match value.get_source() {
                                        ConfigSource::File(path) => {
                                            files_to_edit.insert(path.clone());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut any_edited = false;
        for file in files_to_edit.iter() {
            any_edited = self.edit_config_file(file, &println) || any_edited;
        }

        any_edited
    }

    fn edit_config_file<T>(&self, file_path: &str, println: T) -> bool
    where
        T: Fn(String),
    {
        let mut edited = false;
        let exact_match = format!("{}", self.current_path.to_str().unwrap());
        let prefix_match = format!("{}/", self.current_path.to_str().unwrap());
        let result = ConfigLoader::edit_user_config_file(file_path.to_string(), |config_value| {
            if let Some(config_path) = config_value.get_as_table_mut("path") {
                for (_key, path_list) in config_path.iter_mut() {
                    if let Some(path_list) = path_list.as_array_mut() {
                        for path_config_value in path_list.iter_mut() {
                            if let Some(path_value) = path_config_value.as_str_mut() {
                                if *path_value == exact_match {
                                    *path_value = self.expected_path.to_str().unwrap().to_string();
                                    edited = true;
                                } else if path_value.starts_with(&prefix_match) {
                                    *path_value = format!(
                                        "{}/{}",
                                        self.expected_path.to_str().unwrap(),
                                        &path_value[prefix_match.len()..]
                                    );
                                    edited = true;
                                }
                            }
                        }
                    }
                }
            }

            edited
        });

        if edited {
            if let Err(err) = result {
                println(format!(
                    "{} Failed to update {} to {} in {}: {:?}",
                    "[-]".to_string().light_black(),
                    self.current_path.to_str().unwrap().to_string().light_red(),
                    self.expected_path
                        .to_str()
                        .unwrap()
                        .to_string()
                        .light_green(),
                    file_path.to_string().light_yellow(),
                    err,
                ));
            } else {
                println(format!(
                    "{} Updated {} to {} in {}",
                    "[-]".to_string().light_black(),
                    self.current_path.to_str().unwrap().to_string().light_red(),
                    self.expected_path
                        .to_str()
                        .unwrap()
                        .to_string()
                        .light_green(),
                    file_path.to_string().light_yellow(),
                ));
            }
        }

        edited
    }
}

impl Eq for TidyGitRepo {}

impl PartialEq for TidyGitRepo {
    fn eq(&self, other: &Self) -> bool {
        self.current_path == other.current_path
    }
}

impl Hash for TidyGitRepo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.current_path.hash(state);
    }
}

impl ToString for TidyGitRepo {
    fn to_string(&self) -> String {
        let mut s = String::new();

        if self.organized {
            // s.push_str(&format!("{} {}", "✓", self.current_path.to_str().unwrap()));
            s.push_str(&format!("{}", self.current_path.to_str().unwrap()));
            return s.light_green();
        }

        s.push_str(&format!("{}", self.current_path.to_str().unwrap()).light_red());
        s.push_str(" \u{2192} "); // arrow to the right in UTF-8

        let dest = format!("{}", self.expected_path.to_str().unwrap());

        if self.organizable {
            s.push_str(&dest.light_green());
        } else {
            s.push_str(&dest.light_yellow());
            s.push_str(&format!(" \u{26A0}\u{FE0F}").light_yellow()); // small warning sign in UTF-8
        }

        s.light_yellow()
    }
}
