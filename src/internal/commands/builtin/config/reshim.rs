use std::process::exit;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::Command;
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
        Some(CommandSyntax::default())
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        // We do not have any arguments here, but parse them so we can error out if any are passed
        let command = Command::Builtin(self.clone_boxed());
        let _ = command
            .exec_parse_args_typed(argv, self.name())
            .expect("should have args to parse");

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
}
