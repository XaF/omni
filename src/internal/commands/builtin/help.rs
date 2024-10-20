use std::collections::BTreeMap;
use std::collections::HashSet;
use std::process::exit;

use once_cell::sync::OnceCell;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::command_loader;
use crate::internal::commands::void::VoidCommand;
use crate::internal::commands::Command;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
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
                clap::Arg::new("unfold")
                    .long("unfold")
                    .short('u')
                    .action(clap::ArgAction::SetTrue),
            )
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

        Self {
            unfold: *matches.get_one::<bool>("unfold").unwrap_or(&false),
            unparsed,
        }
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

    fn help_global(&self) {
        eprintln!(
            "{}\n\n{} {} {} {} {}",
            omni_header!(),
            "Usage:".italic().bold(),
            "omni".bold(),
            "<command>".cyan().bold(),
            "[options]".cyan().bold(),
            "ARG...".cyan().bold(),
        );

        self.print_categorized_command_help(vec![]);
        eprintln!();
    }

    fn help_command(&self, command: &Command, called_as: Vec<String>) {
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

            // if !syntax.parameters.is_empty() {
            // let longest = syntax
            // .parameters
            // .iter()
            // .map(|arg| arg.name.len())
            // .max()
            // .unwrap_or(0);
            // let ljust = std::cmp::max(longest + 2, 15);
            // let join_str = format!("\n  {}", " ".repeat(ljust));

            // for arg in syntax.parameters.iter() {
            // let missing_just = ljust - arg.name.len();
            // let str_name = format!("  {}{}", arg.name.cyan(), " ".repeat(missing_just));
            // let help = if let Some(desc) = &arg.desc {
            // wrap_text(&strip_colors_if_needed(desc), max_width - ljust)
            // .join(join_str.as_str())
            // } else {
            // "".to_string()
            // };
            // eprintln!("\n{}{}", str_name, help);
            // }
            // }
        }

        self.print_categorized_command_help(called_as);

        eprintln!(
            "\n{} {}",
            "Source:".light_black(),
            command.help_source().underline()
        );
    }

    fn help_void(&self, called_as: Vec<String>) {
        self.help_command(
            &Command::Void(VoidCommand::new_for_help(called_as.clone())),
            called_as,
        );
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

                let first_desc_line = help_desc.first().unwrap_or(&empty_str);
                let last_name_line = wrapped_name_and_len
                    .last()
                    .expect("Name should not be empty");
                buf.push_str(&format!(
                    "  {}{}{}\n",
                    last_name_line.0,
                    " ".repeat(ljust - last_name_line.1),
                    first_desc_line,
                ));

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

        // let all_names_str = format!("{}{}", all_names_str, "-".repeat(num_folded_len));
        // let all_names_vec = wrap_text(&all_names_str, wrap_threshold);
        // let all_names_and_len = all_names_vec
        // .iter()
        // .enumerate()
        // .map(|(idx, name)| {
        // let namelen = strip_ansi_codes(name).len();
        // let name = if idx == all_names_vec.len() - 1 {
        // match name.strip_suffix("--") {
        // Some(name) => format!("{}{}", name, num_folded),
        // None => name.to_string(),
        // }
        // } else {
        // name.to_string()
        // };
        // (name, namelen)
        // })
        // .collect::<Vec<(String, usize)>>();

        // // Prepare the help contents
        // let help_vec = wrap_text(&strip_colors_if_needed(command.help_short()), help_just);

        // // Prepare the help message to print for this command
        // let empty_str = "".to_string();
        // let mut buf = String::new();
        // if print_name_on_same_line {
        // all_names_and_len
        // .iter()
        // .take(all_names_and_len.len() - 1)
        // .for_each(|(name, _)| {
        // buf.push_str(&format!("  {}\n", name));
        // });

        // let first_desc_line = help_vec.first().unwrap_or(&empty_str);
        // let last_name_line = all_names_and_len.last().expect("Name should not be empty");
        // buf.push_str(&format!(
        // "  {}{}{}\n",
        // last_name_line.0,
        // " ".repeat(ljust - last_name_line.1),
        // first_desc_line,
        // ));

        // help_vec.iter().skip(1).for_each(|line| {
        // buf.push_str(&format!("{}{}\n", " ".repeat(ljust + 2), line));
        // });
        // } else {
        // all_names_and_len.iter().for_each(|(name, _)| {
        // buf.push_str(&format!("  {}\n", name));
        // });

        // help_vec.iter().for_each(|line| {
        // buf.push_str(&format!("{}{}\n", " ".repeat(ljust + 2), line));
        // });
        // }

        // eprint!("{}", buf);
        // }

        Ok(())
    }

    fn print_categorized_command_help(&self, prefix: Vec<String>) -> bool {
        let command_loader = command_loader(".");
        let organizer = HelpCommandOrganizer::new_from_commands(command_loader.commands.clone());
        let commands = organizer.get_commands_with_fold(
            prefix.clone(),
            match self.cli_args().unfold {
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
                    name: "--unfold".to_string(),
                    desc: Some("Show all subcommands".to_string()),
                    required: false,
                    ..Default::default()
                },
                SyntaxOptArg {
                    name: "command".to_string(),
                    desc: Some("The command to get help for".to_string()),
                    required: false,
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
        if self.cli_args.set(HelpCommandArgs::parse(argv)).is_err() {
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

        omni_print!(format!("{} {}", "command not found:".red(), argv.join(" ")));
        exit(1);
    }

    fn autocompletion(&self) -> bool {
        true
    }

    fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) -> Result<(), ()> {
        command_loader(".").complete(comp_cword, argv, false)
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
