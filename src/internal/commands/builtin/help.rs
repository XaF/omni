use std::collections::HashSet;
use std::process::exit;

use crate::internal::commands::command_loader;
use crate::internal::commands::Command;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::user_interface::term_width;
use crate::internal::user_interface::wrap_blocks;
use crate::internal::user_interface::wrap_text;
use crate::internal::user_interface::StringColor;
use crate::omni_header;
use crate::omni_print;

#[derive(Debug, Clone)]
pub struct HelpCommand {}

impl HelpCommand {
    pub fn new() -> Self {
        Self {}
    }

    pub fn name(&self) -> Vec<String> {
        vec!["help".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Show help for omni commands\n",
                "\n",
                "If no command is given, show a list of all available commands.",
            )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            arguments: vec![],
            options: vec![SyntaxOptArg {
                name: "command".to_string(),
                desc: Some("The command to get help for".to_string()),
            }],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if argv.is_empty() {
            self.help_global();
            exit(0);
        }

        if let Some((omni_cmd, called_as, argv)) = command_loader(".").to_serve(&argv) {
            if argv.is_empty() {
                self.help_command(omni_cmd, called_as);
                exit(0);
            }
        }

        omni_print!(format!(
            "{} {}",
            "command not found:".to_string().red(),
            argv.join(" ")
        ));
        exit(1);
    }

    pub fn autocompletion(&self) -> bool {
        true
    }

    pub fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) {
        command_loader(".").complete(comp_cword, argv, false);
    }

    fn help_global(&self) {
        eprintln!(
            "{}\n\n{} omni {} [options] ARG...",
            omni_header!(),
            "Usage:".to_string().italic(),
            "<command>".to_string().cyan(),
        );

        let mut seen = HashSet::new();

        let command_loader = command_loader(".");
        let commands = command_loader.sorted();

        // Get the longest command so we know how to justify the help
        let longest_command = commands
            .iter()
            .map(|command| {
                let name_len = command.name().join(" ").len();
                let aliases_len: usize = command
                    .aliases()
                    .iter()
                    .map(|alias| alias.join(" ").len() + 2)
                    .sum();
                name_len + aliases_len
            })
            .max()
            .unwrap_or(0);
        let ljust = std::cmp::max(longest_command + 2, 15);
        let join_str = format!("\n  {}", " ".repeat(ljust));
        let help_just = term_width() - ljust - 4;

        // Keep track of the current category so we only print it once
        let mut cur_category = None;

        // Print the help
        for command in commands {
            let name = command.name().join(" ");
            if !seen.insert(name.clone()) {
                continue;
            }

            let mut category = command.category();
            if category.is_some() && category.as_ref().unwrap().is_empty() {
                category = None;
            }

            if category != cur_category {
                cur_category = category.clone();
                let new_category = if let Some(category) = category {
                    let mut cat_elems = category.clone();
                    let last_elem = cat_elems.pop().expect("Category should not be empty");
                    cat_elems = cat_elems
                        .iter()
                        .map(|elem| elem.light_black().bold())
                        .collect();
                    cat_elems.push(last_elem.bold());
                    cat_elems.reverse();
                    cat_elems.join(" < ")
                } else {
                    "Uncategorized".to_string().bold()
                };
                let line = format!("{}", new_category);
                eprintln!("\n{}", line);
            }

            let all_names = command
                .all_names()
                .iter()
                .map(|name| name.join(" "))
                .collect::<Vec<String>>();
            let all_names_len = all_names.join(", ").len();
            let all_names = all_names
                .iter()
                .map(|name| name.cyan())
                .collect::<Vec<String>>()
                .join(", ");

            let missing_just = ljust - all_names_len;
            let str_name = format!("  {}{}", all_names, " ".repeat(missing_just));

            let help = wrap_text(&command.help_short(), help_just).join(join_str.as_str());

            eprintln!("{}{}", str_name, help);
        }

        eprintln!("");
    }

    fn help_command(&self, command: &Command, called_as: Vec<String>) {
        eprintln!("{}", omni_header!());

        let max_width = term_width() - 4;

        let help = command.help();
        if help != "" {
            eprintln!("\n{}", wrap_blocks(&help, max_width).join("\n"));
        }

        eprintln!(
            "\n{} {}",
            "Usage:".to_string().italic().bold(),
            command.usage(Some(called_as.join(" "))).bold()
        );

        if let Some(syntax) = command.syntax() {
            if syntax.arguments.len() > 0 || syntax.options.len() > 0 {
                // Make a single vector with contents from both syntax.arguments and syntax.options
                let mut args = syntax.arguments.clone();
                args.extend(syntax.options.clone());

                let longest = args.iter().map(|arg| arg.name.len()).max().unwrap_or(0);
                let ljust = std::cmp::max(longest + 2, 15);
                let join_str = format!("\n  {}", " ".repeat(ljust));

                for arg in args {
                    let missing_just = ljust - arg.name.len();
                    let str_name = format!("  {}{}", arg.name.cyan(), " ".repeat(missing_just));
                    let help = if let Some(desc) = arg.desc {
                        wrap_text(&desc, max_width - ljust).join(join_str.as_str())
                    } else {
                        "".to_string()
                    };
                    eprintln!("\n{}{}", str_name, help);
                }
            }
        }

        eprintln!(
            "\n{} {}",
            "Source:".to_string().light_black(),
            command.help_source().underline()
        );
    }
}
