use std::collections::BTreeMap;
use std::process::exit;

use shell_escape::escape;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::base::CommandAutocompletion;
use crate::internal::commands::utils::omni_cmd;
use crate::internal::commands::utils::path_auto_complete;
use crate::internal::commands::Command;
use crate::internal::config::config;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::omni_cmd_file;
use crate::internal::git::ORG_LOADER;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;

#[derive(Debug, Clone)]
struct CdCommandArgs {
    locate: bool,
    include_packages: bool,
    workdir: Option<String>,
}

impl From<BTreeMap<String, ParseArgsValue>> for CdCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let locate = matches!(
            args.get("locate"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );

        let yes_include_packages = matches!(
            args.get("include_packages"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let no_include_packages = matches!(
            args.get("no_include_packages"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let include_packages = if no_include_packages {
            false
        } else if yes_include_packages {
            true
        } else {
            locate
        };

        let workdir = match args.get("workdir") {
            Some(ParseArgsValue::SingleString(Some(workdir))) => Some(workdir.clone()),
            _ => None,
        };

        Self {
            locate,
            include_packages,
            workdir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CdCommand {}

impl CdCommand {
    pub fn new() -> Self {
        Self {}
    }

    fn cd_main_org(&self, args: &CdCommandArgs) {
        let path = if let Some(main_org) = ORG_LOADER.first() {
            main_org.worktree()
        } else {
            let config = config(".");
            config.worktree()
        };

        let path_str = path.to_string();

        if args.locate {
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

    fn cd_workdir(&self, wd: &str, args: &CdCommandArgs) {
        if let Some(path_str) = self.cd_workdir_find(wd, args) {
            if args.locate {
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

        if args.locate {
            exit(1);
        }

        omni_error!(format!("{}: No such work directory", wd.yellow()));
        exit(1);
    }

    fn cd_workdir_find(&self, wd: &str, args: &CdCommandArgs) -> Option<String> {
        // Handle the special case of `...` to go to the work directory root
        if wd == "..." {
            let wd = workdir(".");
            return wd.root().map(|wd_root| wd_root.to_string());
        }

        // Delegate to the shell if this is a path
        if wd.starts_with('/')
            || wd.starts_with('.')
            || wd.starts_with("~/")
            || wd == "~"
            || wd == "-"
        {
            return Some(wd.to_string());
        }

        // Check if the requested wd is actually a path that exists from the current directory
        if let Ok(wd_path) = std::fs::canonicalize(wd) {
            return Some(format!("{}", wd_path.display()));
        }

        let only_worktree = !args.include_packages;
        let allow_interactive = !args.locate;
        if let Some(wd_path) = ORG_LOADER.find_repo(wd, only_worktree, false, allow_interactive) {
            return Some(format!("{}", wd_path.display()));
        }

        None
    }
}

impl BuiltinCommand for CdCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["cd".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Change directory to the root of the specified work directory\n",
                "\n",
                "If no work directory is specified, change to the git directory of the main org as ",
                "specified by \x1B[3mOMNI_ORG\x1B[0m, if specified, or errors out if not ",
                "specified.",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["-l".to_string(), "--locate".to_string()],
                    desc: Some(
                        concat!(
                            "If provided, will only return the path to the work directory instead of switching ",
                            "directory to it. When this flag is passed, interactions are also disabled, ",
                            "as it is assumed to be used for command line purposes. ",
                            "This will exit with 0 if the work directory is found, 1 otherwise.",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-p".to_string(), "--include-packages".to_string()],
                    desc: Some(
                        concat!(
                            "If provided, will include packages when running the command; ",
                            "this defaults to including packages when using \x1B[3m--locate\x1B[0m, ",
                            "and not including packages otherwise.",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    conflicts_with: vec!["--no-include-packages".to_string()],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--no-include-packages".to_string()],
                    desc: Some(
                        concat!(
                            "If provided, will NOT include packages when running the command; ",
                            "this defaults to including packages when using \x1B[3m--locate\x1B[0m, ",
                            "and not including packages otherwise.",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["workdir".to_string()],
                    desc: Some(
                        concat!(
                            "The name of the work directory to change directory to; this can be in the format ",
                            "<org>/<repo>, or just <repo>, in which case the work directory will be searched for ",
                            "in all the organizations, trying to use \x1B[3mOMNI_ORG\x1B[0m if it is set, and then ",
                            "trying all the other organizations alphabetically.",
                        )
                        .to_string()
                    ),
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
        let args = CdCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        if omni_cmd_file().is_none() && !args.locate {
            omni_error!("not available without the shell integration");
            exit(1);
        }

        if let Some(workdir) = &args.workdir {
            self.cd_workdir(workdir, &args);
        } else {
            self.cd_main_org(&args);
        }
        exit(0);
    }

    fn autocompletion(&self) -> CommandAutocompletion {
        CommandAutocompletion::Partial
    }

    fn autocomplete(
        &self,
        comp_cword: usize,
        argv: Vec<String>,
        parameter: Option<String>,
    ) -> Result<(), ()> {
        // We only have the work directory to autocomplete
        if parameter.unwrap_or_default() == "workdir" {
            let repo = argv.get(comp_cword).map_or("", String::as_str);

            path_auto_complete(repo, true)
                .iter()
                .for_each(|s| println!("{}", s));
        }

        Ok(())
    }
}
