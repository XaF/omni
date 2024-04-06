use std::process::exit;

use uuid::Uuid;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::config::CommandSyntax;

#[derive(Debug, Clone)]
pub struct HookUuidCommand {}

impl HookUuidCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for HookUuidCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["hook".to_string(), "uuid".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(concat!(
            "Hook to generate a UUID\n",
            "\n",
            "The \x1B[1m\x1B[4muuid\x1B[0m hook provides and alternative to \x1B[3muuidgen\x1B[0m, ",
            "in case it is not installed, so that omni can work without extra dependencies..",
        ).to_string())
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

    fn exec(&self, _argv: Vec<String>) {
        let uuid = Uuid::new_v4();
        println!("{}", uuid);
        exit(0);
    }

    fn autocompletion(&self) -> bool {
        false
    }

    fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        Err(())
    }
}
