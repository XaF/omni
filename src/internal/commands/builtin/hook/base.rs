use crate::internal::commands::base::BuiltinCommand;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;

#[derive(Debug, Clone)]
pub struct HookCommand {}

impl HookCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for HookCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["hook".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(concat!("Call one of omni's hooks for the shell\n",).to_string())
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["hook".to_string()],
                    desc: Some("Which hook to call".to_string()),
                    required: true,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["options".to_string()],
                    desc: Some("Any options to pass to the hook.".to_string()),
                    leftovers: true,
                    allow_hyphen_values: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, _argv: Vec<String>) {}

    fn autocompletion(&self) -> bool {
        false
    }

    fn autocomplete(&self, comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        if comp_cword == 0 {
            println!("env");
            println!("init");
            println!("uuid");
        }

        Ok(())
    }
}
