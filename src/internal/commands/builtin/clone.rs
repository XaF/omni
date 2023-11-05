use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use clap;
use git_url_parse::GitUrl;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use once_cell::sync::OnceCell;
use shell_escape::escape;
use shell_words::join as shell_join;
use tokio::process::Command as TokioCommand;

use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::builtin::UpCommand;
use crate::internal::commands::utils::omni_cmd;
use crate::internal::config;
use crate::internal::config::up::utils::run_command_with_handler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::git::format_path;
use crate::internal::git::package_path_from_git_url;
use crate::internal::git::safe_git_url_parse;
use crate::internal::git::ORG_LOADER;
use crate::internal::user_interface::StringColor;
use crate::internal::ENV;
use crate::omni_error;

#[derive(Debug, Clone)]
struct CloneCommandArgs {
    repository: String,
    package: bool,
    options: Vec<String>,
}

impl CloneCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("package")
                    .long("package")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(clap::Arg::new("repo").action(clap::ArgAction::Set))
            .arg(
                clap::Arg::new("options")
                    .action(clap::ArgAction::Append)
                    .allow_hyphen_values(true),
            )
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["clone".to_string()]);
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

        let repository;
        if let Some(repo) = matches.get_one::<String>("repo") {
            repository = repo.to_string();
        } else {
            omni_error!("no repository specified");
            exit(1);
        }

        Self {
            repository: repository,
            package: *matches.get_one::<bool>("package").unwrap_or(&false),
            options: matches
                .get_many::<String>("options")
                .map(|args| args.map(|arg| arg.to_string()).collect())
                .unwrap_or(vec![]),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CloneCommand {
    cli_args: OnceCell<CloneCommandArgs>,
}

impl CloneCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &CloneCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    pub fn name(&self) -> Vec<String> {
        vec!["clone".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Clone the specified repository\n",
                "\n",
                "The clone operation will be handled using the first organization that matches ",
                "the argument and for which the repository exists. The repository will be cloned ",
                "in a path that matches omni's expectations, depending on your configuration.",
            )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "--package".to_string(),
                    desc: Some(
                        "Clone the repository as a package \x1B[90m(default: no)\x1B[0m"
                            .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "repo".to_string(),
                    desc: Some(
                        concat!(
                            "The repository to clone; this can be in format <org>/<repo>, ",
                            "just <repo>, or the full URL. If the case where only the repo ",
                            "name is specified, \x1B[3mOMNI_ORG\x1B[0m will be used to search ",
                            "for the repository to clone."
                        )
                        .to_string(),
                    ),
                    required: true,
                },
                SyntaxOptArg {
                    name: "options...".to_string(),
                    desc: Some("Any additional options to pass to git clone.".to_string()),
                    required: false,
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["Git commands".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if let Err(_) = self.cli_args.set(CloneCommandArgs::parse(argv)) {
            unreachable!();
        }

        let repo = self.cli_args().repository.clone();
        let clone_args = self.cli_args().options.clone();
        let clone_as_package = self.cli_args().package;

        // Create a spinner
        let spinner = if ENV.interactive_shell {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg:.green}")
                    .unwrap(),
            );
            spinner.set_message(format!("Looking for {}", repo));
            spinner.enable_steady_tick(Duration::from_millis(50));
            Some(spinner)
        } else {
            None
        };

        let cloned = self
            .clone_repo_handle(
                &repo,
                &clone_args,
                clone_as_package,
                spinner.clone(),
                None,
                true,
            )
            .is_some();

        // If we still haven't got a match, we can error out
        if !cloned {
            spinner.clone().map(|s| {
                s.set_message("Not found");
                s.finish_and_clear()
            });
            omni_error!(format!("could not find repository {}", repo.yellow()));
            exit(1);
        }

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {
        // noop
    }

    pub fn lookup_repo_handle(
        &self,
        repo: &str,
        clone_as_package: bool,
        spinner: Option<ProgressBar>,
    ) -> Option<(PathBuf, GitUrl)> {
        self.try_repo_handle(repo, &vec![], clone_as_package, spinner, None, false, true)
    }

    pub fn clone_repo_handle(
        &self,
        repo: &str,
        clone_args: &Vec<String>,
        clone_as_package: bool,
        spinner: Option<ProgressBar>,
        should_run_cd: Option<bool>,
        should_run_up: bool,
    ) -> Option<(PathBuf, GitUrl)> {
        self.try_repo_handle(
            repo,
            clone_args,
            clone_as_package,
            spinner,
            should_run_cd,
            should_run_up,
            false,
        )
    }

    fn try_repo_handle(
        &self,
        repo: &str,
        clone_args: &Vec<String>,
        clone_as_package: bool,
        spinner: Option<ProgressBar>,
        should_run_cd: Option<bool>,
        should_run_up: bool,
        lookup_only: bool,
    ) -> Option<(PathBuf, GitUrl)> {
        let mut cloned = None;
        let repo = repo.to_string();

        // We check first among the orgs
        for org in ORG_LOADER.orgs.iter() {
            if let (Some(clone_url), Some(clone_path)) =
                (org.get_repo_git_url(&repo), org.get_repo_path(&repo))
            {
                let clone_path = if clone_as_package {
                    if let Some(clone_path) = package_path_from_git_url(&clone_url) {
                        clone_path
                    } else {
                        omni_error!(format!(
                            "could not format package path for {}",
                            repo.yellow()
                        ));
                        exit(1);
                    }
                } else {
                    clone_path
                };

                if self.try_clone(
                    &clone_url,
                    &clone_path,
                    &clone_args,
                    spinner.clone(),
                    should_run_cd.unwrap_or(!clone_as_package),
                    should_run_up,
                    lookup_only,
                ) {
                    cloned = Some((clone_path, clone_url));
                    break;
                }
            }
        }

        // If no match, check if the link is a full git url, in which case
        // we can clone to the default worktree
        if cloned.is_none() {
            if let Ok(clone_url) = safe_git_url_parse(&repo) {
                if clone_url.scheme.to_string() != "file"
                    && clone_url.name != ""
                    && clone_url.owner.is_some()
                    && clone_url.host.is_some()
                {
                    let config = config(".");
                    let worktree = config.worktree();
                    let clone_path = format_path(&worktree, &clone_url);
                    let clone_path = if clone_as_package {
                        if let Some(clone_path) = package_path_from_git_url(&clone_url) {
                            clone_path
                        } else {
                            omni_error!(format!(
                                "could not format package path for {}",
                                repo.yellow()
                            ));
                            exit(1);
                        }
                    } else {
                        clone_path
                    };

                    if self.try_clone(
                        &clone_url,
                        &clone_path,
                        &clone_args,
                        spinner.clone(),
                        should_run_cd.unwrap_or(!clone_as_package),
                        should_run_up,
                        lookup_only,
                    ) {
                        cloned = Some((clone_path, clone_url));
                    }
                }
            }
        }

        cloned
    }

    fn suggest_run_up(&self) -> bool {
        let question = requestty::Question::confirm("suggest_run_up")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message(format!(
                "{} {}",
                "omni:".to_string().light_cyan(),
                format!("Do you want to run {} ?", "omni up".to_string().underline()),
            ))
            .default(true)
            .build();

        match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::Bool(confirmed) => {
                    return confirmed;
                }
                _ => {}
            },
            Err(err) => {
                // print!("\x1B[1A\x1B[2K"); // This clears the line, so there's no artifact left
                println!("{}", format!("[✘] {:?}", err).red());
            }
        }

        false
    }

    fn try_clone(
        &self,
        clone_url: &GitUrl,
        clone_path: &PathBuf,
        clone_args: &Vec<String>,
        spinner: Option<ProgressBar>,
        auto_cd: bool,
        should_run_up: bool,
        lookup_only: bool,
    ) -> bool {
        let log_command = |message: String| {
            if lookup_only {
                return;
            } else if let Some(spinner) = &spinner {
                spinner.println(message);
            } else {
                eprintln!("{}", message);
            }
        };

        let log_progress = |message: String| {
            if lookup_only {
                return;
            } else if let Some(spinner) = &spinner {
                spinner.set_message(message);
            } else {
                eprintln!("{}", message);
            }
        };

        let mut run_up = should_run_up;

        if clone_path.exists() {
            log_progress(format!("Found {}", clone_path.to_string_lossy()));
            spinner.map(|s| s.finish_and_clear());

            if lookup_only {
                return true;
            }

            omni_error!(format!(
                "repository already exists {}",
                format!("({})", clone_path.to_string_lossy()).light_black()
            ));

            if should_run_up {
                run_up = self.suggest_run_up();
            }
        } else {
            log_progress(format!("Checking {}", clone_url.to_string()));

            // Check using git ls-remote if the repository exists
            let mut cmd = TokioCommand::new("git");
            cmd.arg("ls-remote");
            cmd.arg(&clone_url.to_string());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let result = run_command_with_handler(
                &mut cmd,
                |_stdout, _stderr| {
                    // Do nothing
                },
                RunConfig::with_timeout(config(".").clone.ls_remote_timeout_seconds),
            );

            if result.is_err() {
                log_progress(format!(
                    "Repository {} does not exist",
                    clone_url.to_string()
                ));
                return false;
            }

            if lookup_only {
                return true;
            }

            log_progress(format!("Cloning {}", clone_url.to_string()));
            spinner.clone().map(|s| s.finish_and_clear());

            let mut cmd_args = vec!["git".to_string(), "clone".to_string()];
            cmd_args.push(clone_url.to_string());
            cmd_args.push(clone_path.to_string_lossy().to_string());
            cmd_args.extend(clone_args.clone());

            let mut cmd = std::process::Command::new(&cmd_args[0]);
            cmd.args(&cmd_args[1..]);
            cmd.stdout(std::process::Stdio::inherit());
            cmd.stderr(std::process::Stdio::inherit());

            log_command(format!("$ {}", shell_join(cmd_args)).light_black());

            let result = cmd.output();
            if result.is_err() {
                let msg = format!(
                    "failed to clone repository {}",
                    format!("({})", clone_url.to_string()).light_black()
                );

                omni_error!(msg);
                exit(1);
            }
        }

        // If we reach here, the repo either exists or just got cloned, so we can
        // directly cd into it
        if auto_cd && ENV.omni_cmd_file.is_some() {
            let path_str = clone_path.to_string_lossy();
            let path_escaped = escape(path_str);
            match omni_cmd(format!("cd {}", path_escaped).as_str()) {
                Ok(_) => {}
                Err(e) => {
                    omni_error!(e);
                    exit(1);
                }
            }
        }

        if run_up {
            if let Err(err) = std::env::set_current_dir(&clone_path) {
                omni_error!(format!(
                    "failed to change directory {}: {}",
                    format!("({})", clone_path.to_string_lossy()).light_black(),
                    format!("{}", err).red()
                ));
                exit(1);
            }

            eprintln!("{}", format!("$ omni up --bootstrap").light_black());

            let up_cmd = UpCommand::new_command();
            up_cmd.exec(
                vec!["--bootstrap".to_string()],
                Some(vec!["up".to_string()]),
            );

            panic!("omni up failed");
        }

        true
    }
}
