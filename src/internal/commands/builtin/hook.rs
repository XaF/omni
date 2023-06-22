use std::process::exit;

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
        Some(concat!(
            "Call one of omni's hooks for the shell.\n",
            "\n",
            "The \x1B[1m\x1B[4minit\x1B[0m hook will provide you with the command to run to ",
            "initialize omni in your shell. You can specify which shell you wish to load it ",
            "for by specifying either one of \x1B[1mzsh\x1B[0m, \x1B[1mbash\x1B[0m, or ",
            "\x1B[1mfish\x1B[0m as optional parameter. If no argument is specified, the login ",
            "shell, as provided by the \x1B[3mSHELL\x1B[0m environment variable, will be used. ",
            "You can load omni in your shell by using \x1B[1meval \"$(omni hook init YOURSHELL)",
            "\"\x1B[0m for bash or zsh, or \x1B[1momni hook init fish | source\x1B[0m for fish.\n",
            "\n",
            "The \x1B[1m\x1B[4menv\x1B[0m hook is called during your shell prompt to set the ",
            "dynamic environment required for \x1B[3momni up\x1B[0m-ed repositories.\n",
            "\n",
            "The \x1B[1m\x1B[4muuid\x1B[0m hook provides and alternative to \x1B[3muuidgen\x1B[0m, ",
            "in case it is not installed, so that omni can work without extra dependencies..",
        ).to_string())
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            arguments: vec![SyntaxOptArg {
                name: "hook".to_string(),
                desc: Some("Which hook to call".to_string()),
            }],
            options: vec![SyntaxOptArg {
                name: "options...".to_string(),
                desc: Some("Any options to pass to the hook.".to_string()),
            }],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, comp_cword: usize, _argv: Vec<String>) {
        if comp_cword == 0 {
            println!("env");
            println!("init");
            println!("uuid");
        }
        exit(0);
    }
}
