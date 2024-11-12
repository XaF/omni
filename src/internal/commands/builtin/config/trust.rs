use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::exit;

use crate::internal::cache::utils::CacheObject;
use crate::internal::cache::RepositoriesCache;
use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::Command;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
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
    workdir: Option<String>,
}

impl From<BTreeMap<String, ParseArgsValue>> for ConfigTrustCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let check_status = matches!(
            args.get("check"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let workdir = match args.get("workdir") {
            Some(ParseArgsValue::SingleString(Some(workdir))) => {
                let workdir = workdir.trim();
                if workdir.is_empty() {
                    None
                } else {
                    Some(workdir.to_string())
                }
            }
            _ => None,
        };

        Self {
            check_status,
            workdir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigTrustCommand {}

impl ConfigTrustCommand {
    pub fn new() -> Self {
        Self {}
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
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["--check".to_string()],
                    desc: Some(
                        "Check the trust status of the repository instead of changing it"
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["workdir".to_string()],
                    desc: Some(
                        concat!(
                            "The work directory to trust or untrust ",
                            "[\x1B[1mdefault: current\x1B[0m]"
                        )
                        .to_string(),
                    ),
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        let command = Command::Builtin(self.clone_boxed());
        let args = ConfigTrustCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        let path = if let Some(repo) = &args.workdir {
            if let Some(repo_path) = ORG_LOADER.find_repo(repo, true, false, false) {
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

        if args.check_status {
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
