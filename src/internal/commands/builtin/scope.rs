use std::process::exit;

use once_cell::sync::OnceCell;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::command_loader;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::env::current_dir;
use crate::internal::env::Shell;
use crate::internal::git::ORG_LOADER;
use crate::internal::user_interface::StringColor;
use crate::omni_error;

#[derive(Debug, Clone)]
struct ScopeCommandArgs {
    include_packages: bool,
    scope: String,
    command: Vec<String>,
}

impl ScopeCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
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
            .arg(clap::Arg::new("scope").action(clap::ArgAction::Set))
            .arg(
                clap::Arg::new("command")
                    .action(clap::ArgAction::Append)
                    .allow_hyphen_values(true),
            )
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["scope".to_string()]);
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

        let include_packages = if *matches
            .get_one::<bool>("no-include-packages")
            .unwrap_or(&false)
        {
            false
        } else {
            // We don't need to check if `include-packages` is passed
            // because it's the default
            true
        };

        let scope = if let Some(scope) = matches.get_one::<String>("scope") {
            scope.to_string()
        } else {
            omni_error!("no scope specified");
            exit(1);
        };

        let command: Vec<_> = matches
            .get_many::<String>("command")
            .map(|args| args.map(|arg| arg.to_string()).collect())
            .unwrap_or_default();
        if command.is_empty() {
            omni_error!("no command specified");
            exit(1);
        };

        Self {
            include_packages,
            scope,
            command,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScopeCommand {
    cli_args: OnceCell<ScopeCommandArgs>,
}

impl ScopeCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &ScopeCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    fn autocomplete_repo(&self, repo: String) -> Result<(), ()> {
        // Figure out if this is a path, so we can avoid the expensive repository search
        let path_only = repo.starts_with('/')
            || repo.starts_with('.')
            || repo.starts_with("~/")
            || repo == "~"
            || repo == "-";

        // Print all the completion related to path completion
        let (list_dir, strip_path_prefix) = if let Some(slash) = repo.rfind('/') {
            (repo[..slash].to_string(), false)
        } else {
            (".".to_string(), true)
        };
        if let Ok(files) = std::fs::read_dir(&list_dir) {
            for path in files.flatten() {
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

        // Get all the repositories per org
        if !path_only {
            let add_space = if Shell::current().is_fish() { " " } else { "" };
            for match_repo in ORG_LOADER.complete(&repo) {
                println!("{}{}", match_repo, add_space);
            }
        }

        Ok(())
    }

    fn switch_scope(
        &self,
        repo: &str,
        include_packages: bool,
        silent_failure: bool,
    ) -> Result<(), ()> {
        if let Ok(repo_path) = std::fs::canonicalize(repo) {
            if let Err(err) = std::env::set_current_dir(&repo_path) {
                if !silent_failure {
                    omni_error!(format!(
                        "failed to change directory {}: {}",
                        format!("({})", repo_path.display()).light_black(),
                        format!("{}", err).red()
                    ));
                }
                return Err(());
            }
            return Ok(());
        }

        if let Some(repo_path) = ORG_LOADER.find_repo(repo, include_packages, true) {
            if let Err(err) = std::env::set_current_dir(&repo_path) {
                if !silent_failure {
                    omni_error!(format!(
                        "failed to change directory {}: {}",
                        format!("({})", repo_path.display()).light_black(),
                        format!("{}", err).red()
                    ));
                }
                return Err(());
            }
            return Ok(());
        }

        if !silent_failure {
            omni_error!(format!("{}: No such repository", repo.yellow()));
        }

        Err(())
    }
}

impl BuiltinCommand for ScopeCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["scope".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Runs an omni command in the context of the specified repository\n",
                "\n",
                "This allows to run any omni command that would be available while in the ",
                "repository directory, but without having to change directory to the ",
                "repository first.",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    name: "repo".to_string(),
                    desc: Some(
                        concat!(
                            "The name of the repo to run commands in the context of; this ",
                            "can be in the format <org>/<repo>, or just <repo>, in which case ",
                            "the repo will be searched for in all the organizations, trying ",
                            "to use \x1B[3mOMNI_ORG\x1B[0m if it is set, and then trying all ",
                            "the other organizations alphabetically."
                        )
                        .to_string(),
                    ),
                    required: true,
                    ..Default::default()
                },
                SyntaxOptArg {
                    name: "command".to_string(),
                    desc: Some(
                        "The omni command to run in the context of the specified repository."
                            .to_string(),
                    ),
                    required: true,
                    ..Default::default()
                },
                SyntaxOptArg {
                    name: "options...".to_string(),
                    desc: Some("Any options to pass to the omni command.".to_string()),
                    required: false,
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
        if self.cli_args.set(ScopeCommandArgs::parse(argv)).is_err() {
            unreachable!();
        }

        if self
            .switch_scope(
                &self.cli_args().scope,
                self.cli_args().include_packages,
                false,
            )
            .is_err()
        {
            exit(1);
        }

        let argv = self.cli_args().command.clone();
        let command_loader = command_loader(".");
        if let Some((omni_cmd, called_as, argv)) = command_loader.to_serve(&argv) {
            omni_cmd.exec(argv, Some(called_as));
            panic!("exec returned");
        }

        eprintln!(
            "{} {} {}",
            "omni:".light_cyan(),
            "command not found:".red(),
            argv.join(" ")
        );

        if let Some((omni_cmd, called_as, argv)) = command_loader.find_command(&argv) {
            omni_cmd.exec(argv, Some(called_as));
            panic!("exec returned");
        }

        exit(1);
    }

    fn autocompletion(&self) -> bool {
        true
    }

    fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) -> Result<(), ()> {
        match comp_cword.cmp(&0) {
            std::cmp::Ordering::Equal => {
                let repo = if !argv.is_empty() {
                    argv[0].clone()
                } else {
                    "".to_string()
                };
                self.autocomplete_repo(repo)
            }
            std::cmp::Ordering::Greater => {
                if argv.is_empty() {
                    // Unsure why we would get here, but if we try to complete
                    // a command but a repository is not provided, we can't, so
                    // let's simply skip it
                    return Ok(());
                }

                // We want to switch context to the repository, so we can offer
                // completion of the commands for that specific repository
                let mut argv = argv.clone();
                let repo = argv.remove(0);

                let curdir = current_dir();
                // TODO: use the previous arguments to know if we should include packages or not
                if self.switch_scope(&repo, true, true).is_err() {
                    return Err(());
                }

                // Finally, we can try completing the command
                let command_loader = command_loader(".");
                let result = command_loader.complete(comp_cword - 1, argv.to_vec(), true);

                // Restore current scope
                if std::env::set_current_dir(curdir).is_err() {
                    return Err(());
                }

                result
            }
            std::cmp::Ordering::Less => Err(()),
        }
    }
}
