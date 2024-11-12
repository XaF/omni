use std::collections::BTreeMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;

use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use walkdir::WalkDir;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::path::global_omnipath_entries;
use crate::internal::commands::utils::abs_path;
use crate::internal::commands::Command;
use crate::internal::config::config;
use crate::internal::config::global_config_loader;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigSource;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::format_path_with_template;
use crate::internal::git::package_path_from_handle;
use crate::internal::git::package_root_path;
use crate::internal::git::path_entry_config;
use crate::internal::git::safe_git_url_parse;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::ConfigLoader;
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

impl From<BTreeMap<String, ParseArgsValue>> for TidyCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let yes = matches!(
            args.get("yes"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let search_paths = match args.get("search_path") {
            Some(ParseArgsValue::ManyString(search_paths)) => {
                search_paths.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };
        let up_all = matches!(
            args.get("up_all"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let up_args = match args.get("up_args") {
            Some(ParseArgsValue::ManyString(up_args)) => {
                up_args.iter().flat_map(|v| v.clone()).collect()
            }
            _ => vec![],
        };

        Self {
            yes,
            search_paths,
            up_all,
            up_args,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TidyCommand {}

impl TidyCommand {
    pub fn new() -> Self {
        Self {}
    }

    fn list_repositories(&self, args: &TidyCommandArgs) -> Vec<TidyGitRepo> {
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
        for search_path in args.search_paths.iter() {
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

    fn up_repositories(&self, repositories: &[TidyGitRepo], args: &TidyCommandArgs) {
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
            .flat_map(|repo| vec![repo.current_path.clone(), repo.expected_path.clone()])
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        all_paths.sort();

        let mut any_error = false;
        for repo_path in all_paths.iter() {
            if !repo_path.exists() {
                continue;
            }

            let config_exists = [".omni.yaml", ".omni/config.yaml"]
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
                Some(ref package) => format!("{}:{}", "package".underline(), package.light_cyan()),
                None => path_entry.to_string().light_cyan(),
            };

            omni_info!(format!(
                "running {} in {}",
                "omni up".light_yellow(),
                location,
            ));

            let mut omni_up_command = std::process::Command::new(current_exe.clone());
            omni_up_command.arg("up");
            omni_up_command.args(args.up_args.clone());
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

        omni_info!("done!".light_green());
        if any_error {
            omni_error!("some errors occurred!".light_red());
            exit(1);
        }
    }
}

impl BuiltinCommand for TidyCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["tidy".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
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

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["--yes".to_string()],
                    desc: Some(
                        "Do not ask for confirmation before organizing repositories".to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-P".to_string(), "--search-path".to_string()],
                    desc: Some(
                        concat!(
                            "Extra path to search git repositories to tidy up ",
                            "(repeat as many times as you need)",
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--up-all".to_string()],
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
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["up args".to_string()],
                    desc: Some(
                        concat!(
                            "Arguments to pass to \x1B[3momni up\x1B[0m when running ",
                            "with \x1B[3m--up-all\x1B[0m",
                        )
                        .to_string(),
                    ),
                    last_arg_double_hyphen: true,
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["Git commands".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        let command = Command::Builtin(self.clone_boxed());
        let args = TidyCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        // Find the packages in the path that might not exist
        let omnipath_entries = global_omnipath_entries();
        let missing_packages = omnipath_entries
            .iter()
            .filter(|pe| pe.is_package() && !pe.package_path().unwrap_or_default().exists())
            .collect::<Vec<_>>();

        // Find all the repositories
        let all_repositories = self.list_repositories(&args);

        // Filter the repositories that are already organized
        let repositories = all_repositories
            .iter()
            .filter(|r| !r.organized)
            .collect::<Vec<_>>();

        if repositories.is_empty() && missing_packages.is_empty() {
            if args.up_all {
                self.up_repositories(&all_repositories, &args);
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

        let selected_repositories = if args.yes {
            repositories
        } else if !shell_is_interactive() {
            omni_info!(format!(
                "Found {} repositor{} to tidy up:",
                format!("{}", repositories.len()).underline(),
                if repositories.len() > 1 { "ies" } else { "y" }
            ));

            for repository in repositories.iter() {
                eprintln!("{}", repository);
            }

            omni_info!(format!("use {} to organize them", "--yes".light_blue()));
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
                    "omni:".light_cyan(),
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
            if args.up_all {
                self.up_repositories(&all_repositories, &args);
            } else {
                omni_info!("Nothing to do! \u{1F971}"); // yawning face emoji
            }
            exit(0);
        }

        // Let's create a progress bar if the shell is interactive
        let progress_bar = if shell_is_interactive() {
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
                if repository.organize(printstr) {
                    moved.insert(repository);
                    if let Some(pb) = progress_bar.as_ref() {
                        pb.inc(1)
                    }
                }
            }

            if moved.is_empty() {
                for repository in repos_to_organize.iter() {
                    printstr(format!("{} Skipping {}", "[✘]".light_red(), repository,));
                    if let Some(pb) = progress_bar.as_ref() {
                        pb.inc(1)
                    }
                }

                break;
            }

            repos_to_organize.retain(|r| !moved.contains(r));
        }

        // Clear the progress bar once we're finished
        if let Some(pb) = progress_bar {
            pb.finish_and_clear()
        }

        // TODO: should we offer to up the moved repositories ?

        if args.up_all {
            self.up_repositories(&all_repositories, &args);
        }

        exit(0);
    }

    fn autocompletion(&self) -> bool {
        true
    }

    fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        // TODO: if the last parameter before completion is `search-path`,
        // TODO: we should autocomplete with the file system paths
        println!("--search-path");
        println!("-y");
        println!("--yes");
        println!("--up-all");
        println!("-h");
        println!("--help");

        Ok(())
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
        let spinner = if shell_is_interactive() {
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

        let worktrees = worktrees.into_iter().map(abs_path).collect::<HashSet<_>>();

        // Cleanup the paths by removing each path for which
        // the parent is also in the list
        let mut worktrees = worktrees.into_iter().collect::<Vec<_>>();
        worktrees.sort();
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
                        || filepath.file_name().is_none()
                        || filepath.file_name().unwrap() != ".git"
                    {
                        continue;
                    }

                    // Take the parent
                    let filepath = filepath.parent().unwrap();

                    if let Some(s) = spinner.clone() {
                        s.set_message(format!("Searching: {}", filepath.to_str().unwrap()))
                    }

                    // Convert to a string
                    let filepath_str = filepath.to_str().unwrap();

                    repositories.insert(filepath_str.to_string());
                }
                if let Some(s) = spinner.clone() {
                    s.tick()
                }
            }
        }

        if let Some(s) = spinner.clone() {
            s.set_message("Analyzing repositories...")
        }

        let mut repositories = repositories.into_iter().collect::<Vec<_>>();
        repositories.sort();

        let mut tidy_repos = Vec::new();
        for repository in repositories.iter() {
            if let Some(s) = spinner.clone() {
                s.set_message(format!("Analyzing: {}", repository))
            }

            if let Some(tidy_repo) = Self::new(repository) {
                tidy_repos.push(tidy_repo);
            }

            if let Some(s) = spinner.clone() {
                s.tick()
            }
        }

        if let Some(s) = spinner.clone() {
            s.finish_and_clear()
        }

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
            expected_path = package_path_from_handle(origin_url);
        } else {
            // We check first among the orgs
            for org in ORG_LOADER.orgs.iter() {
                if let Some(repo_path) = org.get_repo_path(origin_url) {
                    expected_path = Some(repo_path);
                    break;
                }
            }

            // If no match, check if the link is a full git url, in which case
            // we can clone to the default worktree
            if expected_path.is_none() {
                if let Ok(repo_url) = safe_git_url_parse(origin_url) {
                    if repo_url.scheme.to_string() != "file"
                        && !repo_url.name.is_empty()
                        && repo_url.owner.is_some()
                        && repo_url.host.is_some()
                    {
                        let config = config(".");
                        let worktree = config.worktree();
                        let repo_path = format_path_with_template(
                            &worktree,
                            &repo_url,
                            &config.repo_path_format,
                        );
                        expected_path = Some(repo_path);
                    }
                }
            }
        }

        expected_path.as_ref()?;
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

        println(format!("{} Moved {}", "[✔]".light_green(), self,));

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
                                if let ConfigSource::File(path) = value.get_source() {
                                    files_to_edit.insert(path.clone());
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
                            let mut path_entry = PathEntryConfig::from_config_value(value);
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
                    "[-]".light_black(),
                    self.current_path.to_str().unwrap().light_red(),
                    self.expected_path
                        .to_str()
                        .unwrap()
                        .to_string()
                        .light_green(),
                    file_path.light_yellow(),
                    err,
                ));
            } else {
                println(format!(
                    "{} Updated {} to {} in {}",
                    "[-]".light_black(),
                    self.current_path.to_str().unwrap().light_red(),
                    self.expected_path
                        .to_str()
                        .unwrap()
                        .to_string()
                        .light_green(),
                    file_path.light_yellow(),
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

impl std::fmt::Display for TidyGitRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut s = String::new();

        if self.organized {
            // s.push_str(&format!("{} {}", "✓", self.current_path.to_str().unwrap()));
            s.push_str(self.current_path.to_str().unwrap());
            return write!(f, "{}", s.light_green());
        }

        s.push_str(&self.current_path.to_str().unwrap().light_red());
        s.push_str(" \u{2192} "); // arrow to the right in UTF-8

        let dest = self.expected_path.to_str().unwrap().to_string();

        if self.organizable {
            s.push_str(&dest.light_green());
        } else {
            s.push_str(&dest.light_yellow());
            s.push_str(&" \u{26A0}\u{FE0F}".light_yellow()); // small warning sign in UTF-8
        }

        write!(f, "{}", s.light_yellow())
    }
}
