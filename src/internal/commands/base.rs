use std::collections::BTreeMap;
use std::process::exit;

use crate::internal::commands::fromconfig::ConfigCommand;
use crate::internal::commands::frommakefile::MakefileCommand;
use crate::internal::commands::frompath::PathCommand;
use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::commands::void::VoidCommand;
use crate::internal::config::parser::ParseArgsErrorKind;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::user_interface::colors::strip_colors;
use crate::internal::user_interface::colors::strip_colors_if_needed;
use crate::internal::user_interface::term_width;
use crate::internal::user_interface::wrap_text;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir::is_trusted;
use crate::internal::workdir::is_trusted_or_ask;
use crate::omni_error;
use crate::omni_print;

pub trait BuiltinCommand: std::fmt::Debug + Send + Sync {
    fn new_command() -> Command
    where
        Self: Sized,
    {
        Command::Builtin(Self::new_boxed())
    }
    fn new_boxed() -> Box<dyn BuiltinCommand>
    where
        Self: Sized;
    fn clone_boxed(&self) -> Box<dyn BuiltinCommand>;
    fn name(&self) -> Vec<String>;
    fn aliases(&self) -> Vec<Vec<String>>;
    fn help(&self) -> Option<String>;
    fn syntax(&self) -> Option<CommandSyntax>;
    fn category(&self) -> Option<Vec<String>>;
    fn exec(&self, argv: Vec<String>);
    fn autocompletion(&self) -> bool;
    fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) -> Result<(), ()>;
}

#[derive(Debug)]
pub enum Command {
    // Take any BuiltinCommand that's also Debuggable
    Builtin(Box<dyn BuiltinCommand>),
    FromConfig(Box<ConfigCommand>),
    FromMakefile(MakefileCommand),
    FromPath(PathCommand),
    Void(VoidCommand),
}

impl Clone for Command {
    fn clone(&self) -> Self {
        match self {
            Command::Builtin(command) => Command::Builtin(command.clone_boxed()),
            Command::FromConfig(command) => Command::FromConfig(command.clone()),
            Command::FromMakefile(command) => Command::FromMakefile(command.clone()),
            Command::FromPath(command) => Command::FromPath(command.clone()),
            Command::Void(command) => Command::Void(command.clone()),
        }
    }
}

impl Command {
    pub fn name(&self) -> Vec<String> {
        match self {
            Command::Builtin(command) => command.name(),
            Command::FromPath(command) => command.name(),
            Command::FromConfig(command) => command.name(),
            Command::FromMakefile(command) => command.name(),
            Command::Void(command) => command.name(),
        }
    }

    pub fn flat_name(&self) -> String {
        self.name().join(" ")
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        match self {
            Command::Builtin(command) => command.aliases(),
            Command::FromPath(command) => command.aliases(),
            Command::FromConfig(command) => command.aliases(),
            Command::FromMakefile(command) => command.aliases(),
            Command::Void(command) => command.aliases(),
        }
    }

    pub fn all_names(&self) -> Vec<Vec<String>> {
        let mut names = vec![self.name()];
        names.extend(self.aliases());
        names
    }

    pub fn all_names_with_prefix(&self, prefix: Vec<String>) -> Vec<Vec<String>> {
        self.all_names()
            .iter()
            .filter(|name| name.starts_with(&prefix))
            .cloned()
            .map(|name| name[prefix.len()..].to_vec())
            .collect()
    }

    pub fn shadow_names(&self) -> Vec<Vec<String>> {
        match self {
            Command::FromConfig(command) => match command.orig_name() {
                Some(orig_name) => vec![orig_name],
                None => vec![],
            },
            Command::FromMakefile(command) => match command.orig_name() {
                Some(orig_name) => vec![vec![orig_name]],
                None => vec![],
            },
            _ => vec![],
        }
    }

    pub fn has_source(&self) -> bool {
        matches!(
            self,
            Command::FromPath(_) | Command::FromConfig(_) | Command::FromMakefile(_)
        )
    }

    pub fn source(&self) -> String {
        match self {
            Command::Builtin(_) => "builtin".to_string(),
            Command::FromPath(command) => command.source(),
            Command::FromConfig(command) => command.source(),
            Command::FromMakefile(command) => command.source(),
            Command::Void(_) => "auto-generated".to_string(),
        }
    }

    pub fn source_dir(&self) -> String {
        let source = self.source();
        match source.as_str() {
            "builtin" => ".".to_string(),
            _ => {
                // get the canonical path for source
                let source = match std::fs::canonicalize(&source) {
                    Ok(path) => path,
                    Err(_) => return ".".to_string(),
                };

                // Return the parent directory
                match source.parent() {
                    Some(path) => path.to_str().unwrap().to_string(),
                    None => ".".to_string(),
                }
            }
        }
    }

    pub fn exec_dir(&self) -> String {
        match self {
            Command::FromConfig(command) => command
                .exec_dir()
                .map_err(|err| {
                    omni_error!(err.to_string());
                    exit(1);
                })
                .expect("Failed to get exec dir")
                .to_string_lossy()
                .to_string(),
            _ => self.source_dir(),
        }
    }

    pub fn help_source(&self) -> String {
        let source = self.source();
        if !source.starts_with('/') {
            return source;
        }

        let path = abs_or_rel_path(&source);
        match self {
            Command::FromMakefile(command) => format!("{}:{}", path, command.lineno()),
            _ => path,
        }
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        match self {
            Command::Builtin(command) => command.syntax(),
            Command::FromPath(command) => command.syntax(),
            Command::FromConfig(command) => command.syntax(),
            Command::FromMakefile(command) => command.syntax(),
            Command::Void(command) => command.syntax(),
        }
    }

    pub fn category(&self) -> Option<Vec<String>> {
        match self {
            Command::Builtin(command) => command.category(),
            Command::FromPath(command) => command.category(),
            Command::FromConfig(command) => command.category(),
            Command::FromMakefile(command) => command.category(),
            Command::Void(command) => command.category(),
        }
    }

    pub fn help(&self) -> String {
        let help: Option<String> = match self {
            Command::Builtin(command) => command.help(),
            Command::FromPath(command) => command.help(),
            Command::FromConfig(command) => command.help(),
            Command::FromMakefile(command) => command.help(),
            Command::Void(command) => command.help(),
        };

        if let Some(help) = help {
            help
        } else {
            "".to_string()
        }
    }

    pub fn help_short(&self) -> String {
        self.help().split("\n\n").next().unwrap_or("").to_string()
    }

    pub fn usage(&self, called_as: Option<String>) -> String {
        let name = if let Some(called_as) = called_as {
            if self
                .all_names()
                .iter()
                .any(|name| name.join(" ") == called_as)
            {
                called_as
            } else {
                self.name().join(" ")
            }
        } else {
            self.name().join(" ")
        };
        let mut usage = format!("omni {}", name).bold();

        if let Some(syntax) = self.syntax() {
            if let Some(syntax_usage) = syntax.usage {
                usage += &format!(" {}", syntax_usage);
            } else if !syntax.parameters.is_empty() {
                let params = syntax.parameters.clone();

                // Take all options, i.e. non-positional that are not required
                let (options, params): (Vec<_>, Vec<_>) = params
                    .into_iter()
                    .partition(|param| !param.required && !param.is_positional());
                if !options.is_empty() {
                    usage += &" [OPTIONS]".cyan();
                }

                // Take all non-positional that are required
                let (required_options, params): (Vec<_>, Vec<_>) = params
                    .into_iter()
                    .partition(|param| param.required && !param.is_positional());
                for param in required_options {
                    let param_usage = format!(" {}", param.usage());
                    usage += &param_usage;
                }

                // Finally, we're only left with positional parameters
                for param in params {
                    let param_usage = format!(" {}", param.usage());
                    usage += &param_usage;
                }
            }
        }

        usage
    }

    pub fn commands_serve(&self, argv: &[String], command_names: Vec<Vec<String>>) -> usize {
        let argv = argv.to_vec();

        let mut max_match: usize = 0;
        'outer: for alias in command_names {
            if argv.len() < alias.len() {
                continue;
            }

            let check = alias.clone();
            let mut argv = argv.clone();

            for part in &check {
                if argv[0] != *part {
                    continue 'outer;
                }

                argv.remove(0);
            }

            max_match = alias.len()
        }

        max_match
    }

    pub fn serves(&self, argv: &[String]) -> usize {
        self.commands_serve(argv, self.all_names())
    }

    pub fn shadow_serves(&self, argv: &[String]) -> usize {
        self.commands_serve(argv, self.shadow_names())
    }

    pub fn is_subcommand_of(&self, argv: &[String]) -> bool {
        for alias in self.all_names() {
            if argv.len() > alias.len() {
                continue;
            }

            if alias.starts_with(argv) {
                return true;
            }
        }

        false
    }

    pub fn exec_parse_args(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> Option<BTreeMap<String, String>> {
        let should_parse_args = match self {
            Command::FromConfig(command) => command.argparser(),
            Command::FromPath(command) => command.argparser(),
            _ => false,
        };

        if !should_parse_args {
            return None;
        }

        let syntax = self.syntax().unwrap_or_default();
        let parsed_args = self
            .exec_parse_args_error_handling(syntax.parse_args(argv, called_as.clone()), called_as);
        Some(parsed_args)
    }

    pub fn exec_parse_args_typed(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> Option<BTreeMap<String, ParseArgsValue>> {
        if !matches!(self, Command::Builtin(_)) {
            return None;
        }

        let syntax = self.syntax().unwrap_or_default();
        let parsed_args = self.exec_parse_args_error_handling(
            syntax.parse_args_typed(argv, called_as.clone()),
            called_as,
        );

        Some(parsed_args)
    }

    fn exec_parse_args_error_handling<V>(
        &self,
        result: Result<BTreeMap<String, V>, ParseArgsErrorKind>,
        called_as: Vec<String>,
    ) -> BTreeMap<String, V> {
        match result {
            Ok(parsed_args) => parsed_args,
            Err(ParseArgsErrorKind::ParserBuildError(err)) => {
                omni_print!(format!("{} {}", "error building parser:".red(), err));
                exit(1);
            }
            Err(ParseArgsErrorKind::ArgumentParsingError(err)) => {
                let clap_rich_error = err.render().ansi().to_string();
                let clap_rich_error = strip_colors_if_needed(&clap_rich_error);
                let parts = clap_rich_error.trim().split('\n');

                for (idx, line) in parts.enumerate() {
                    let line_wo_colors = strip_colors(line);
                    if line_wo_colors.starts_with("Usage: ") {
                        // Print our own usage formatting
                        let max_width = term_width() - 4;
                        let command_usage = self.usage(Some(called_as.join(" ")));
                        let wrapped_usage = wrap_text(&command_usage, max_width - 7); // 7 is the length of "Usage: "

                        eprintln!("{} {}", "Usage:".underline().bold(), wrapped_usage[0]);
                        wrapped_usage.iter().skip(1).for_each(|line| {
                            eprintln!("       {}", line);
                        });
                        continue;
                    }
                    if idx > 0 {
                        eprintln!("{}", line);
                    } else {
                        omni_print!(format!(
                            "{} {}",
                            format!("{}:", called_as.join(" ")).light_yellow(),
                            line
                        ));
                    }
                }
                exit(1);
            }
        }
    }

    pub fn exec(&self, argv: Vec<String>, called_as: Option<Vec<String>>) {
        // Load the dynamic environment for that command
        update_dynamic_env_for_command(self.exec_dir());

        // Set the general execution environment
        let called_as = match called_as {
            Some(called_as) => called_as,
            None => self.name().clone(),
        };
        let name = called_as.join(" ");
        std::env::set_var("OMNI_SUBCOMMAND", name.clone());

        // Add the omni version to the environment
        std::env::set_var("OMNI_VERSION", env!("CARGO_PKG_VERSION"));

        // Clear all `OMNI_ARG_` environment variables
        for (key, _) in std::env::vars() {
            if key.starts_with("OMNI_ARG_") {
                std::env::remove_var(&key);
            }
        }

        // Set environment variables for the parsed arguments, if we are parsing any
        if let Some(args) = self.exec_parse_args(argv.clone(), called_as.clone()) {
            for (key, value) in args {
                std::env::set_var(key, value);
            }
        }

        match self {
            Command::FromConfig(cmd) if cmd.is_trusted() => {
                // If the configuration command is not provided by a workdir,
                // we can trust it right away
            }
            Command::FromPath(_) | Command::FromConfig(_) | Command::FromMakefile(_) => {
                // Check if the workdir where the command is located is trusted
                if !is_trusted_or_ask(
                    &self.source_dir(),
                    format!(
                        "Do you want to run {} provided by this directory?",
                        format!("omni {}", name).light_yellow(),
                    ),
                ) {
                    omni_error!(format!(
                        "skipping running command as directory is not trusted."
                    ));
                    exit(1);
                }
            }
            _ => {}
        }

        match self {
            Command::Builtin(command) => command.exec(argv),
            Command::FromPath(command) => command.exec(argv, Some(called_as)),
            Command::FromConfig(command) => command.exec(argv),
            Command::FromMakefile(command) => command.exec(argv),
            Command::Void(_) => {}
        }
        panic!("Command::exec() not implemented");
    }

    pub fn autocompletion(&self) -> bool {
        match self {
            Command::Builtin(command) => command.autocompletion(),
            Command::FromPath(command) => command.autocompletion(),
            Command::FromConfig(_command) => false,
            Command::FromMakefile(_command) => false,
            Command::Void(_) => false,
        }
    }

    pub fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) -> Result<(), ()> {
        match self {
            Command::FromPath(_) | Command::FromConfig(_) | Command::FromMakefile(_) => {
                // Check if the workdir where the command is located is trusted
                if !is_trusted(self.source_dir()) {
                    return Err(());
                }
            }
            _ => {}
        }

        match self {
            Command::Builtin(command) => return command.autocomplete(comp_cword, argv),
            Command::FromPath(command) => {
                // Load the dynamic environment for that command
                update_dynamic_env_for_command(self.source_dir());

                let result = command.autocomplete(comp_cword, argv);

                // Reset the dynamic environment
                update_dynamic_env_for_command(".");

                return result;
            }
            Command::FromConfig(_command) => {}
            Command::FromMakefile(_command) => {}
            Command::Void(_) => {}
        }

        Err(())
    }

    fn command_type_sort_order(&self) -> usize {
        match self {
            Command::FromConfig(_) => 1,
            Command::FromMakefile(_) => 2,
            Command::Void(command) => command.type_sort_order(),
            _ => match self.category() {
                Some(_) => 0,
                None => usize::MAX,
            },
        }
    }

    pub fn category_sort_key(&self) -> (usize, Vec<String>) {
        (
            self.command_type_sort_order(),
            self.category().clone().unwrap_or_default(),
        )
    }

    pub fn requires_sync_update(&self) -> bool {
        match self {
            Command::FromPath(command) => command.requires_sync_update(),
            _ => false,
        }
    }
}
