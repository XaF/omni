use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;

#[derive(Debug, Clone)]
pub struct HookCommand {}

impl HookCommand {
    pub fn new() -> Self {
        Self {}
    }

    pub fn name(&self) -> Vec<String> {
        vec!["hook".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(concat!("Call one of omni's hooks for the shell\n",).to_string())
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "hook".to_string(),
                    desc: Some("Which hook to call".to_string()),
                    required: true,
                },
                SyntaxOptArg {
                    name: "options...".to_string(),
                    desc: Some("Any options to pass to the hook.".to_string()),
                    required: false,
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        if comp_cword == 0 {
            println!("env");
            println!("init");
            println!("uuid");
        }

        Ok(())
    }
}
