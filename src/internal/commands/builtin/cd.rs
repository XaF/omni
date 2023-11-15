use std::process::exit;

use clap;
use once_cell::sync::OnceCell;
use shell_escape::escape;

use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::utils::omni_cmd;
use crate::internal::config::config;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::env::Shell;
use crate::internal::git::ORG_LOADER;
use crate::internal::user_interface::StringColor;
use crate::internal::ENV;
use crate::omni_error;

#[derive(Debug, Clone)]
struct CdCommandArgs {
    locate: bool,
    include_packages: bool,
    repository: Option<String>,
}

impl CdCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("locate")
                    .short('l')
                    .long("locate")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("include-packages")
                    .short('p')
                    .long("include-packages")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("no-include-packages")
                    .long("no-include-packages")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(clap::Arg::new("repo").action(clap::ArgAction::Set))
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["cd".to_string()]);
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

        let locate = *matches.get_one::<bool>("locate").unwrap_or(&false);
        let include_packages = if *matches
            .get_one::<bool>("no-include-packages")
            .unwrap_or(&false)
        {
            false
        } else if *matches
            .get_one::<bool>("include-packages")
            .unwrap_or(&false)
        {
            true
        } else {
            locate
        };

        Self {
            locate: locate,
            include_packages: include_packages,
            repository: matches.get_one::<String>("repo").map(|arg| arg.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CdCommand {
    cli_args: OnceCell<CdCommandArgs>,
}

impl CdCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &CdCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    pub fn name(&self) -> Vec<String> {
        vec!["cd".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Change directory to the git directory of the specified repository\n",
                "\n",
                "If no repository is specified, change to the git directory of the main org as ",
                "specified by \x1B[3mOMNI_ORG\x1B[0m, if specified, or errors out if not ",
                "specified.",
            )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "--locate".to_string(),
                    desc: Some(
                        concat!(
                            "If provided, will only return the path to the repository instead of switching ",
                            "directory to it. When this flag is passed, interactions are also disabled, ",
                            "as it is assumed to be used for command line purposes. ",
                            "This will exit with 0 if the repository is found, 1 otherwise.",
                        )
                        .to_string()
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--[no-]include-packages".to_string(),
                    desc: Some(
                        concat!(
                            "If provided, will include (or not include) packages when running the command; ",
                            "this defaults to including packages when using \x1B[3m--locate\x1B[0m, ",
                            "and not including packages otherwise.",
                        )
                        .to_string()
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "repo".to_string(),
                    desc: Some(
                        concat!(
                            "The name of the repo to change directory to; this can be in the format <org>/<repo>, ",
                            "or just <repo>, in which case the repo will be searched for in all the organizations, ",
                            "trying to use \x1B[3mOMNI_ORG\x1B[0m if it is set, and then trying all the other ",
                            "organizations alphabetically.",
                        )
                        .to_string()
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
        if let Err(_) = self.cli_args.set(CdCommandArgs::parse(argv)) {
            unreachable!();
        }

        if ENV.omni_cmd_file.is_none() && !self.cli_args().locate {
            omni_error!("not available without the shell integration");
            exit(1);
        }

        if let Some(repository) = &self.cli_args().repository {
            self.cd_repo(&repository);
        } else {
            self.cd_main_org();
        }
        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        true
    }

    pub fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) {
        if comp_cword > 0 {
            exit(0);
        }

        let repo = if argv.len() > 0 {
            argv[0].clone()
        } else {
            "".to_string()
        };

        // Figure out if this is a path, so we can avoid the expensive repository search
        let path_only = repo.starts_with("/")
            || repo.starts_with(".")
            || repo.starts_with("~/")
            || repo == "~"
            || repo == "-";

        // Print all the completion related to path completion
        let (list_dir, strip_path_prefix) = if let Some(slash) = repo.rfind("/") {
            (format!("{}", &repo[..slash]), false)
        } else {
            (".".to_string(), true)
        };
        if let Ok(files) = std::fs::read_dir(&list_dir) {
            for path in files {
                if let Ok(path) = path {
                    if path.path().is_dir() {
                        let path_obj = path.path();
                        let path = if strip_path_prefix {
                            path_obj.strip_prefix(&list_dir).unwrap()
                        } else {
                            path_obj.as_path()
                        };
                        let path_str = path.to_str().unwrap();

                        if !path_str.starts_with(repo.as_str()) {
                            continue;
                        }

                        println!("{}/", path.display());
                    }
                }
            }
        }

        // Get all the repositories per org
        if !path_only {
            let add_space = if Shell::current().is_fish() { " " } else { "" };
            for match_repo in ORG_LOADER.complete(&repo) {
                println!("{}{}", match_repo, add_space);
            }
        }

        exit(0);
    }

    fn cd_main_org(&self) {
        let path = if let Some(main_org) = ORG_LOADER.first() {
            main_org.worktree()
        } else {
            let config = config(".");
            config.worktree()
        };

        let path_str = format!("{}", path);

        if self.cli_args().locate {
            println!("{}", path_str);
            exit(0);
        }

        let path_escaped = escape(std::borrow::Cow::Borrowed(path_str.as_str()));
        match omni_cmd(format!("cd {}", path_escaped).as_str()) {
            Ok(_) => {}
            Err(e) => {
                omni_error!(e);
                exit(1);
            }
        }
        exit(0);
    }

    fn cd_repo(&self, repo: &str) {
        if let Some(path_str) = self.cd_repo_find(repo) {
            if self.cli_args().locate {
                println!("{}", path_str);
                exit(0);
            }

            let path_escaped = escape(std::borrow::Cow::Borrowed(path_str.as_str()));
            match omni_cmd(format!("cd {}", path_escaped).as_str()) {
                Ok(_) => {}
                Err(e) => {
                    omni_error!(e);
                    exit(1);
                }
            }
            return;
        }

        if self.cli_args().locate {
            exit(1);
        }

        omni_error!(format!("{}: No such repository", repo.to_string().yellow()));
        exit(1);
    }

    fn cd_repo_find(&self, repo: &str) -> Option<String> {
        // Delegate to the shell if this is a path
        if repo.starts_with("/")
            || repo.starts_with(".")
            || repo.starts_with("~/")
            || repo == "~"
            || repo == "-"
        {
            return Some(format!("{}", repo));
        }

        // Check if the requested repo is actually a path that exists from the current directory
        if let Ok(repo_path) = std::fs::canonicalize(repo) {
            return Some(format!("{}", repo_path.display()));
        }

        if let Some(repo_path) = ORG_LOADER.find_repo(
            repo,
            self.cli_args().include_packages,
            !self.cli_args().locate,
        ) {
            return Some(format!("{}", repo_path.display()));
        }

        None
    }
}
