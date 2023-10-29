use std::collections::BTreeMap;
use std::collections::HashSet;
use std::process::exit;

use clap;
use once_cell::sync::OnceCell;

use crate::internal::commands::command_loader;
use crate::internal::commands::void::VoidCommand;
use crate::internal::commands::Command;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::user_interface::term_width;
use crate::internal::user_interface::wrap_blocks;
use crate::internal::user_interface::wrap_text;
use crate::internal::user_interface::StringColor;
use crate::omni_error;
use crate::omni_header;
use crate::omni_print;

#[derive(Debug, Clone)]
struct HelpCommandArgs {
    unparsed: Vec<String>,
}

impl HelpCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("unparsed")
                    .action(clap::ArgAction::Append)
                    .allow_hyphen_values(true),
            )
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["help".to_string()]);
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

        let matches = matches.unwrap();

        let unparsed = if let Some(unparsed) = matches.get_many::<String>("unparsed").clone() {
            unparsed
                .into_iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        Self { unparsed: unparsed }
    }
}

#[derive(Debug, Clone)]
pub struct HelpCommand {
    cli_args: OnceCell<HelpCommandArgs>,
}

impl HelpCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &HelpCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
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
        if let Err(_) = self.cli_args.set(HelpCommandArgs::parse(argv)) {
            unreachable!();
        }

        let argv = self.cli_args().unparsed.clone();

        if argv.is_empty() {
            self.help_global();
            exit(0);
        }

        let command_loader = command_loader(".");
        if let Some((omni_cmd, called_as, argv)) = command_loader.to_serve(&argv) {
            if argv.is_empty() {
                self.help_command(omni_cmd, called_as);
                exit(0);
            }
        }

        if command_loader.has_subcommand_of(&argv) {
            self.help_void(argv.to_vec());
            exit(0);
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

        self.print_categorized_command_help(vec![]);
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

        self.print_categorized_command_help(called_as);

        eprintln!(
            "\n{} {}",
            "Source:".to_string().light_black(),
            command.help_source().underline()
        );
    }

    fn help_void(&self, called_as: Vec<String>) {
        eprintln!(
            "{}\n\nProvides {} commands\n\n{} omni {} {} [options] ARG...",
            omni_header!(),
            called_as.join(" ").to_string().italic(),
            "Usage:".to_string().italic(),
            called_as.join(" "),
            "<command>".to_string().cyan(),
        );

        self.print_categorized_command_help(called_as);
        eprintln!("");
    }

    fn print_categorized_command_help(
        &self,
        prefix: Vec<String>,
        // category_prefix: Vec<String>,
    ) -> bool {
        let command_loader = command_loader(".");
        let organizer = HelpCommandOrganizer::new_from_commands(command_loader.commands.clone());
        let commands = organizer.get_commands_with_fold(prefix.clone(), 1);

        if commands.is_empty() {
            return false;
        }

        let mut seen = HashSet::new();

        // Get the longest command so we know how to justify the help
        let longest_command = commands
            .iter()
            .map(|cmd| {
                let command = &cmd.command;
                let all_names = command
                    .all_names_with_prefix(prefix.clone())
                    .iter()
                    .map(|name| name.join(" "))
                    .collect::<Vec<String>>();
                let all_names_len = all_names.join(", ").len();

                let num_folded = match cmd.num_folded() {
                    0 => 0,
                    _ => 2,
                };

                all_names_len + num_folded
            })
            .max()
            .unwrap_or(0);
        let ljust = std::cmp::max(longest_command + 2, 15);
        let join_str = format!("\n  {}", " ".repeat(ljust));
        let help_just = term_width() - ljust - 4;

        // Keep track of the current category so we only print it once
        let mut cur_category = None;

        // Print the help
        for cmd in commands {
            let command = &cmd.command;
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
                .all_names_with_prefix(prefix.clone())
                .iter()
                .map(|name| name.join(" "))
                .collect::<Vec<String>>();
            let all_names_len = all_names.join(", ").len();
            let all_names = all_names
                .iter()
                .map(|name| name.cyan())
                .collect::<Vec<String>>()
                .join(", ");

            let num_folded_len = match cmd.num_folded() {
                0 => 0,
                _ => 2,
            };
            let num_folded = match cmd.num_folded() {
                0 => "".to_string(),
                _ => " ▶".to_string().light_black(),
            };

            let missing_just = ljust - all_names_len - num_folded_len;
            let str_name = format!("  {}{}{}", all_names, num_folded, " ".repeat(missing_just));

            let help = wrap_text(&command.help_short(), help_just).join(join_str.as_str());

            eprintln!("{}{}", str_name, help);
        }

        return true;
    }
}

type HelpCommandMetadataKey = (usize, Vec<String>, Vec<String>);

#[derive(Debug, Clone)]
struct HelpCommandOrganizer {
    commands: BTreeMap<HelpCommandMetadataKey, HelpCommandMetadata>,
}

impl HelpCommandOrganizer {
    fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }

    fn new_from_commands(commands: Vec<Command>) -> Self {
        let mut metadata = Self::new();

        for command in commands {
            for command_name in command.all_names() {
                metadata.add_command(command_name.clone(), &command);
            }
        }

        metadata
    }

    fn add_command(&mut self, path: Vec<String>, command: &Command) {
        let mut command_itself = true;

        for i in (1..=path.len()).rev() {
            let (cmdpath, _) = path.split_at(i);
            let cmd_sort_key = cmdpath.to_vec();
            let cat_sort_key = command.category_sort_key();
            let key = (cat_sort_key.0, cat_sort_key.1, cmd_sort_key.clone());

            // Check if that key already exists in the commands
            if let Some(cmd) = self.commands.get_mut(&key) {
                if !command_itself {
                    cmd.subcommands.insert(path.clone());
                    continue;
                }

                match cmd.command {
                    Command::Void(_) => {
                        cmd.command = command.clone();
                        cmd.subcommands.insert(path.clone());
                    }
                    _ => {
                        // Command exists twice in the path, but second entry in the path
                        // won't ever be called; we can simply skip and go to next step
                        continue;
                    }
                }
            } else {
                let insert_command = if command_itself {
                    command.clone()
                } else {
                    Command::Void(VoidCommand::new(
                        cmd_sort_key,
                        cat_sort_key.0,
                        command.category().unwrap_or(vec![]),
                    ))
                };

                let mut new_command = HelpCommandMetadata::new(&insert_command);
                new_command.subcommands.insert(path.clone());
                self.commands.insert(key, new_command);
            }

            command_itself = false;
        }
    }

    fn get_commands_with_fold(
        &self,
        prefix: Vec<String>,
        max_before_fold: usize,
    ) -> Vec<HelpCommandMetadata> {
        let mut commands = vec![];

        let considered_commands = if prefix.is_empty() {
            // Get all commands
            self.commands.iter().collect::<BTreeMap<_, _>>()
        } else {
            // Get all commands prefixed by `prefix` but not exactly `prefix`
            self.commands
                .iter()
                .filter(|(key, _)| key.2.starts_with(&prefix) && key.2.len() > prefix.len())
                .collect::<BTreeMap<_, _>>()
        };

        let mut seen = HashSet::new();

        for (key, metadata) in considered_commands {
            if !seen.insert(key.clone()) {
                continue;
            }

            let mut metadata = metadata.clone();
            metadata.folding = metadata.subcommands.len() > max_before_fold;

            if metadata.folding {
                for subcommand in metadata.subcommands.iter() {
                    let sub_key = (key.0, key.1.clone(), subcommand.clone());
                    seen.insert(sub_key);
                }
            }

            let command_is_a_void = match &metadata.command {
                Command::Void(_) => true,
                _ => false,
            };

            if metadata.folding || !command_is_a_void {
                commands.push(metadata.clone());
            }
        }

        return commands;
    }
}

#[derive(Debug, Clone)]
struct HelpCommandMetadata {
    command: Command,
    subcommands: HashSet<Vec<String>>,
    folding: bool,
}

impl HelpCommandMetadata {
    fn new(command: &Command) -> Self {
        Self {
            command: command.clone(),
            subcommands: HashSet::new(),
            folding: false,
        }
    }

    fn num_folded(&self) -> usize {
        match self.folding {
            true => self.subcommands.len(),
            false => 0,
        }
    }
}
