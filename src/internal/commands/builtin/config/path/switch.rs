use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use clap;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use once_cell::sync::OnceCell;

use crate::internal::commands::builtin::CloneCommand;
use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::builtin::TidyGitRepo;
use crate::internal::commands::builtin::UpCommand;
use crate::internal::commands::path::global_omnipath_entries;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::git::full_git_url_parse;
use crate::internal::git::id_from_git_url;
use crate::internal::git::package_path_from_handle;
use crate::internal::git::path_entry_config;
use crate::internal::git::ORG_LOADER;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir_flush_cache;
use crate::internal::ENV;
use crate::omni_error;
use crate::omni_info;

#[derive(Debug, Clone)]
struct ConfigPathSwitchCommandArgs {
    repository: Option<String>,
    source: ConfigPathSwitchCommandArgsSource,
}

#[derive(Debug, Clone)]
enum ConfigPathSwitchCommandArgsSource {
    Package,
    Worktree,
    Toggle,
}

impl ConfigPathSwitchCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("package")
                    .long("package")
                    .short('p')
                    .conflicts_with("worktree")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("worktree")
                    .long("worktree")
                    .short('w')
                    .conflicts_with("package")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(clap::Arg::new("repo").action(clap::ArgAction::Set))
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["status".to_string()]);
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

        if *matches.get_one::<bool>("help").unwrap_or(&false) {
            HelpCommand::new().exec(vec![
                "config".to_string(),
                "path".to_string(),
                "switch".to_string(),
            ]);
            exit(1);
        }

        let source = if *matches.get_one::<bool>("package").unwrap_or(&false) {
            ConfigPathSwitchCommandArgsSource::Package
        } else if *matches.get_one::<bool>("worktree").unwrap_or(&false) {
            ConfigPathSwitchCommandArgsSource::Worktree
        } else {
            ConfigPathSwitchCommandArgsSource::Toggle
        };

        Self {
            repository: matches.get_one::<String>("repo").map(|arg| arg.to_string()),
            source: source,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigPathSwitchCommand {
    cli_args: OnceCell<ConfigPathSwitchCommandArgs>,
}

impl ConfigPathSwitchCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    #[allow(dead_code)]
    fn cli_args(&self) -> &ConfigPathSwitchCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    pub fn name(&self) -> Vec<String> {
        vec![
            "config".to_string(),
            "path".to_string(),
            "switch".to_string(),
        ]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Switch the source of a repository in the omnipath\n",
                "\n",
                "This allows to change the omnipath source from using a package or ",
                "a development version in a worktree.\n",
                "\n",
                "When switching into a mode, if the source of the requested type does ",
                "not exist, the repository will be cloned.\n",
            )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "--<source>".to_string(),
                    desc: Some(
                        concat!(
                            "The source to use for the repository; this can be either ",
                            "\x1B[1m--package\x1B[0m or \x1B[1m--worktree\x1B[0m, or will ",
                            "toggle between the two if not specified.\n",
                        )
                        .to_string()
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "repo".to_string(),
                    desc: Some(
                        concat!(
                            "The name of the repository to switch the source from; this can be in the format ",
                            "<org>/<repo>, or just <repo>. If the repository is not provided, the current ",
                            "repository will be used, or the command will fail if not in a repository. If ",
                            "the repo is not found in the omnipath, the command will fail.\n",
                        )
                        .to_string()
                    ),
                    required: false,
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if let Err(_) = self.cli_args.set(ConfigPathSwitchCommandArgs::parse(argv)) {
            unreachable!();
        }

        let mut repo_id = None;
        let mut repo_handle = None;
        let mut worktree_path = None;
        let mut package_path = None;

        if let Some(repo) = self.cli_args().repository.clone() {
            let repo_path = ORG_LOADER.find_repo(&repo, true, false);
            if let Some(repo_path) = repo_path {
                let git = git_env(&repo_path.to_string_lossy());
                if git.in_repo() && git.has_origin() {
                    repo_handle = Some(git.origin().unwrap().to_string());
                    package_path = package_path_from_handle(&repo_handle.clone().unwrap());
                    if let Some(package_path) = package_path.clone() {
                        if package_path != repo_path {
                            worktree_path = Some(repo_path);
                        }
                    }
                }
            }

            if repo_handle.is_none() || worktree_path.is_none() {
                let clone_command = CloneCommand::new();
                let lookup_repo = repo_handle.unwrap_or(repo.clone());
                let remote_repo = clone_command.lookup_repo_handle(
                    &lookup_repo,
                    false,
                    if ENV.interactive_shell {
                        let spinner = ProgressBar::new_spinner();
                        spinner.set_style(
                            ProgressStyle::default_spinner()
                                .template("{spinner:.green} {msg:.green}")
                                .unwrap(),
                        );
                        spinner.set_message(format!("Looking for {}", lookup_repo));
                        spinner.enable_steady_tick(Duration::from_millis(50));
                        Some(spinner)
                    } else {
                        None
                    },
                );
                if remote_repo.is_none() {
                    omni_error!(format!(
                        "could not find repository {}",
                        lookup_repo.yellow()
                    ));
                    exit(1);
                }
                let (repo_path, repo_git_url) = remote_repo.unwrap();

                repo_id = id_from_git_url(&repo_git_url);
                repo_handle = Some(repo_git_url.to_string());
                worktree_path = Some(repo_path);
                package_path = package_path_from_handle(&repo_handle.clone().unwrap());
            }
        } else {
            let git = git_env(".");
            if !git.in_repo() {
                omni_error!("not in a repository");
                exit(1);
            }
            if !git.has_origin() {
                omni_error!("repository does not have an origin");
                exit(1);
            }

            repo_handle = Some(git.origin().unwrap().to_string());
            package_path = package_path_from_handle(&repo_handle.clone().unwrap());
            let git_root = git.root().unwrap();
            if let Some(package_path) = package_path.clone() {
                if package_path != PathBuf::from(&git_root) {
                    worktree_path = Some(git_root.into());
                }
            }

            if worktree_path.is_none() {
                let lookup_repo = repo_handle.clone().unwrap();
                let clone_command = CloneCommand::new();
                // Create a spinner to show that we're looking for the repository
                let remote_repo = clone_command.lookup_repo_handle(
                    &lookup_repo,
                    false,
                    if ENV.interactive_shell {
                        let spinner = ProgressBar::new_spinner();
                        spinner.set_style(
                            ProgressStyle::default_spinner()
                                .template("{spinner:.green} {msg:.green}")
                                .unwrap(),
                        );
                        spinner.set_message(format!("Looking for {}", lookup_repo));
                        spinner.enable_steady_tick(Duration::from_millis(50));
                        Some(spinner)
                    } else {
                        None
                    },
                );
                if remote_repo.is_none() {
                    omni_error!(format!(
                        "could not find repository {}",
                        lookup_repo.yellow()
                    ));
                    exit(1);
                }
                let (repo_path, repo_git_url) = remote_repo.unwrap();

                repo_id = id_from_git_url(&repo_git_url);
                repo_handle = Some(repo_git_url.to_string());
                worktree_path = Some(repo_path);
            }
        }

        if repo_handle.is_none() || worktree_path.is_none() || package_path.is_none() {
            omni_error!("could not find repository");
            exit(1);
        }
        let repo_handle = repo_handle.unwrap();
        let worktree_path = worktree_path.unwrap();
        let package_path = package_path.unwrap();

        let repo_id = repo_id.unwrap_or_else(|| {
            let git_url = full_git_url_parse(&repo_handle);
            if git_url.is_err() {
                omni_error!(format!(
                    "failed to parse git url {}",
                    repo_handle.light_blue()
                ));
                exit(1);
            }
            let repo_id = id_from_git_url(&git_url.unwrap());
            if repo_id.is_none() {
                omni_error!(format!(
                    "failed to resolve repository id from git url {}",
                    repo_handle.light_blue()
                ));
                exit(1);
            }
            repo_id.unwrap()
        });

        let worktree_exists = worktree_path.exists();
        let package_exists = package_path.exists();
        if !worktree_exists && !package_exists {
            omni_error!(format!("repository {} is not cloned", repo_handle.yellow()));
            exit(1);
        }

        let worktree_entry = path_entry_config(&worktree_path.to_string_lossy());
        let package_entry = path_entry_config(&package_path.to_string_lossy());

        let worktree_in_omnipath = global_omnipath_entries()
            .iter()
            .any(|entry| entry.starts_with(&worktree_entry));

        let package_in_omnipath = global_omnipath_entries()
            .iter()
            .any(|entry| entry.starts_with(&package_entry));

        if !worktree_in_omnipath && !package_in_omnipath {
            omni_error!(format!("{} is not in the omnipath", repo_handle.yellow(),));
            exit(1);
        }

        let to_source = match &self.cli_args().source {
            ConfigPathSwitchCommandArgsSource::Toggle => {
                if worktree_in_omnipath {
                    ConfigPathSwitchCommandArgsSource::Package
                } else {
                    ConfigPathSwitchCommandArgsSource::Worktree
                }
            }
            _ => self.cli_args().source.clone(),
        };

        let (switch_from_path, switch_to_path, targets_package) = match to_source {
            ConfigPathSwitchCommandArgsSource::Package => {
                if !worktree_in_omnipath {
                    omni_info!(format!(
                        "{} is already using the {} source",
                        repo_id.light_blue(),
                        "📦 package".to_string().light_green(),
                    ));
                    exit(0);
                }

                (worktree_path, package_path, true)
            }
            ConfigPathSwitchCommandArgsSource::Worktree => {
                if !package_in_omnipath {
                    omni_info!(format!(
                        "{} is already using the {} source",
                        repo_id.light_blue(),
                        "🌳 worktree".to_string().light_green(),
                    ));
                    exit(0);
                }

                (package_path, worktree_path, false)
            }
            ConfigPathSwitchCommandArgsSource::Toggle => unreachable!(),
        };

        // If the target path does not exist, clone it, but hold running `omni up` for now
        let requires_cloning = !switch_to_path.exists();
        if requires_cloning {
            // Create a spinner to show that we're cloning the repository
            let spinner = if ENV.interactive_shell {
                let spinner = ProgressBar::new_spinner();
                spinner.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg:.green}")
                        .unwrap(),
                );
                spinner.set_message(format!("Looking for {}", repo_handle));
                spinner.enable_steady_tick(Duration::from_millis(50));
                Some(spinner)
            } else {
                None
            };

            let clone_command = CloneCommand::new();
            let cloned = clone_command.clone_repo_handle(
                &repo_handle,
                &vec![],
                targets_package,
                spinner.clone(),
                None,
                false,
            );

            if cloned.is_none() {
                omni_error!(format!(
                    "failed to clone repository {}",
                    repo_handle.light_blue()
                ));
                exit(1);
            }

            workdir_flush_cache(switch_to_path.to_string_lossy());
        }

        // Update the configuration files with the new path
        TidyGitRepo::new_with_paths(switch_from_path.clone(), switch_to_path.clone())
            .edit_config(|_s: String| {});

        omni_info!(format!(
            "Switched to {} {}",
            if targets_package {
                "📦 package"
            } else {
                "🌳 worktree"
            },
            repo_id.light_blue(),
        ));

        // If the target path is a package, run `omni up` to update the package
        if targets_package {
            if let Err(err) = std::env::set_current_dir(&switch_to_path) {
                omni_error!(format!(
                    "failed to change directory {}: {}",
                    format!("({})", switch_to_path.to_string_lossy()).light_black(),
                    format!("{}", err).red()
                ));
                exit(1);
            }

            omni_info!(format!(
                "Running {} to make sure the package is up to date",
                "omni up".to_string().light_yellow().underline(),
            ));

            let up_cmd = UpCommand::new_command();
            up_cmd.exec(
                if requires_cloning {
                    vec![]
                } else {
                    vec!["--update-repository".to_string()]
                },
                Some(vec!["up".to_string()]),
            );
        }

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {
        ()
    }
}
