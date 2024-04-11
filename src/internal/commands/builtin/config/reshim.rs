use std::process::exit;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::HelpCommand;
use crate::internal::config::up::utils::reshim;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::CommandSyntax;
use crate::internal::user_interface::StringColor;

#[derive(Debug, Clone)]
pub struct ConfigReshimCommand {}

impl ConfigReshimCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for ConfigReshimCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["config".to_string(), "reshim".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Regenerate the shims for the environments managed by omni\n",
                "\n",
                "This will get all the binaries that exist for at least one of the ",
                "environments managed by omni and create a shim for them in the ",
                "shim directory.\n",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![],
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        if !argv.is_empty() {
            HelpCommand::new().exec(self.name());
            exit(1);
        }

        let progress_handler = PrintProgressHandler::new("reshim:".light_blue(), None);
        match reshim(&progress_handler) {
            Ok(Some(result)) => {
                progress_handler.success_with_message(result);
            }
            Ok(None) => {
                progress_handler.success_with_message("nothing to do".light_black());
            }
            Err(e) => {
                progress_handler.error_with_message(format!("{}", e));
                exit(1);
            }
        }

        exit(0);
    }

    fn autocompletion(&self) -> bool {
        false
    }

    fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        Err(())
    }
}
