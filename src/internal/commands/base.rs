use std::collections::BTreeMap;
use std::process::exit;

use itertools::Itertools;

use crate::internal::commands::fromconfig::ConfigCommand;
use crate::internal::commands::frommakefile::MakefileCommand;
use crate::internal::commands::frompath::PathCommand;
use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::commands::utils::path_auto_complete;
use crate::internal::commands::void::VoidCommand;
use crate::internal::config::parser::ParseArgsErrorKind;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxGroup;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
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
    fn autocompletion(&self) -> CommandAutocompletion {
        CommandAutocompletion::Null
    }
    fn autocomplete(
        &self,
        _comp_cword: usize,
        _argv: Vec<String>,
        _parameter: Option<(String, usize)>, // parameter name, start index
    ) -> Result<(), ()> {
        Err(())
    }
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

    pub fn tags(&self) -> BTreeMap<String, String> {
        match self {
            Command::FromPath(command) => command.tags(),
            Command::FromConfig(command) => command.tags().clone(),
            _ => BTreeMap::new(),
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
                    if param.is_last() {
                        usage += " --";
                    }

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
        if !self.argparser() {
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
            Err(ParseArgsErrorKind::InvalidValue(err)) => {
                omni_print!(format!("{} {}", "error parsing arguments:".red(), err));
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

    pub fn argparser(&self) -> bool {
        match self {
            Command::FromConfig(command) => command.argparser(),
            Command::FromPath(command) => command.argparser(),
            _ => false,
        }
    }

    pub fn autocompletion(&self) -> CommandAutocompletion {
        let completion = match self {
            Command::Builtin(command) => command.autocompletion(),
            Command::FromPath(command) => command.autocompletion(),
            Command::FromConfig(_command) => CommandAutocompletion::Null,
            Command::FromMakefile(_command) => CommandAutocompletion::Null,
            Command::Void(_) => CommandAutocompletion::Null,
        };

        let trusted = match self {
            Command::FromPath(_) | Command::FromConfig(_) | Command::FromMakefile(_) => {
                // Check if the workdir where the command is located is trusted
                is_trusted(self.source_dir())
            }
            _ => true,
        };

        let argparser = self.argparser() || matches!(self, Command::Builtin(_));

        match (completion, trusted, argparser) {
            (CommandAutocompletion::Full, true, _) => CommandAutocompletion::Full,
            (CommandAutocompletion::Partial, true, true) => CommandAutocompletion::Partial,
            (CommandAutocompletion::Partial, true, false) => CommandAutocompletion::Null,
            (CommandAutocompletion::Null, _, true) => CommandAutocompletion::Argparser,
            (CommandAutocompletion::Null, _, false) => CommandAutocompletion::Null,
            (_, false, true) => CommandAutocompletion::Argparser,
            (_, false, false) => CommandAutocompletion::Null,
            _ => CommandAutocompletion::Null,
        }
    }

    /// Handle the autocompletion for the command
    ///
    /// This function is called when the command is being autocompleted. It handles
    /// the autocompletion for the command and returns the result. If the command
    /// has full autocompletion, this will be delegated directly to the command if
    /// and only if that command is trusted. If that command has partial completion,
    /// it will only be delegated to the command if and only if that command is
    /// trusted _AND_ the argparser autocompletion did not provide any completion.
    /// If the command has no autocompletion, then only the argparser autocompletion
    /// will be used. If the command has neither autocompletion nor argparser,
    /// then no autocompletion will be provided.
    pub fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) -> Result<(), ()> {
        let completion = self.autocompletion();

        // If no autocompletion option is available, just return an error
        if !completion.any() {
            return Err(());
        }

        // Only try to autocomplete with the argparser if the completion is either
        // disabled or partial, or if the command is not trusted
        let mut parameter = None;
        if completion.use_argparser() {
            match self.autocomplete_with_argparser(comp_cword, &argv) {
                Ok(None) => return Ok(()),
                Ok(Some((param_name, param_idx))) => {
                    // Continue with the autocompletion if partial
                    parameter = Some((param_name, param_idx));
                }
                Err(()) => return Err(()),
            }
        }

        // If we get here, try to do a completion with the command if
        // it is trusted and has autocompletion
        if completion.use_command() {
            match self {
                Command::Builtin(command) => {
                    return command.autocomplete(comp_cword, argv, parameter)
                }
                Command::FromPath(command) => {
                    // Load the dynamic environment for that command
                    update_dynamic_env_for_command(self.source_dir());

                    let result = command.autocomplete(comp_cword, argv, parameter);

                    // Reset the dynamic environment
                    update_dynamic_env_for_command(".");

                    return result;
                }
                Command::FromConfig(_command) => {}
                Command::FromMakefile(_command) => {}
                Command::Void(_) => {}
            }
        }

        Err(())
    }

    /// Handle the autocompletion for the command using the argparser
    ///
    /// This will automatically suggest completions based on the argparser
    /// of the command. This function is called when the command is being
    /// autocompleted and the command has an argparser, unless the command
    /// handles the full autocompletion by itself.
    ///
    /// The argparser autocompletion will suggest flags and options, but
    /// also can suggest values for certain specific parameter types (e.g.
    /// if the parameter value is supposed to be a path, it will suggest
    /// paths available in the filesystem).
    fn autocomplete_with_argparser(
        &self,
        comp_cword: usize,
        argv: &[String],
    ) -> Result<Option<(String, usize)>, ()> {
        let syntax = match self.syntax() {
            Some(syntax) => syntax,
            None => return Err(()),
        };

        fn allow_value_check_hyphen(param: &SyntaxOptArg, value: &str) -> bool {
            if let Some(value_without_hyphen) = value.strip_prefix('-') {
                if !param.allow_hyphen_values {
                    // All good if we allow for a parameter to start with `-`
                    return false;
                }

                if !param.allow_negative_numbers || value_without_hyphen.parse::<f64>().is_err() {
                    // All good if we allow for negative numbers
                    return false;
                }
            }

            true
        }

        let mut state = None;
        let mut parameters = syntax.parameters.clone();
        let last_parameter = syntax
            .parameters
            .iter()
            .find(|param| param.is_last())
            .cloned();

        // Go over the arguments we've seen until `comp_cword` and
        // try to resolve the parameter in the syntax and the number
        // of values that have been passed (according to the configuration
        // of the parameter); if a given argument does not have a value,
        // consider it is a positional (if any).
        let args = argv.iter().take(comp_cword).cloned().collect::<Vec<_>>();
        let mut current_arg = args.first().cloned();
        let mut current_idx = 0;

        'loop_args: while let Some(arg) = current_arg {
            current_arg = None;

            let (parameter, mut next_arg) = if arg == "--" {
                // If we have `--`, find the parameter with the 'last' flag, if any
                match last_parameter {
                    Some(ref parameter) => {
                        state = Some(ArgparserAutocompleteState::Value {
                            param: parameter.clone(),
                            param_idx: current_idx + 1,
                        });
                    }
                    None => {
                        // We do not need to autocomplete anything if we are
                        // after the `--` and there is no parameter with the
                        // 'last' flag
                        return Ok(None);
                    }
                }

                // No need to keep reading parameters
                break;
            } else if arg == "-" {
                // If we have `-` we just can't complete any parameter, let's just skip
                (None, None)
            } else if arg.starts_with("--") {
                let parameter = syntax
                    .parameters
                    .iter()
                    .find(|param| param.all_names().iter().any(|name| name == arg.as_str()));

                (parameter, None)
            } else if let Some(arg_name) = arg.strip_prefix('-') {
                // Split the first char and the following ones, if any
                // as they would become the next argument
                let (arg, next_arg) = arg_name.split_at(1);
                let arg = format!("-{}", arg);

                let next_arg = if next_arg.is_empty() {
                    None
                } else {
                    Some(next_arg.to_string())
                };

                let parameter = syntax
                    .parameters
                    .iter()
                    .find(|param| param.all_names().iter().any(|name| name == arg.as_str()));

                (parameter, next_arg)
            } else {
                // Get the parameters from the list of parameters left, since for positional
                // we need to remove them from the list as we can't identify them by name alone
                match syntax.parameters.iter().find(|param| {
                    param.is_positional() && !param.is_last() && parameters.contains(param)
                }) {
                    Some(parameter) => {
                        // We need to pass the parameter itself as "next_arg" since for a
                        // positional, the parameter itself is the value
                        (Some(parameter), Some(arg))
                    }
                    None => {
                        // If we don't have any positional parameters left, then we can't
                        // autocomplete this, just skip it
                        (None, None)
                    }
                }
            };

            // If the parameter is not found, skip to the next
            let parameter = match parameter {
                Some(parameter) => parameter,
                None => {
                    current_idx += 1;
                    current_arg = args.get(current_idx).cloned();
                    continue;
                }
            };

            // If the parameter is not repeatable, remove it from the list
            // TODO: how does that work for positionals?
            if !parameter.is_repeatable() {
                parameters.retain(|param| param != parameter);
            }

            // Handle the conflicts between parameters
            parameters.retain(|param| !check_parameter_conflicts(parameter, param, &syntax.groups));

            // Consume values as needed
            if parameter.takes_value() {
                // How many values to consume at most?
                let max_values: Option<usize> = parameter
                    .num_values
                    .map_or(if parameter.leftovers { None } else { Some(1) }, |num| {
                        num.max()
                    });
                let min_values = parameter.num_values.map_or(1, |num| num.min().unwrap_or(0));

                let param_start_idx = current_idx + if parameter.is_positional() { 0 } else { 1 };
                let mut value_idx = 0;
                loop {
                    if let Some(max) = max_values {
                        if value_idx >= max {
                            // Stop here if we have the maximum number of values
                            break;
                        }
                    }

                    value_idx += 1;
                    let value = if let Some(arg) = next_arg {
                        next_arg = None;
                        Some(arg)
                    } else {
                        current_idx += 1;
                        args.get(current_idx).cloned()
                    };

                    if let Some(ref value) = value {
                        if !allow_value_check_hyphen(parameter, value) {
                            // If the value is not allowed, then consider it
                            // is another argument, so exit this loop
                            current_arg = Some(value.to_string());
                            break;
                        }
                    }

                    if current_idx == comp_cword || value.is_none() {
                        state = Some(if value_idx > min_values && !parameter.leftovers {
                            ArgparserAutocompleteState::ValueAndParameters {
                                param: parameter.clone(),
                                param_idx: param_start_idx,
                            }
                        } else {
                            ArgparserAutocompleteState::Value {
                                param: parameter.clone(),
                                param_idx: param_start_idx,
                            }
                        });
                        break 'loop_args;
                    }
                }
            } else if let Some(next_arg) = next_arg {
                current_arg = Some(format!("-{}", next_arg));
            }

            if current_arg.is_none() {
                current_idx += 1;
                current_arg = args.get(current_idx).cloned();
            }
        }

        let state = state.unwrap_or(
            match (
                parameters
                    .iter()
                    .find(|param| param.is_positional() && !param.is_last()),
                argv.get(comp_cword),
            ) {
                (Some(param), Some(value)) if allow_value_check_hyphen(param, value) => {
                    ArgparserAutocompleteState::ValueAndParameters {
                        param: param.clone(),
                        param_idx: current_idx,
                    }
                }
                (Some(param), None) => ArgparserAutocompleteState::ValueAndParameters {
                    param: param.clone(),
                    param_idx: current_idx,
                },
                (_, _) => ArgparserAutocompleteState::Parameters,
            },
        );

        // Grab the value to be completed, or default to the empty string
        let comp_value = argv.get(comp_cword).cloned().unwrap_or_default();

        if state.complete_parameters() {
            // If we get here, go over the parameters still in the list, filter
            // them using the value to be completed, and return their names
            parameters
                .iter()
                .filter(|param| !param.is_positional())
                .flat_map(|param| param.all_names())
                .filter(|name| name.starts_with(&comp_value))
                .sorted()
                .for_each(|name| {
                    println!("{}", name);
                });

            // Autocomplete '--' if there is a last parameter
            if last_parameter.is_some() && "--".starts_with(&comp_value) {
                println!("--");
            }
        }

        if let Some((param, param_idx)) = state.parameter() {
            let arg_type = param.arg_type().terminal_type().clone();

            if let Some(possible_values) = arg_type.possible_values() {
                possible_values
                    .iter()
                    .filter(|val| val.starts_with(&comp_value))
                    .for_each(|val| {
                        println!("{}", val);
                    });

                // We've done the whole completion for that parameter, no
                // need to delegate to the underlying command
                return Ok(None);
            }

            if matches!(
                arg_type,
                SyntaxOptArgType::DirPath | SyntaxOptArgType::FilePath | SyntaxOptArgType::RepoPath
            ) {
                let include_repositories = matches!(arg_type, SyntaxOptArgType::RepoPath);
                let include_files = matches!(arg_type, SyntaxOptArgType::FilePath);

                path_auto_complete(&comp_value, include_repositories, include_files)
                    .iter()
                    .for_each(|s| println!("{}", s));

                // We offered path autocompletions, no need to delegate
                // to the underlying command
                return Ok(None);
            }

            Ok(Some((param.name(), param_idx.to_owned())))
        } else {
            Ok(None)
        }
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

#[inline]
fn check_parameter_conflicts(
    param1: &SyntaxOptArg,
    param2: &SyntaxOptArg,
    groups: &[SyntaxGroup],
) -> bool {
    // Get the groups for param1
    let param1_groups = groups
        .iter()
        .filter(|group| {
            group.parameters.iter().any(|name| {
                param1
                    .all_names()
                    .iter()
                    .any(|n| n.as_str() == name.as_str())
            })
        })
        .collect::<Vec<_>>();
    let param1_all = param1
        .all_names()
        .into_iter()
        .chain(param1_groups.iter().map(|group| group.name.to_string()))
        .collect::<Vec<_>>();

    // Get the groups for param2
    let param2_groups = groups
        .iter()
        .filter(|group| {
            group.parameters.iter().any(|name| {
                param2
                    .all_names()
                    .iter()
                    .any(|n| n.as_str() == name.as_str())
            })
        })
        .collect::<Vec<_>>();
    let param2_all = param2
        .all_names()
        .into_iter()
        .chain(param2_groups.iter().map(|group| group.name.to_string()))
        .collect::<Vec<_>>();

    // If param1 defines conflicts with param2 or any of its groups, return true
    if param1
        .conflicts_with
        .iter()
        .any(|name| param2_all.iter().any(|n| n == name))
    {
        return true;
    }

    // If param2 defines conflicts with param1 or any of its groups, return true
    if param2
        .conflicts_with
        .iter()
        .any(|name| param1_all.iter().any(|n| n == name))
    {
        return true;
    }

    // If param1 and param2 are in the same group, and that group does
    // not allow multiple, return true
    let common_groups = param1_groups
        .iter()
        .filter(|group| param2_groups.contains(group))
        .collect::<Vec<_>>();
    if common_groups.iter().any(|group| !group.multiple) {
        return true;
    }

    // Check if any of the param1 groups conflicts with param2
    if param1_groups.iter().any(|group| {
        group
            .conflicts_with
            .iter()
            .any(|name| param2_all.iter().any(|n| n == name))
    }) {
        return true;
    }

    // Check if any of the param2 groups conflicts with param1
    if param2_groups.iter().any(|group| {
        group
            .conflicts_with
            .iter()
            .any(|name| param1_all.iter().any(|n| n == name))
    }) {
        return true;
    }

    // If we get here, there is no conflict
    false
}

#[derive(Debug)]
enum ArgparserAutocompleteState {
    Parameters,
    ValueAndParameters {
        param: SyntaxOptArg,
        param_idx: usize,
    },
    Value {
        param: SyntaxOptArg,
        param_idx: usize,
    },
}

impl ArgparserAutocompleteState {
    fn complete_parameters(&self) -> bool {
        matches!(self, Self::Parameters | Self::ValueAndParameters { .. })
    }

    fn parameter(&self) -> Option<(SyntaxOptArg, usize)> {
        match self {
            Self::Value { param, param_idx } | Self::ValueAndParameters { param, param_idx } => {
                Some((param.clone(), *param_idx))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandAutocompletion {
    #[default]
    Null,
    Partial,
    Full,
    Argparser,
}

impl CommandAutocompletion {
    fn any(&self) -> bool {
        !matches!(self, Self::Null)
    }

    fn use_argparser(&self) -> bool {
        matches!(self, Self::Argparser | Self::Partial)
    }

    fn use_command(&self) -> bool {
        matches!(self, Self::Full | Self::Partial)
    }
}

impl serde::Serialize for CommandAutocompletion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Null => serializer.serialize_bool(false),
            Self::Partial => serializer.serialize_str("partial"),
            Self::Full => serializer.serialize_bool(true),
            Self::Argparser => {
                unreachable!("Argparser autocompletion is not serializable")
            }
        }
    }
}

impl From<CommandAutocompletion> for bool {
    fn from(value: CommandAutocompletion) -> Self {
        match value {
            CommandAutocompletion::Full
            | CommandAutocompletion::Partial
            | CommandAutocompletion::Argparser => true,
            CommandAutocompletion::Null => false,
        }
    }
}

impl From<bool> for CommandAutocompletion {
    fn from(value: bool) -> Self {
        if value {
            Self::Full
        } else {
            Self::Null
        }
    }
}

impl From<String> for CommandAutocompletion {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&str> for CommandAutocompletion {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "full" | "true" | "1" | "on" | "enable" | "enabled" => Self::Full,
            "partial" => Self::Partial,
            _ => Self::Null,
        }
    }
}
