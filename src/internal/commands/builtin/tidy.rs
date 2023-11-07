use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;

use clap;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use once_cell::sync::OnceCell;
use walkdir::WalkDir;

use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::path::global_omnipath_entries;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::config;
use crate::internal::config::global_config_loader;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigSource;
use crate::internal::config::SyntaxOptArg;
use crate::internal::git::format_path;
use crate::internal::git::package_path_from_handle;
use crate::internal::git::package_root_path;
use crate::internal::git::path_entry_config;
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
    up_all: bool,
    up_args: Vec<String>,
}

impl TidyCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("yes")
                    .short('y')
                    .long("yes")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("search-path")
                    .short('p')
                    .long("search-path")
                    .action(clap::ArgAction::Append),
            )
            .arg(
                clap::Arg::new("up-all")
                    .long("up-all")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("up-args")
                    .action(clap::ArgAction::Append)
                    .last(true),
            )
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["tidy".to_string()]);
                }
                clap::error::ErrorKind::DisplayVersion => {
                    unreachable!("version flag is disabled");
                }
                _ => {
                    let err_str = format!("{}", err);
                    let err_str = err_str
                        .split('\n')
                        .take_while(|line| !line.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let err_str = err_str.trim_start_matches("error: ");
                    omni_error!(err_str);
                }
            }
            exit(1);
        }

        let matches = matches.unwrap();

        let search_paths =
            if let Some(search_paths) = matches.get_many::<String>("search-path").clone() {
                search_paths
                    .into_iter()
                    .map(|arg| arg.to_string())
                    .collect::<HashSet<_>>()
            } else {
                HashSet::new()
            };

        let up_args = if let Some(up_args) = matches.get_many::<String>("up-args").clone() {
            up_args
                .into_iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        Self {
            yes: *matches.get_one::<bool>("yes").unwrap_or(&false),
            search_paths: search_paths,
            up_all: *matches.get_one::<bool>("up-all").unwrap_or(&false),
            up_args: up_args,
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
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
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
            parameters: vec![
                SyntaxOptArg {
                    name: "--yes".to_string(),
                    desc: Some(
                        "Do not ask for confirmation before organizing repositories".to_string(),
                    ),
                    required: false,
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
                    required: false,
                },
                SyntaxOptArg {
                    name: "--up-all".to_string(),
                    desc: Some(
                        concat!(
                            "Run \x1B[3momni up\x1B[0m in all the repositories ",
                            "with an omni configuration; any argument passed to the ",
                            "\x1B[3mtidy\x1B[0m command after \x1B[3m--\x1B[0m will ",
                            "be passed to \x1B[3momni up\x1B[0m (e.g. ",
                            "\x1B[3momni tidy --up-all -- --update-repository\x1B[0m)",
                        )
                        .to_string(),
                    ),
                    required: false,
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

        // Find the packages in the path that might not exist
        let omnipath_entries = global_omnipath_entries();
        let missing_packages = omnipath_entries
            .iter()
            .filter(|pe| pe.is_package() && !pe.package_path().unwrap_or(PathBuf::new()).exists())
            .collect::<Vec<_>>();

        // Find all the repositories
        let all_repositories = self.list_repositories();

        // Filter the repositories that are already organized
        let repositories = all_repositories
            .iter()
            .filter(|r| !r.organized)
            .collect::<Vec<_>>();

        if repositories.is_empty() && missing_packages.is_empty() {
            if self.cli_args().up_all {
                self.up_repositories(&all_repositories);
            } else {
                omni_info!("Everything is already tidied up! \u{1F389}"); // party popper emoji code
            }
            exit(0);
        }

        if !missing_packages.is_empty() {
            omni_info!(format!(
                "Found {} missing package{}, cloning them.",
                format!("{}", missing_packages.len()).underline(),
                if missing_packages.len() > 1 { "s" } else { "" }
            ));

            let current_exe = std::env::current_exe();
            if current_exe.is_err() {
                omni_error!("failed to get current executable path");
                exit(1);
            }
            let current_exe = current_exe.unwrap();

            for package in missing_packages.iter() {
                // Call 'omni clone' the missing packages but in a separate process so we don't
                // kill the current process; however we want to make sure we link stdin, stdout and
                // stderr
                let mut omni_clone_command = std::process::Command::new(current_exe.clone());
                omni_clone_command.arg("clone");
                omni_clone_command.arg("--package");
                omni_clone_command.arg(package.package.clone().unwrap());
                omni_clone_command.env_remove("OMNI_FORCE_UPDATE");
                omni_clone_command.env("OMNI_SKIP_UPDATE", "1");

                // Run the process synchronously but capture the exit code
                let child = omni_clone_command.spawn();
                match child {
                    Ok(mut child) => {
                        let status = child.wait();
                        match status {
                            Ok(status) => {
                                if !status.success() {
                                    omni_error!(format!(
                                        "failed to clone package: {}",
                                        package.package.clone().unwrap()
                                    ));
                                }
                            }
                            Err(err) => {
                                omni_error!(format!("failed to wait on process: {}", err));
                                exit(1);
                            }
                        }
                    }
                    Err(err) => {
                        omni_error!(format!("failed to spawn process: {}", err));
                        exit(1);
                    }
                }
            }

            if repositories.is_empty() {
                exit(0);
            }
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
            if self.cli_args().up_all {
                self.up_repositories(&all_repositories);
            } else {
                omni_info!("Nothing to do! \u{1F971}"); // yawning face emoji
            }
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
                if repository.organize(&printstr) {
                    moved.insert(repository);
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

        if self.cli_args().up_all {
            self.up_repositories(&all_repositories);
        }

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
        println!("--up-all");
        println!("-h");
        println!("--help");
        exit(0);
    }

    fn list_repositories(&self) -> Vec<TidyGitRepo> {
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

        // And the package path
        worktrees.insert(PathBuf::from(package_root_path()));

        // And now we list the repositories
        TidyGitRepo::list_repositories(worktrees)
    }

    fn up_repositories(&self, repositories: &Vec<TidyGitRepo>) {
        let current_exe = std::env::current_exe();
        if current_exe.is_err() {
            omni_error!("failed to get current executable path");
            exit(1);
        }
        let current_exe = current_exe.unwrap();

        // We get all paths so we don't have to know if the repository
        // was organized or not; we also use an hashset to remove duplicates
        // and then convert back to a vector to sort the paths
        let mut all_paths = repositories
            .iter()
            .map(|repo| vec![repo.current_path.clone(), repo.expected_path.clone()])
            .flatten()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        all_paths.sort();

        let mut any_error = false;
        for repo_path in all_paths.iter() {
            if !repo_path.exists() {
                continue;
            }

            let config_exists = vec![".omni.yaml", ".omni/config.yaml"]
                .iter()
                .any(|file| repo_path.join(file).exists());
            if !config_exists {
                continue;
            }

            let path_entry = path_entry_config(repo_path.to_str().unwrap());
            if !path_entry.is_valid() {
                continue;
            }

            let location = match path_entry.package {
                Some(ref package) => format!(
                    "{}:{}",
                    "package".to_string().underline(),
                    package.to_string().light_cyan(),
                ),
                None => path_entry.as_string().light_cyan(),
            };

            omni_info!(format!(
                "running {} in {}",
                "omni up".to_string().light_yellow(),
                location,
            ));

            let mut omni_up_command = std::process::Command::new(current_exe.clone());
            omni_up_command.arg("up");
            omni_up_command.args(self.cli_args().up_args.clone());
            omni_up_command.current_dir(repo_path.clone());
            omni_up_command.env_remove("OMNI_FORCE_UPDATE");
            omni_up_command.env("OMNI_SKIP_UPDATE", "1");

            let child = omni_up_command.spawn();
            match child {
                Ok(mut child) => {
                    let status = child.wait();
                    match status {
                        Ok(status) => {
                            if !status.success() {
                                any_error = true;
                            }
                        }
                        Err(err) => {
                            omni_error!(format!("failed to wait on process: {}", err));
                        }
                    }
                }
                Err(err) => {
                    omni_error!(format!("failed to spawn process: {}", err));
                }
            }
        }

        omni_info!(format!("done!").light_green());
        if any_error {
            omni_error!(format!("some errors occurred!").light_red());
            exit(1);
        }
    }
}

#[derive(Debug, Clone)]
pub struct TidyGitRepo {
    current_path: PathBuf,
    expected_path: PathBuf,
    pub origin_url: String,
    organized: bool,
    organizable: bool,
}

impl TidyGitRepo {
    pub fn list_repositories(worktrees: HashSet<PathBuf>) -> Vec<Self> {
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

        let worktrees = worktrees
            .into_iter()
            .map(|path| abs_path(path))
            .collect::<HashSet<_>>();

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

            if let Some(tidy_repo) = Self::new(repository) {
                tidy_repos.push(tidy_repo);
            }

            spinner.clone().map(|s| s.tick());
        }

        spinner.clone().map(|s| s.finish_and_clear());

        tidy_repos.into_iter().collect::<Vec<_>>()
    }

    pub fn new_with_paths(current_path: PathBuf, expected_path: PathBuf) -> Self {
        Self {
            current_path: current_path.clone(),
            expected_path: expected_path.clone(),
            origin_url: "".to_string(),
            organized: current_path == expected_path,
            organizable: false,
        }
    }

    fn new(path: &str) -> Option<Self> {
        let git_env = git_env(path);
        if !git_env.in_repo() || !git_env.has_origin() {
            return None;
        }
        let origin_url = git_env.origin().unwrap();

        let path = PathBuf::from(path);

        // Try and find the expected path
        let mut expected_path = None;

        // If the path is in the package tree, we need to compare to the
        // expected package path
        if path.starts_with(package_root_path()) {
            expected_path = package_path_from_handle(&origin_url);
        } else {
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
        }

        if expected_path.is_none() {
            return None;
        }
        let expected_path = expected_path.unwrap();

        Some(Self {
            current_path: path.clone(),
            expected_path: expected_path.clone(),
            origin_url: origin_url.to_string(),
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

    pub fn edit_config<T>(&self, println: T) -> bool
    where
        T: Fn(String),
    {
        let mut files_to_edit = HashSet::new();

        let config = global_config_loader().raw_config.clone();
        let current_path = path_entry_config(self.current_path.to_str().unwrap());

        if let Some(config_path) = config.get_as_table("path") {
            for key in config_path.keys() {
                if let Some(path_list) = config_path.get(key) {
                    if let Some(path_list) = path_list.as_array() {
                        for value in path_list {
                            let path_entry = PathEntryConfig::from_config_value(&value);
                            if path_entry.starts_with(&current_path) {
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
        let current_path = path_entry_config(self.current_path.to_str().unwrap());
        let expected_path = path_entry_config(self.expected_path.to_str().unwrap());

        let result = ConfigLoader::edit_user_config_file(file_path.to_string(), |config_value| {
            if let Some(config_path) = config_value.get_as_table_mut("path") {
                for (_key, path_list) in config_path.iter_mut() {
                    if let Some(path_list) = path_list.as_array_mut() {
                        for value in path_list.iter_mut() {
                            let mut path_entry = PathEntryConfig::from_config_value(&value);
                            if path_entry.replace(&current_path, &expected_path) {
                                *value = path_entry.as_config_value().clone();
                                edited = true;
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
