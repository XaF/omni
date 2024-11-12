use std::collections::BTreeMap;
use std::process::exit;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::Command;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::dynenv::DynamicEnvExportOptions;
use crate::internal::env::Shell;
use crate::internal::git::report_update_error;
use crate::internal::StringColor;

#[derive(Debug, Clone)]
struct HookEnvCommandArgs {
    quiet: bool,
    keep_shims: bool,
    shell: Shell,
}

impl From<BTreeMap<String, ParseArgsValue>> for HookEnvCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let quiet = matches!(
            args.get("quiet"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let keep_shims = matches!(
            args.get("keep-shims"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let shell = match args.get("shell") {
            Some(ParseArgsValue::SingleString(Some(shell))) => {
                let shell = shell.trim();
                Shell::from_str(shell)
            }
            _ => Shell::from_env(),
        };

        Self {
            quiet,
            keep_shims,
            shell,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HookEnvCommand {}

impl HookEnvCommand {
    pub fn new() -> Self {
        Self {}
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
                    names: vec!["-q".to_string(), "--quiet".to_string()],
                    desc: Some(
                        concat!(
                            "Suppress the output of the hook showing information about the ",
                            "dynamic environment update."
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--keep-shims".to_string()],
                    desc: Some(
                        concat!(
                            "Keep the shims directory in the PATH. This is useful for instance ",
                            "if you are used to launch your IDE from the terminal."
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["shell".to_string()],
                    desc: Some(
                        concat!(
                            "The shell for which to export the dynamic environment. ",
                            "If not provided, the shell will be detected from the environment."
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Enum(
                        Shell::all().iter().map(|s| s.to_string()).collect(),
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
        let args = HookEnvCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        let shell_type = &args.shell;
        match shell_type.dynenv_export_mode() {
            Some(export_mode) => {
                DynamicEnvExportOptions::new(export_mode)
                    .quiet(args.quiet)
                    .keep_shims(args.keep_shims)
                    .apply();
                report_update_error();
                exit(0);
            }
            None => {
                if !args.quiet {
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
