use std::process::exit;

use crate::internal::config::CommandSyntax;
use crate::internal::dynenv::update_dynamic_env;
use crate::internal::env::Shell;
use crate::internal::git::report_update_error;
use crate::internal::StringColor;

#[derive(Debug, Clone)]
pub struct HookEnvCommand {}

impl HookEnvCommand {
    pub fn new() -> Self {
        Self {}
    }

    pub fn name(&self) -> Vec<String> {
        vec!["hook".to_string(), "env".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
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

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        let shell_type = if argv.len() > 2 {
            Shell::from_str(&argv[2])
        } else {
            Shell::from_env()
        };

        match shell_type.dynenv_export_mode() {
            Some(export_mode) => {
                update_dynamic_env(export_mode);
                report_update_error();
                exit(0);
            }
            None => {
                eprintln!(
                    "{} {} {}",
                    "omni:".light_cyan(),
                    "invalid export mode:".red(),
                    shell_type.to_str(),
                );
                exit(1);
            }
        }
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        Err(())
    }
}
