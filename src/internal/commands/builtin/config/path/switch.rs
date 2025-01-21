use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use indicatif::ProgressBar;
use indicatif::ProgressStyle;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::base::CommandAutocompletion;
use crate::internal::commands::builtin::CloneCommand;
use crate::internal::commands::builtin::TidyGitRepo;
use crate::internal::commands::builtin::UpCommand;
use crate::internal::commands::path::global_omnipath_entries;
use crate::internal::commands::Command;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxGroup;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::full_git_url_parse;
use crate::internal::git::id_from_git_url;
use crate::internal::git::package_path_from_handle;
use crate::internal::git::path_entry_config;
use crate::internal::git::ORG_LOADER;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir_flush_cache;
use crate::omni_error;
use crate::omni_info;

#[derive(Debug, Clone)]
enum ConfigPathSwitchCommandArgsSource {
    Package,
    Worktree,
    Toggle,
}

#[derive(Debug, Clone)]
struct ConfigPathSwitchCommandArgs {
    repository: Option<String>,
    source: ConfigPathSwitchCommandArgsSource,
}

impl From<BTreeMap<String, ParseArgsValue>> for ConfigPathSwitchCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let repository = match args.get("repository") {
            Some(ParseArgsValue::SingleString(Some(repo))) => {
                let repo = repo.trim();
                Some(repo.to_string())
            }
            _ => None,
        };

        let package = matches!(
            args.get("package"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let worktree = matches!(
            args.get("worktree"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let source = if package {
            ConfigPathSwitchCommandArgsSource::Package
        } else if worktree {
            ConfigPathSwitchCommandArgsSource::Worktree
        } else {
            ConfigPathSwitchCommandArgsSource::Toggle
        };

        Self { repository, source }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigPathSwitchCommand {}

impl ConfigPathSwitchCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for ConfigPathSwitchCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec![
            "config".to_string(),
            "path".to_string(),
            "switch".to_string(),
        ]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
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

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["-p".to_string(), "--package".to_string()],
                    desc: Some(
                        concat!(
                            "Switch the source to use the package in the omnipath; this will ",
                            "clone the repository if it does not exist. This defaults to toggling \n",
                            "between the two sources if not specified.\n",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-w".to_string(), "--worktree".to_string()],
                    desc: Some(
                        concat!(
                            "Switch the source to use the worktree in the omnipath; this will ",
                            "clone the repository if it does not exist. This defaults to toggling \n",
                            "between the two sources if not specified.\n",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["repository".to_string()],
                    desc: Some(
                        concat!(
                            "The name of the repository to switch the source from; this can be in the format ",
                            "<org>/<repo>, or just <repo>. If the repository is not provided, the current ",
                            "repository will be used, or the command will fail if not in a repository. If ",
                            "the repo is not found in the omnipath, the command will fail.\n",
                        )
                        .to_string()
                    ),
                    ..Default::default()
                },
            ],
            groups: vec![
                SyntaxGroup {
                    name: "source".to_string(),
                    parameters: vec!["package".to_string(), "worktree".to_string()],
                    ..Default::default()
                }
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        let command = Command::Builtin(self.clone_boxed());
        let args = ConfigPathSwitchCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        let mut repo_id = None;
        let mut repo_handle = None;
        let mut worktree_path = None;
        let mut package_path = None;

        if let Some(repo) = args.repository.clone() {
            let repo_path = ORG_LOADER.find_repo(&repo, true, false, false);
            if let Some(repo_path) = repo_path {
                let git = git_env(repo_path.to_string_lossy());
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
                    if shell_is_interactive() {
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
                    if shell_is_interactive() {
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
            let git_url = match full_git_url_parse(&repo_handle) {
                Ok(git_url) => git_url,
                Err(err) => {
                    omni_error!(format!(
                        "failed to parse git url {}: {}",
                        repo_handle.light_blue(),
                        err
                    ));
                    exit(1);
                }
            };

            match id_from_git_url(&git_url) {
                Some(repo_id) => repo_id,
                None => {
                    omni_error!(format!(
                        "failed to resolve repository id from git url {}",
                        repo_handle.light_blue()
                    ));
                    exit(1);
                }
            }
        });

        let worktree_exists = worktree_path.exists();
        let package_exists = package_path.exists();
        if !worktree_exists && !package_exists {
            omni_error!(format!("repository {} is not cloned", repo_handle.yellow()));
            exit(1);
        }

        let worktree_entry = path_entry_config(worktree_path.to_string_lossy());
        let package_entry = path_entry_config(package_path.to_string_lossy());

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

        let to_source = match &args.source {
            ConfigPathSwitchCommandArgsSource::Toggle => {
                if worktree_in_omnipath {
                    ConfigPathSwitchCommandArgsSource::Package
                } else {
                    ConfigPathSwitchCommandArgsSource::Worktree
                }
            }
            _ => args.source.clone(),
        };

        let (switch_from_path, switch_to_path, targets_package) = match to_source {
            ConfigPathSwitchCommandArgsSource::Package => {
                if !worktree_in_omnipath {
                    omni_info!(format!(
                        "{} is already using the {} source",
                        repo_id.light_blue(),
                        "📦 package".light_green(),
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
                        "🌳 worktree".light_green(),
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
            let spinner = if shell_is_interactive() {
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
                &[],
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
                "omni up".light_yellow().underline(),
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

    fn autocompletion(&self) -> CommandAutocompletion {
        CommandAutocompletion::Null
    }

    fn autocomplete(
        &self,
        _comp_cword: usize,
        _argv: Vec<String>,
        _parameter: Option<String>,
    ) -> Result<(), ()> {
        Ok(())
    }
}
