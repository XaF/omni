use std::process::exit;

use once_cell::sync::OnceCell;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::HelpCommand;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::dynenv::DynamicEnvExportOptions;
use crate::internal::env::Shell;
use crate::internal::git::report_update_error;
use crate::internal::StringColor;
use crate::omni_error;

#[derive(Debug, Clone)]
struct HookEnvCommandArgs {
    quiet: bool,
    keep_shims: bool,
    shell: Shell,
}

impl HookEnvCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(clap::Arg::new("shell").action(clap::ArgAction::Set))
            .arg(
                clap::Arg::new("quiet")
                    .short('q')
                    .long("quiet")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("keep-shims")
                    .long("keep-shims")
                    .action(clap::ArgAction::SetTrue),
            )
            .try_get_matches_from(&parse_argv);

        let matches = match matches {
            Ok(matches) => matches,
            Err(err) => {
                match err.kind() {
                    clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                        HelpCommand::new().exec(vec!["hook".to_string(), "env".to_string()]);
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

        let shell = matches
            .get_one::<String>("shell")
            .map(|shell| shell.as_str())
            .map(Shell::from_str)
            .unwrap_or_else(Shell::from_env);
        let quiet = *matches.get_one::<bool>("quiet").unwrap_or(&false);
        let keep_shims = *matches.get_one::<bool>("keep-shims").unwrap_or(&false);

        Self {
            shell,
            quiet,
            keep_shims,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HookEnvCommand {
    cli_args: OnceCell<HookEnvCommandArgs>,
}

impl HookEnvCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &HookEnvCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }
}

impl BuiltinCommand for HookEnvCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["hook".to_string(), "env".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Hook used to update the dynamic environment\n",
                "\n",
                "The \x1B[1m\x1B[4menv\x1B[0m hook is called during your shell prompt to set the ",
                "dynamic environment required for \x1B[3momni up\x1B[0m-ed repositories.",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    name: "--quiet".to_string(),
                    desc: Some(
                        concat!(
                            "Suppress the output of the hook showing information about the ",
                            "dynamic environment update."
                        )
                        .to_string(),
                    ),
                    required: false,
                    ..Default::default()
                },
                SyntaxOptArg {
                    name: "--keep-shims".to_string(),
                    desc: Some(
                        concat!(
                            "Keep the shims directory in the PATH. This is useful for instance ",
                            "if you are used to launch your IDE from the terminal."
                        )
                        .to_string(),
                    ),
                    required: false,
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
        if self.cli_args.set(HookEnvCommandArgs::parse(argv)).is_err() {
            unreachable!();
        }

        let shell_type = &self.cli_args().shell;
        match shell_type.dynenv_export_mode() {
            Some(export_mode) => {
                DynamicEnvExportOptions::new(export_mode)
                    .quiet(self.cli_args().quiet)
                    .keep_shims(self.cli_args().keep_shims)
                    .apply();
                report_update_error();
                exit(0);
            }
            None => {
                if !self.cli_args().quiet {
                    eprintln!(
                        "{} {} {}",
                        "omni:".light_cyan(),
                        "invalid export mode:".red(),
                        shell_type.to_str(),
                    );
                }
                exit(1);
            }
        }
    }

    fn autocompletion(&self) -> bool {
        false
    }

    fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        Err(())
    }
}
