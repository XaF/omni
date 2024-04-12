use std::path::PathBuf;
use std::process::exit;

use once_cell::sync::OnceCell;

use crate::internal::cache::utils::CacheObject;
use crate::internal::cache::RepositoriesCache;
use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::HelpCommand;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::internal::workdir::add_trust;
use crate::internal::workdir::is_trusted;
use crate::internal::workdir::remove_trust;
use crate::internal::ORG_LOADER;
use crate::omni_error;
use crate::omni_info;

#[derive(Debug, Clone)]
struct ConfigTrustCommandArgs {
    check_status: bool,
    repository: Option<String>,
}

impl ConfigTrustCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("check")
                    .long("check")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(clap::Arg::new("repo").action(clap::ArgAction::Set))
            .try_get_matches_from(&parse_argv);

        let matches = match matches {
            Ok(matches) => matches,
            Err(err) => {
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
        };

        Self {
            check_status: *matches.get_one::<bool>("check").unwrap_or(&false),
            repository: matches.get_one::<String>("repo").map(|arg| arg.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigTrustCommand {
    cli_args: OnceCell<ConfigTrustCommandArgs>,
}

impl ConfigTrustCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &ConfigTrustCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    fn subcommand(&self) -> String {
        std::env::var("OMNI_SUBCOMMAND").unwrap_or("config trust".to_string())
    }

    fn is_trust(&self) -> bool {
        self.subcommand() == "config trust"
    }
}

impl BuiltinCommand for ConfigTrustCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["config".to_string(), "trust".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![vec!["config".to_string(), "untrust".to_string()]]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Trust or untrust a work directory.\n",
                "\n",
                "If the work directory is trusted, \x1B[1mup\x1B[0m and work directory-provided ",
                "commands will be available to run without asking confirmation.\n",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "--check".to_string(),
                    desc: Some(
                        concat!(
                            "Check the trust status of the repository instead of changing it ",
                            "\x1B[90m(default: false)\x1B[0m",
                        )
                        .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "repo".to_string(),
                    desc: Some(
                        concat!(
                            "The repository to trust or untrust ",
                            "\x1B[90m(default: current)\x1B[0m",
                        )
                        .to_string(),
                    ),
                    required: false,
                },
            ],
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        if self
            .cli_args
            .set(ConfigTrustCommandArgs::parse(argv))
            .is_err()
        {
            unreachable!();
        }

        let path = if let Some(repo) = &self.cli_args().repository {
            if let Some(repo_path) = ORG_LOADER.find_repo(repo, true, false) {
                repo_path
            } else {
                omni_error!(format!("repository not found: {}", repo));
                exit(1);
            }
        } else {
            PathBuf::from(".")
        };

        let path_str = path.display().to_string();

        let wd = workdir(path_str.as_str());
        let wd_id = match wd.id() {
            Some(id) => id,
            None => {
                omni_error!(format!(
                    "path {} is not a work directory",
                    path_str.light_yellow()
                ));
                exit(2);
            }
        };

        let is_trusted = is_trusted(path_str.as_str());

        if self.cli_args().check_status {
            if is_trusted {
                omni_info!(
                    format!("work directory is {}", "trusted".light_green()),
                    wd_id
                );
                exit(0);
            } else {
                omni_info!(
                    format!("work directory is {}", "not trusted".light_red(),),
                    wd_id
                );
                exit(2);
            }
        } else if self.is_trust() {
            if is_trusted {
                omni_info!(
                    format!("work directory is already {}", "trusted".light_green()),
                    wd_id
                );
                exit(0);
            }

            if add_trust(path_str.as_str()) {
                omni_info!(
                    format!("work directory is now {}", "trusted".light_green()),
                    wd_id
                );
                exit(0);
            } else {
                exit(1);
            }
        } else {
            if !is_trusted {
                omni_info!(
                    format!("work directory is already {}", "untrusted".light_red()),
                    wd_id
                );
                exit(0);
            }

            let wd_trust_id = wd.trust_id().expect("trust id not found");
            if !RepositoriesCache::get().has_trusted(wd_trust_id.as_str()) {
                omni_error!(format!(
                    "work directory {} is in a trusted organization",
                    wd_id.light_blue()
                ));
                exit(1);
            }

            if remove_trust(path_str.as_str()) {
                omni_info!(
                    format!("work directory is now {}", "untrusted".light_red()),
                    wd_id
                );
                exit(0);
            } else {
                exit(1);
            }
        }
    }

    fn autocompletion(&self) -> bool {
        false
    }

    fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        // TODO: autocomplete repositories if first argument
        Err(())
    }
}
