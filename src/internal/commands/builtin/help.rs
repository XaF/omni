use std::collections::BTreeMap;
use std::collections::HashSet;
use std::process::exit;

use serde::Serialize;

use crate::internal::cache::utils as cache_utils;
use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::base::CommandAutocompletion;
use crate::internal::commands::command_loader;
use crate::internal::commands::void::VoidCommand;
use crate::internal::commands::Command;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::user_interface::colors::strip_colors_if_needed;
use crate::internal::user_interface::print::strip_ansi_codes;
use crate::internal::user_interface::term_width;
use crate::internal::user_interface::wrap_blocks;
use crate::internal::user_interface::wrap_text;
use crate::internal::user_interface::StringColor;
use crate::omni_error;
use crate::omni_header;
use crate::omni_print;

#[derive(Debug, Clone)]
struct HelpCommandArgs {
    unfold: bool,
    output: HelpCommandOutput,
    command: Vec<String>,
}

impl From<BTreeMap<String, ParseArgsValue>> for HelpCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let unfold = matches!(
            args.get("unfold"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let command = match args.get("command") {
            Some(ParseArgsValue::ManyString(values)) => values
                .iter()
                .filter_map(|v| v.clone())
                .collect::<Vec<String>>(),
            _ => vec![],
        };
        let output = match args.get("output") {
            Some(ParseArgsValue::SingleString(Some(value))) => match value.as_str() {
                "json" => HelpCommandOutput::Json,
                "plain" => HelpCommandOutput::Plain,
                _ => unreachable!("unknown value for output"),
            },
            _ => HelpCommandOutput::Plain,
        };

        Self {
            unfold,
            output,
            command,
        }
    }
}

#[derive(Debug, Clone)]
enum HelpCommandOutput {
    Plain,
    Json,
}

#[derive(Debug, Clone)]
pub struct HelpCommand {}

impl HelpCommand {
    pub fn new() -> Self {
        Self {}
    }

    fn help_global(&self, printer: &dyn HelpCommandPrinter, unfold: bool) {
        printer.print_global_help(unfold);
    }

    fn help_command(
        &self,
        printer: &dyn HelpCommandPrinter,
        command: &Command,
        called_as: Vec<String>,
        unfold: bool,
    ) {
        printer.print_command_help(command, called_as, unfold);
    }

    fn help_void(&self, printer: &dyn HelpCommandPrinter, called_as: Vec<String>, unfold: bool) {
        self.help_command(
            printer,
            &Command::Void(VoidCommand::new_for_help(called_as.clone())),
            called_as,
            unfold,
        );
    }

    pub fn exec_with_exit_code(&self, argv: Vec<String>, exit_code: i32) {
        let command = Command::Builtin(self.clone_boxed());
        let args = HelpCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        let printer: Box<dyn HelpCommandPrinter> = match args.output {
            HelpCommandOutput::Plain => Box::new(HelpCommandPlainPrinter::new()),
            HelpCommandOutput::Json => Box::new(HelpCommandJsonPrinter::new()),
        };

        let argv = args.command.clone();
        if argv.is_empty() {
            self.help_global(&*printer, args.unfold);
            exit(exit_code);
        }

        let command_loader = command_loader(".");
        if let Some((omni_cmd, called_as, argv)) = command_loader.to_serve(&argv) {
            if argv.is_empty() {
                self.help_command(&*printer, omni_cmd, called_as, args.unfold);
                exit(exit_code);
            }
        }

        if command_loader.has_subcommand_of(&argv) {
            self.help_void(&*printer, argv.to_vec(), args.unfold);
            exit(exit_code);
        }

        printer.print_error("command not found", &argv.join(" "));
        exit(1);
    }
}

impl BuiltinCommand for HelpCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["help".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Show help for omni commands\n",
                "\n",
                "If no command is given, show a list of all available commands.",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["--unfold".to_string()],
                    desc: Some("Show all subcommands".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-o".to_string(), "--output".to_string()],
                    desc: Some("Output format".to_string()),
                    arg_type: SyntaxOptArgType::Enum(vec!["json".to_string(), "plain".to_string()]),
                    default: Some("plain".to_string()),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["command".to_string()],
                    desc: Some("The command to get help for".to_string()),
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

    fn exec(&self, argv: Vec<String>) {
        self.exec_with_exit_code(argv, 0);
    }

    fn autocompletion(&self) -> CommandAutocompletion {
        // TODO: convert to partial so the autocompletion works for options too
        CommandAutocompletion::Full
    }

    fn autocomplete(
        &self,
        comp_cword: usize,
        argv: Vec<String>,
        _parameter: Option<String>,
    ) -> Result<(), ()> {
        command_loader(".").complete(comp_cword, argv, false)
    }
}

trait HelpCommandPrinter {
    fn print_global_help(&self, unfold: bool);
    fn print_command_help(&self, command: &Command, called_as: Vec<String>, unfold: bool);
    fn print_error(&self, error_type: &str, error_msg: &str);
}

struct HelpCommandPlainPrinter {}

impl HelpCommandPrinter for HelpCommandPlainPrinter {
    fn print_global_help(&self, unfold: bool) {
        eprintln!(
            "{}\n\n{} {} {} {} {}",
            omni_header!(),
            "Usage:".italic().bold(),
            "omni".bold(),
            "<command>".cyan().bold(),
            "[options]".cyan().bold(),
            "ARG...".cyan().bold(),
        );

        self.print_categorized_command_help(vec![], unfold);
        eprintln!();
    }

    fn print_command_help(&self, command: &Command, called_as: Vec<String>, unfold: bool) {
        eprintln!("{}", omni_header!());

        let max_width = term_width() - 4;

        let help = command.help();
        if !help.is_empty() {
            eprintln!(
                "\n{}",
                wrap_blocks(&strip_colors_if_needed(help), max_width).join("\n")
            );
        }

        let is_shadow_name = command
            .all_names()
            .iter()
            .any(|name| name.join(" ") == called_as.join(" "));
        if !is_shadow_name {
            eprintln!(
                "\n{}",
                format!(
                    "\u{26A0}\u{FE0F}  '{}' is a shadow alias of '{}';\n   you should use the latter instead, as shadow\n   aliases can be overriden by any command at any time.",
                    called_as.join(" "),
                    command.name().join(" "),
                ).light_yellow(),
            );
        }

        let tags = command.tags();
        if !tags.is_empty() {
            eprintln!();

            let taglen = tags.keys().map(|tag| tag.len()).max().unwrap_or(0) + 2;
            for (tag, value) in tags {
                let wrapped_value = wrap_text(&value, max_width - taglen - 2);
                eprintln!(
                    "{}{:<width$}{}",
                    format!("{}:", tag).bold(),
                    "",
                    wrapped_value[0],
                    width = taglen - tag.len() - 1,
                );

                wrapped_value.iter().skip(1).for_each(|line| {
                    eprintln!("{:<width$}{}", "", line, width = taglen);
                });
            }
        }

        let command_usage = command.usage(Some(called_as.join(" "))).bold();
        let wrapped_usage = wrap_text(&command_usage, max_width - 7); // 7 is the length of "Usage: "

        eprintln!("\n{} {}", "Usage:".underline().bold(), wrapped_usage[0]);
        wrapped_usage.iter().skip(1).for_each(|line| {
            eprintln!("       {}", line);
        });

        if let Some(syntax) = command.syntax() {
            if !syntax.parameters.is_empty() {
                let (arguments, options): (Vec<_>, Vec<_>) = syntax
                    .parameters
                    .iter()
                    .partition(|arg| arg.is_positional());

                if !arguments.is_empty() {
                    eprintln!("\n{}", "Arguments:".bold().underline());
                    if let Err(err) = self.print_syntax_column_help(&arguments) {
                        omni_error!(err);
                    }
                }

                if !options.is_empty() {
                    eprintln!("\n{}", "Options:".bold().underline());
                    if let Err(err) = self.print_syntax_column_help(&options) {
                        omni_error!(err);
                    }
                }
            }
        }

        self.print_categorized_command_help(called_as, unfold);

        eprintln!(
            "\n{} {}",
            "Source:".light_black(),
            command.help_source().underline()
        );
    }

    fn print_error(&self, error_type: &str, error_msg: &str) {
        omni_print!(format!(
            "{} {}",
            format!("{}:", error_type.red()),
            error_msg,
        ));
    }
}

impl HelpCommandPlainPrinter {
    fn new() -> Self {
        Self {}
    }

    fn print_syntax_column_help(&self, args: &[&SyntaxOptArg]) -> Result<(), String> {
        // Get the longest command so we know how to justify the help
        const MIN_LJUST: usize = 15;
        let abs_max_ljust = term_width() / 2;
        let max_ljust = std::cmp::min(std::cmp::max(term_width() / 3, 40), abs_max_ljust);
        if max_ljust < MIN_LJUST {
            return Err("terminal width is too small to print help".to_string());
        }

        let (_, longest_under_threshold) = get_longest_under_threshold_func::<&SyntaxOptArg>(
            args.iter().copied(),
            |arg| arg.help_name(true, false),
            None::<for<'a, 'b, 'c, 'd> fn(&'a &SyntaxOptArg, &'b _, &'c _, &'d _) -> _>,
            max_ljust - 4,
        );
        let ljust = std::cmp::max(longest_under_threshold + 2, MIN_LJUST);
        let help_just = term_width() - ljust - 4;

        // Print the help
        for arg in args.iter() {
            let help_name = arg.help_name(true, true);

            let (print_name_on_same_line, wrap_threshold) = match strip_ansi_codes(&help_name).len()
            {
                n if n <= longest_under_threshold => (true, longest_under_threshold),
                _ => (false, term_width() - 4),
            };

            let wrapped_name = wrap_text(&help_name, wrap_threshold);
            let wrapped_name_and_len = wrapped_name
                .iter()
                .map(|name| (name.to_string(), strip_ansi_codes(name).len()))
                .collect::<Vec<(String, usize)>>();

            // Prepare the help contents
            let help_desc = wrap_text(&strip_colors_if_needed(arg.help_desc()), help_just);
            // Remove help_desc lines until we find the first non-empty line
            let help_desc = help_desc
                .iter()
                .skip_while(|line| line.trim().is_empty())
                .map(|line| line.to_string())
                .collect::<Vec<String>>();

            // Prepare the help message to print for this command
            let empty_str = "".to_string();
            let mut buf = String::new();
            if print_name_on_same_line {
                wrapped_name_and_len
                    .iter()
                    .take(wrapped_name_and_len.len() - 1)
                    .for_each(|(name, _)| {
                        buf.push_str(&format!("  {}\n", name));
                    });

                let first_desc_line = help_desc.first().unwrap_or(&empty_str).trim();
                let last_name_line = wrapped_name_and_len
                    .last()
                    .expect("Name should not be empty");

                if first_desc_line.is_empty() {
                    buf.push_str(&format!("  {}\n", last_name_line.0));
                } else {
                    buf.push_str(&format!(
                        "  {}{}{}\n",
                        last_name_line.0,
                        " ".repeat(ljust - last_name_line.1),
                        first_desc_line,
                    ));
                }

                help_desc.iter().skip(1).for_each(|line| {
                    buf.push_str(&format!("{}{}\n", " ".repeat(ljust + 2), line));
                });
            } else {
                wrapped_name_and_len.iter().for_each(|(name, _)| {
                    buf.push_str(&format!("  {}\n", name));
                });

                help_desc.iter().for_each(|line| {
                    buf.push_str(&format!("{}{}\n", " ".repeat(ljust + 2), line));
                });
            }

            // Print the help message for this argument
            eprint!("{}", buf);
        }

        Ok(())
    }

    fn print_categorized_command_help(&self, prefix: Vec<String>, unfold: bool) -> bool {
        let command_loader = command_loader(".");
        let organizer = HelpCommandOrganizer::new_from_commands(command_loader.commands.clone());
        let commands = organizer.get_commands_with_fold(
            prefix.clone(),
            match unfold {
                true => 0,
                false => 1,
            },
        );

        if commands.is_empty() {
            return false;
        }

        let mut seen = HashSet::new();

        // Get the longest command so we know how to justify the help
        const MIN_LJUST: usize = 15;
        let abs_max_ljust = term_width() / 2;
        let max_ljust = std::cmp::min(std::cmp::max(term_width() / 3, 40), abs_max_ljust);
        if max_ljust < MIN_LJUST {
            omni_error!("terminal width is too small to print help");
            exit(1);
        }

        let (_, longest_under_threshold) = get_longest_command(&commands, &prefix, max_ljust - 4);
        let ljust = std::cmp::max(longest_under_threshold + 2, MIN_LJUST);

        let help_just = term_width() - ljust - 4;

        // Keep track of the current category so we only print it once
        let mut cur_category = None;

        // Print the help
        for (idx, cmd) in commands.iter().enumerate() {
            let command = &cmd.command;
            let name = command.name().join(" ");
            if !seen.insert(name.clone()) {
                continue;
            }

            let mut category = command.category();
            if category.is_some() && category.as_ref().unwrap().is_empty() {
                category = None;
            }

            if idx == 0 || category != cur_category {
                cur_category.clone_from(&category);
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
                    "Uncategorized".bold()
                };
                let line = new_category.to_string();
                eprintln!("\n{}", line);
            }

            let all_names = command
                .all_names_with_prefix(prefix.clone())
                .iter()
                .map(|name| name.join(" "))
                .collect::<Vec<String>>();
            let all_names_str = all_names
                .iter()
                .map(|name| name.cyan())
                .collect::<Vec<String>>()
                .join(", ");

            let (num_folded, num_folded_len) = match cmd.num_folded() {
                0 => ("".to_string(), 0),
                _ => (" ▶".light_black(), 2),
            };

            let (print_name_on_same_line, wrap_threshold) = match get_command_length(cmd, &prefix) {
                n if n <= longest_under_threshold => (true, longest_under_threshold),
                _ => (false, term_width() - 4),
            };

            let all_names_str = format!("{}{}", all_names_str, "-".repeat(num_folded_len));
            let all_names_vec = wrap_text(&all_names_str, wrap_threshold);
            let all_names_and_len = all_names_vec
                .iter()
                .enumerate()
                .map(|(idx, name)| {
                    let namelen = strip_ansi_codes(name).len();
                    let name = if idx == all_names_vec.len() - 1 {
                        match name.strip_suffix("--") {
                            Some(name) => format!("{}{}", name, num_folded),
                            None => name.to_string(),
                        }
                    } else {
                        name.to_string()
                    };
                    (name, namelen)
                })
                .collect::<Vec<(String, usize)>>();

            // Prepare the help contents
            let help_vec = wrap_text(&strip_colors_if_needed(command.help_short()), help_just);

            // Prepare the help message to print for this command
            let empty_str = "".to_string();
            let mut buf = String::new();
            if print_name_on_same_line {
                all_names_and_len
                    .iter()
                    .take(all_names_and_len.len() - 1)
                    .for_each(|(name, _)| {
                        buf.push_str(&format!("  {}\n", name));
                    });

                let first_desc_line = help_vec.first().unwrap_or(&empty_str);
                let last_name_line = all_names_and_len.last().expect("Name should not be empty");
                buf.push_str(&format!(
                    "  {}{}{}\n",
                    last_name_line.0,
                    " ".repeat(ljust - last_name_line.1),
                    first_desc_line,
                ));

                help_vec.iter().skip(1).for_each(|line| {
                    buf.push_str(&format!("{}{}\n", " ".repeat(ljust + 2), line));
                });
            } else {
                all_names_and_len.iter().for_each(|(name, _)| {
                    buf.push_str(&format!("  {}\n", name));
                });

                help_vec.iter().for_each(|line| {
                    buf.push_str(&format!("{}{}\n", " ".repeat(ljust + 2), line));
                });
            }

            eprint!("{}", buf);
        }

        true
    }
}

struct HelpCommandJsonPrinter {}

#[derive(Debug, Serialize, Clone)]
struct SerializableCommandHelp {
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    usage: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    source: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    category: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    short_help: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    help: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    arguments: Vec<SerializableCommandSyntax>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    options: Vec<SerializableCommandSyntax>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    subcommands: Vec<SerializableSubcommand>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    tags: BTreeMap<String, String>,
}

impl Default for SerializableCommandHelp {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            usage: "".to_string(),
            source: "".to_string(),
            category: vec![],
            short_help: "".to_string(),
            help: "".to_string(),
            arguments: vec![],
            options: vec![],
            subcommands: vec![],
            tags: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct SerializableCommandSyntax {
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    desc: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct SerializableSubcommand {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    category: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    desc: String,
    #[serde(skip_serializing_if = "cache_utils::is_zero")]
    folded: usize,
}

impl HelpCommandPrinter for HelpCommandJsonPrinter {
    fn print_global_help(&self, unfold: bool) {
        let subcommands = self.subcommands(vec![], unfold);
        let command_help = SerializableCommandHelp {
            usage: "omni <command> [options] ARG...".to_string(),
            subcommands,
            ..Default::default()
        };

        let json =
            serde_json::to_string_pretty(&command_help).expect("failed to serialize help to JSON");
        println!("{}", json);
    }

    fn print_command_help(&self, command: &Command, called_as: Vec<String>, unfold: bool) {
        let name = command.name().join(" ");
        let category: Vec<String> = command.category().unwrap_or_default();
        let short_help = strip_ansi_codes(&command.help_short());
        let help = strip_ansi_codes(&command.help());
        let source = command.help_source();
        let usage = strip_ansi_codes(&command.usage(Some(called_as.join(" "))));

        let mut arguments = vec![];
        let mut options = vec![];
        if let Some(syntax) = command.syntax() {
            if !syntax.parameters.is_empty() {
                for param in syntax.parameters.iter() {
                    let name = param.help_name(true, false);
                    let desc = strip_ansi_codes(&param.help_desc());

                    let serializable_syntax = SerializableCommandSyntax { name, desc };

                    if param.is_positional() {
                        arguments.push(serializable_syntax);
                    } else {
                        options.push(serializable_syntax);
                    }
                }
            }
        }

        let subcommands = self.subcommands(called_as, unfold);

        // Create a serializable command help object
        let command_help = SerializableCommandHelp {
            name,
            usage,
            source,
            category,
            short_help,
            help,
            arguments,
            options,
            subcommands,
            tags: command.tags(),
        };

        // Serialize the command help to JSON
        let json =
            serde_json::to_string_pretty(&command_help).expect("failed to serialize help to JSON");
        println!("{}", json);
    }

    fn print_error(&self, error_type: &str, error_msg: &str) {
        let json = serde_json::json!({
            "error_type": error_type,
            "error_msg": error_msg,
        });

        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    }
}

impl HelpCommandJsonPrinter {
    fn new() -> Self {
        Self {}
    }

    fn subcommands(&self, prefix: Vec<String>, unfold: bool) -> Vec<SerializableSubcommand> {
        let command_loader = command_loader(".");
        let organizer = HelpCommandOrganizer::new_from_commands(command_loader.commands.clone());
        let commands = organizer.get_commands_with_fold(
            prefix.clone(),
            match unfold {
                true => 0,
                false => 1,
            },
        );

        if commands.is_empty() {
            return vec![];
        }

        let mut subcommands = vec![];

        let mut seen = HashSet::new();
        for cmd in commands.iter() {
            let command = &cmd.command;
            let name = command.name().join(" ");
            if !seen.insert(name.clone()) {
                continue;
            }

            let category = command.category().unwrap_or_default();
            let folded = cmd.num_folded();

            let all_names = command
                .all_names_with_prefix(prefix.clone())
                .iter()
                .map(|name| name.join(" "))
                .collect::<Vec<String>>()
                .join(", ");

            let subcommand = SerializableSubcommand {
                name: all_names,
                category,
                desc: strip_ansi_codes(&command.help_short()),
                folded,
            };

            subcommands.push(subcommand);
        }

        subcommands
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
        let mut inserted_paths = vec![path.clone()].into_iter().collect::<HashSet<_>>();

        for i in (1..=path.len()).rev() {
            let (cmdpath, _) = path.split_at(i);
            let cmd_sort_key = cmdpath.to_vec();
            let cat_sort_key = command.category_sort_key();
            let key = (cat_sort_key.0, cat_sort_key.1, cmd_sort_key.clone());

            // Check if that key already exists in the commands
            if let Some(cmd) = self.commands.get_mut(&key) {
                if !command_itself {
                    cmd.subcommands.extend(inserted_paths.clone());
                    continue;
                }

                match cmd.command {
                    Command::Void(_) => {
                        cmd.command = command.clone();
                        cmd.subcommands.extend(inserted_paths.clone());
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
                        command.category().unwrap_or_default(),
                    ))
                };

                let mut new_command = HelpCommandMetadata::new(&insert_command);
                new_command.subcommands.extend(inserted_paths.clone());
                self.commands.insert(key, new_command);

                if !command_itself {
                    inserted_paths.insert(cmdpath.to_vec());
                }
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
            metadata.folding = match max_before_fold {
                0 => false,
                _ => metadata.subcommands.len() > max_before_fold,
            };

            if metadata.folding {
                for subcommand in metadata.subcommands.iter() {
                    let sub_key = (key.0, key.1.clone(), subcommand.clone());
                    seen.insert(sub_key);
                }
            }

            let command_is_a_void = matches!(&metadata.command, Command::Void(_));

            if metadata.folding || !command_is_a_void {
                commands.push(metadata.clone());
            }
        }

        commands
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

/// Get the longest name under a specific threshold, takes as parameter an iterator of objects
/// of arbitrary type, a function to get the name of the object when receiving one of those objects
/// as parameter, and a threshold to consider for the length of the name.
/// The function returns a tuple with the longest name, and the longest name under the threshold.
fn get_longest_under_threshold_func<T>(
    iter: impl Iterator<Item = T>,
    name_func: impl Fn(&T) -> String,
    name_len_func: Option<impl Fn(&T, &str, &usize, &usize) -> usize>,
    wrap_threshold: usize,
) -> (usize, usize) {
    let mut longest_name = 0;
    let mut longest_under_threshold = 0;

    for item in iter {
        let name = name_func(&item);
        let name_wrapped = wrap_text(&name, wrap_threshold);
        let name_len = name_wrapped
            .iter()
            .enumerate()
            .map(|(idx, name)| {
                if let Some(name_len_func) = &name_len_func {
                    name_len_func(&item, name, &idx, &name_wrapped.len())
                } else {
                    name.len()
                }
            })
            .collect::<Vec<usize>>();

        let max_len = name_len.clone().into_iter().max().unwrap_or(0);
        let max_under_threshold = name_len
            .into_iter()
            .filter(|len| *len <= wrap_threshold)
            .max()
            .unwrap_or(0);

        longest_name = std::cmp::max(longest_name, max_len);
        longest_under_threshold = std::cmp::max(longest_under_threshold, max_under_threshold);
    }

    (longest_name, longest_under_threshold)
}

fn get_longest_command(
    commands: &[HelpCommandMetadata],
    prefix: &[String],
    wrap_threshold: usize,
) -> (usize, usize) {
    let mut longest_command = 0;
    let mut longest_under_threshold = 0;

    for cmd in commands.iter() {
        let command = &cmd.command;
        let all_names = command
            .all_names_with_prefix(prefix.to_vec())
            .iter()
            .map(|name| name.join(" "))
            .collect::<Vec<String>>();
        let all_names_str = all_names.join(", ");
        let all_names_wrapped = wrap_text(&all_names_str, wrap_threshold);
        let all_names_len = all_names_wrapped
            .iter()
            .enumerate()
            .map(|(idx, name)| {
                if idx == all_names_wrapped.len() - 1 {
                    name.len()
                        + match cmd.num_folded() {
                            0 => 0,
                            _ => 2,
                        }
                } else {
                    name.len()
                }
            })
            .collect::<Vec<usize>>();

        let max_len = all_names_len.clone().into_iter().max().unwrap_or(0);
        let max_under_threshold = all_names_len
            .into_iter()
            .filter(|len| *len <= wrap_threshold)
            .max()
            .unwrap_or(0);

        longest_command = std::cmp::max(longest_command, max_len);
        longest_under_threshold = std::cmp::max(longest_under_threshold, max_under_threshold);
    }

    (longest_command, longest_under_threshold)
}

fn get_command_length(command: &HelpCommandMetadata, prefix: &[String]) -> usize {
    let all_names = command
        .command
        .all_names_with_prefix(prefix.to_vec())
        .iter()
        .map(|name| name.join(" "))
        .collect::<Vec<String>>();
    let all_names_str = all_names.join(", ");
    let all_names_wrapped = wrap_text(&all_names_str, 40);
    let all_names_len = all_names_wrapped
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            if idx == all_names_wrapped.len() - 1 {
                name.len()
                    + match command.num_folded() {
                        0 => 0,
                        _ => 2,
                    }
            } else {
                name.len()
            }
        })
        .max()
        .unwrap_or(0);

    all_names_len
}
