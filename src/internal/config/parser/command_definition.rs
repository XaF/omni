use std::any::Any;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::process::exit;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils as cache_utils;
use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::utils::str_to_bool;
use crate::internal::commands::HelpCommand;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;
use crate::omni_print;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandDefinition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    pub run: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax: Option<CommandSyntax>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcommands: Option<HashMap<String, CommandDefinition>>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub argparser: bool,
    #[serde(skip)]
    pub source: ConfigSource,
    #[serde(skip)]
    pub scope: ConfigScope,
}

impl CommandDefinition {
    pub(super) fn from_config_value(config_value: &ConfigValue) -> Self {
        let syntax = match config_value.get("syntax") {
            Some(value) => CommandSyntax::from_config_value(&value),
            None => None,
        };

        let category = match config_value.get("category") {
            Some(value) => {
                let mut category = Vec::new();
                if value.is_array() {
                    for value in value.as_array().unwrap() {
                        category.push(value.as_str().unwrap().to_string());
                    }
                } else {
                    category.push(value.as_str().unwrap().to_string());
                }
                Some(category)
            }
            None => None,
        };

        let subcommands = match config_value.get("subcommands") {
            Some(value) => {
                let mut subcommands = HashMap::new();
                for (key, value) in value.as_table().unwrap() {
                    subcommands.insert(
                        key.to_string(),
                        CommandDefinition::from_config_value(&value),
                    );
                }
                Some(subcommands)
            }
            None => None,
        };

        let aliases = match config_value.get_as_array("aliases") {
            Some(value) => value
                .iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect(),
            None => vec![],
        };

        let argparser = config_value
            .get_as_bool_forced("argparser")
            .unwrap_or(false);

        Self {
            desc: config_value
                .get("desc")
                .map(|value| value.as_str().unwrap().to_string()),
            run: config_value
                .get_as_str("run")
                .unwrap_or("true".to_string())
                .to_string(),
            aliases,
            syntax,
            category,
            dir: config_value
                .get_as_str("dir")
                .map(|value| value.to_string()),
            subcommands,
            argparser,
            source: config_value.get_source().clone(),
            scope: config_value.current_scope().clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CommandSyntax {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SyntaxOptArg>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<SyntaxGroup>,
}

impl Default for CommandSyntax {
    fn default() -> Self {
        Self {
            usage: None,
            parameters: vec![],
            groups: vec![],
        }
    }
}

impl CommandSyntax {
    const RESERVED_NAMES: [&'static str; 2] = ["-h", "--help"];

    pub fn new() -> Self {
        Self::default()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        let config_value = ConfigValue::from_value(ConfigSource::Null, ConfigScope::Null, value);
        if let Some(command_syntax) = CommandSyntax::from_config_value(&config_value) {
            Ok(command_syntax)
        } else {
            Err(serde::de::Error::custom("invalid command syntax"))
        }
    }

    pub(super) fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
        let mut usage = None;
        let mut parameters = vec![];
        let mut groups = vec![];

        if let Some(array) = config_value.as_array() {
            parameters.extend(
                array
                    .iter()
                    .filter_map(|value| SyntaxOptArg::from_config_value(value, None)),
            );
        } else if let Some(table) = config_value.as_table() {
            let keys = [
                ("parameters", None),
                ("arguments", Some(true)),
                ("argument", Some(true)),
                ("options", Some(false)),
                ("option", Some(false)),
                ("optional", Some(false)),
            ];

            for (key, required) in keys {
                if let Some(value) = table.get(key) {
                    if let Some(value) = value.as_array() {
                        let arguments = value
                            .iter()
                            .filter_map(|value| SyntaxOptArg::from_config_value(value, required))
                            .collect::<Vec<SyntaxOptArg>>();
                        parameters.extend(arguments);
                    } else if let Some(arg) = SyntaxOptArg::from_config_value(value, required) {
                        parameters.push(arg);
                    }
                }
            }

            if let Some(value) = table.get("groups") {
                groups = SyntaxGroup::from_config_value_multi(value);
            }

            if let Some(value) = table.get("usage") {
                if let Some(value) = value.as_str() {
                    usage = Some(value.to_string());
                }
            }
        } else if let Some(value) = config_value.as_str() {
            usage = Some(value.to_string());
        }

        if parameters.is_empty() && groups.is_empty() && usage.is_none() {
            return None;
        }

        Some(Self {
            usage,
            parameters,
            groups,
        })
    }

    /// The 'leftovers' parameter is used to capture all the remaining arguments
    /// It corresponds to using 'trailing_var_arg' and 'allow_hyphen_values' in clap
    /// The following will lead to panic:
    /// - Using 'leftovers' more than once
    /// - Using 'leftovers' before the last positional argument
    /// - Using 'leftovers' with a non-positional argument
    fn check_parameters_leftovers(&self) -> Result<(), String> {
        // Grab all the leftovers params
        let leftovers_params = self.parameters.iter().filter(|param| param.leftovers);

        // Check if the count is greater than one
        if leftovers_params.clone().count() > 1 {
            return Err(format!(
                "only one argument can use {}; found {}",
                "leftovers".light_yellow(),
                leftovers_params
                    .map(|param| param.name.light_yellow())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Check if any is non-positional
        let nonpositional_leftovers = leftovers_params
            .clone()
            .filter(|param| !param.is_positional());
        if nonpositional_leftovers.clone().count() > 0 {
            return Err(format!(
                "only positional arguments can use {}; found {}",
                "leftovers".light_yellow(),
                nonpositional_leftovers
                    .map(|param| param.name.light_yellow())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Check if our leftovers argument is before the last positional argument
        let last_positional_idx = self
            .parameters
            .iter()
            .rposition(|param| param.is_positional());
        if let Some(lpidx) = last_positional_idx {
            for (idx, param) in self.parameters.iter().enumerate() {
                if param.leftovers && idx < lpidx {
                    return Err(format!(
                        "only the last positional argument can use {}",
                        "leftovers".light_yellow()
                    ));
                }
            }
        }

        Ok(())
    }

    /// The 'last' parameter is used to capture arguments after using '--' on the command line
    /// It corresponds to setting 'last' to true in clap
    /// The following will lead to panic:
    /// - Flags using 'last'
    /// - non-positional using 'last'
    fn check_parameters_last(&self) -> Result<(), String> {
        // Grab all the last params
        let params = self
            .parameters
            .iter()
            .filter(|param| param.last_arg_double_dash);

        // Check if any is a non-positional argument
        let nonpositional_last = params.clone().filter(|param| !param.is_positional());
        if nonpositional_last.clone().count() > 0 {
            return Err(format!(
                "only positional arguments can use {}; found {}",
                "last".light_yellow(),
                nonpositional_last
                    .map(|param| param.name.light_yellow())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(())
    }

    /// Since when setting a counter we do not expect any value, parameters using
    /// the `counter` type will panic if:
    /// - They are positional
    /// - They have a num_values
    fn check_parameters_counter(&self) -> Result<(), String> {
        // Grab all the counter params
        let params = self
            .parameters
            .iter()
            .filter(|param| matches!(param.arg_type(), SyntaxOptArgType::Counter));

        for param in params {
            if param.is_positional() {
                return Err(format!(
                    "counter argument {} cannot be positional",
                    param.name.light_yellow()
                ));
            }

            if param.num_values.is_some() {
                return Err(format!(
                    "counter argument {} cannot have a num_values (counters do not take any values)",
                    param.name.light_yellow()
                ));
            }
        }

        Ok(())
    }

    fn check_parameters_references_iter(
        &self,
        references: impl Iterator<Item = impl ToString>,
        available_references: &HashSet<String>,
        reference_type: &str,
        param_name: &str,
    ) -> Result<(), String> {
        for reference in references {
            let reference = reference.to_string();

            if !available_references.contains(&reference) {
                return Err(format!(
                    "parameter or group {} specified in {} for {} does not exist",
                    reference.light_yellow(),
                    reference_type.light_yellow(),
                    param_name.light_yellow(),
                ));
            }
        }

        Ok(())
    }

    fn check_parameters_references(&self) -> Result<(), String> {
        let available_references = self
            .parameters
            .iter()
            .map(|param| param.dest())
            .chain(self.groups.iter().map(|group| group.dest()))
            .collect::<HashSet<_>>();

        for param in &self.parameters {
            let dest = param.dest();

            self.check_parameters_references_iter(
                param.requires.iter().map(|param| sanitize_str(param)),
                &available_references,
                "requires",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param.conflicts_with.iter().map(|param| sanitize_str(param)),
                &available_references,
                "conflicts_with",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param
                    .required_without
                    .iter()
                    .map(|param| sanitize_str(param)),
                &available_references,
                "required_without",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param
                    .required_without_all
                    .iter()
                    .map(|param| sanitize_str(param)),
                &available_references,
                "required_without_all",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param
                    .required_if_eq
                    .iter()
                    .map(|(k, _)| sanitize_str(k))
                    .collect::<Vec<_>>()
                    .iter(),
                &available_references,
                "required_if_eq",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param
                    .required_if_eq_all
                    .iter()
                    .map(|(k, _)| sanitize_str(k)),
                &available_references,
                "required_if_eq_all",
                &dest,
            )?;
        }

        for group in &self.groups {
            let dest = group.dest();

            self.check_parameters_references_iter(
                group.parameters.iter().map(|param| sanitize_str(param)),
                &available_references,
                "parameters",
                &dest,
            )?;

            self.check_parameters_references_iter(
                group.requires.iter().map(|param| sanitize_str(param)),
                &available_references,
                "requires",
                &dest,
            )?;

            self.check_parameters_references_iter(
                group.conflicts_with.iter().map(|param| sanitize_str(param)),
                &available_references,
                "conflicts_with",
                &dest,
            )?;
        }

        Ok(())
    }

    /// The identifiers in the parameters and groups should be unique
    /// across the parameters and groups, or else it will lead to panic
    fn check_parameters_unique_names(&self) -> Result<(), String> {
        let mut dests = HashSet::new();
        let mut names = HashSet::new();

        for param in &self.parameters {
            let dest = param.dest();
            if !dests.insert(dest.clone()) {
                return Err(format!(
                    "identifier {} is defined more than once",
                    dest.light_yellow()
                ));
            }

            for name in param.all_names() {
                // Check if name is -h or --help or any other reserved names
                if Self::RESERVED_NAMES.contains(&name.as_str()) {
                    return Err(format!(
                        "name {} is reserved and cannot be used",
                        name.light_yellow()
                    ));
                }

                if !names.insert(name.clone()) {
                    return Err(format!(
                        "name {} is defined more than once",
                        name.light_yellow()
                    ));
                }
            }
        }

        for group in &self.groups {
            let dest = group.dest();
            if !dests.insert(dest.clone()) {
                return Err(format!(
                    "identifier {} is defined more than once",
                    dest.light_yellow()
                ));
            }
        }

        Ok(())
    }

    fn check_parameters(&self) -> Result<(), String> {
        self.check_parameters_unique_names()?;
        self.check_parameters_references()?;
        self.check_parameters_leftovers()?;
        self.check_parameters_last()?;
        self.check_parameters_counter()?;

        Ok(())
    }

    pub fn argparser(&self, called_as: Vec<String>) -> Result<clap::Command, String> {
        let mut parser = clap::Command::new(called_as.join(" "))
            .disable_help_subcommand(true)
            .disable_version_flag(true);

        if let Err(err) = self.check_parameters() {
            return Err(err);
        }

        for param in &self.parameters {
            parser = param.add_to_argparser(parser);
        }

        for group in &self.groups {
            parser = group.add_to_argparser(parser);
        }

        Ok(parser)
    }

    pub fn parse_args(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> BTreeMap<String, String> {
        match self.internal_parse_args(argv, called_as) {
            Ok(args) => args,
            Err(_err) => {
                exit(1);
            }
        }
    }

    fn internal_parse_args(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let parser = match self.argparser(called_as.clone()) {
            Ok(parser) => parser,
            Err(err) => {
                omni_print!(format!("{} {}", "parser error:".red(), err));
                return Err(err);
            }
        };

        // let matches = parser.get_matches_from(&parse_argv);
        let matches = match parser.try_get_matches_from(&parse_argv) {
            Err(err) => {
                match err.kind() {
                    clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                        // TODO: move to use the omni help instead
                        // _ = parser.get_matches_from(&parse_argv);
                        HelpCommand::new().exec(called_as);
                        panic!("help command should have exited");
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
                        let err_str = err_str.trim_start_matches("error: ").trim();
                        omni_error!(err_str);
                        return Err(err_str.to_string());
                    }
                }
            }
            Ok(matches) => matches,
        };

        let mut args = BTreeMap::new();
        let mut all_args = Vec::new();

        for param in &self.parameters {
            all_args.push(param.dest());
            param.add_to_args(&mut args, &matches, None);
        }

        for group in &self.groups {
            all_args.push(group.dest());
            group.add_to_args(&mut args, &matches, &self.parameters);
        }

        args.insert("OMNI_ARG_LIST".to_string(), all_args.join(" "));

        Ok(args)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SyntaxOptArg {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "SyntaxOptArgType::is_default")]
    pub arg_type: SyntaxOptArgType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_values: Option<usize>,
    #[serde(rename = "delimiter", skip_serializing_if = "Option::is_none")]
    pub value_delimiter: Option<char>,
    #[serde(rename = "last", skip_serializing_if = "cache_utils::is_false")]
    pub last_arg_double_dash: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub leftovers: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_without: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_without_all: Vec<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub required_if_eq: HashMap<String, String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub required_if_eq_all: HashMap<String, String>,
}

impl Default for SyntaxOptArg {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            dest: None,
            aliases: vec![],
            desc: None,
            required: false,
            placeholder: None,
            arg_type: SyntaxOptArgType::String,
            default: None,
            num_values: None,
            value_delimiter: None,
            last_arg_double_dash: false,
            leftovers: false,
            requires: vec![],
            conflicts_with: vec![],
            required_without: vec![],
            required_without_all: vec![],
            required_if_eq: HashMap::new(),
            required_if_eq_all: HashMap::new(),
        }
    }
}

impl SyntaxOptArg {
    #[allow(dead_code)]
    pub fn new(name: String, desc: Option<String>, required: bool) -> Self {
        Self {
            name,
            desc,
            required,
            ..Self::default()
        }
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        required: Option<bool>,
    ) -> Option<Self> {
        let name;
        let mut desc = None;
        let mut dest = None;
        let mut required = required;
        let mut arg_type = SyntaxOptArgType::String;
        let mut placeholder = None;
        let mut default = None;
        let mut num_values = None;
        let mut value_delimiter = None;
        let mut last_arg_double_dash = false;
        let mut leftovers = false;
        let mut requires = vec![];
        let mut conflicts_with = vec![];
        let mut required_without = vec![];
        let mut required_without_all = vec![];
        let mut required_if_eq = HashMap::new();
        let mut required_if_eq_all = HashMap::new();
        let mut aliases = vec![];

        if let Some(table) = config_value.as_table() {
            let value_for_details;

            if let Some(name_value) = table.get("name") {
                if let Some(name_value) = name_value.as_str() {
                    name = name_value.to_string();
                    value_for_details = Some(config_value.clone());
                } else {
                    return None;
                }
            } else if table.len() == 1 {
                if let Some((key, value)) = table.into_iter().next() {
                    name = key;
                    value_for_details = Some(value);
                } else {
                    return None;
                }
            } else {
                return None;
            }

            if let Some(value_for_details) = value_for_details {
                if let Some(value_str) = value_for_details.as_str() {
                    desc = Some(value_str.to_string());
                } else if let Some(value_table) = value_for_details.as_table() {
                    desc = value_table
                        .get("desc")
                        .and_then(|value| value.as_str_forced());
                    dest = value_table
                        .get("dest")
                        .and_then(|value| value.as_str_forced());

                    if required.is_none() {
                        required = value_table
                            .get("required")
                            .and_then(|value| value.as_bool_forced());
                    }

                    arg_type = value_table
                        .get("type")
                        .and_then(|value| value.as_str_forced())
                        .and_then(|value| SyntaxOptArgType::from_str(&value))
                        .unwrap_or(SyntaxOptArgType::String);
                    placeholder = value_table
                        .get("placeholder")
                        .and_then(|value| value.as_str_forced());
                    default = value_table
                        .get("default")
                        .and_then(|value| value.as_str_forced());
                    num_values = value_table
                        .get("num_values")
                        .and_then(|value| value.as_integer().map(|value| value as usize));
                    value_delimiter = value_table
                        .get("delimiter")
                        .and_then(|value| value.as_str_forced())
                        .and_then(|value| value.chars().next());
                    last_arg_double_dash = value_table
                        .get("last")
                        .and_then(|value| value.as_bool_forced())
                        .unwrap_or(false);
                    leftovers = value_table
                        .get("leftovers")
                        .and_then(|value| value.as_bool_forced())
                        .unwrap_or(false);

                    // TODO: this happens a lot, should make a function in config_value to handle
                    // that situation
                    if let Some(requires_value) = value_table.get("requires") {
                        if let Some(value) = requires_value.as_str_forced() {
                            requires.push(value.to_string());
                        } else if let Some(array) = requires_value.as_array() {
                            for value in array {
                                if let Some(value) = value.as_str_forced() {
                                    requires.push(value.to_string());
                                }
                            }
                        }
                    }

                    if let Some(conflicts_with_value) = value_table.get("conflicts_with") {
                        if let Some(value) = conflicts_with_value.as_str_forced() {
                            conflicts_with.push(value.to_string());
                        } else if let Some(array) = conflicts_with_value.as_array() {
                            for value in array {
                                if let Some(value) = value.as_str_forced() {
                                    conflicts_with.push(value.to_string());
                                }
                            }
                        }
                    }

                    if let Some(required_without_value) = value_table.get("required_without") {
                        if let Some(value) = required_without_value.as_str_forced() {
                            required_without.push(value.to_string());
                        } else if let Some(array) = required_without_value.as_array() {
                            for value in array {
                                if let Some(value) = value.as_str_forced() {
                                    required_without.push(value.to_string());
                                }
                            }
                        }
                    }

                    if let Some(required_without_all_value) =
                        value_table.get("required_without_all")
                    {
                        if let Some(value) = required_without_all_value.as_str_forced() {
                            required_without_all.push(value.to_string());
                        } else if let Some(array) = required_without_all_value.as_array() {
                            for value in array {
                                if let Some(value) = value.as_str_forced() {
                                    required_without_all.push(value.to_string());
                                }
                            }
                        }
                    }

                    if let Some(required_if_eq_value) = value_table.get("required_if_eq") {
                        if let Some(value) = required_if_eq_value.as_table() {
                            for (key, value) in value {
                                if let Some(value) = value.as_str_forced() {
                                    required_if_eq.insert(key.to_string(), value.to_string());
                                }
                            }
                        }
                    }

                    if let Some(required_if_eq_all_value) = value_table.get("required_if_eq_all") {
                        if let Some(value) = required_if_eq_all_value.as_table() {
                            for (key, value) in value {
                                if let Some(value) = value.as_str_forced() {
                                    required_if_eq_all.insert(key.to_string(), value.to_string());
                                }
                            }
                        }
                    }

                    if let Some(aliases_value) = value_table.get("aliases") {
                        if let Some(value) = aliases_value.as_str_forced() {
                            aliases.push(value.to_string());
                        } else if let Some(array) = aliases_value.as_array() {
                            for value in array {
                                if let Some(value) = value.as_str_forced() {
                                    aliases.push(value.to_string());
                                }
                            }
                        }
                    }
                }
            }
        } else {
            name = config_value.as_str().unwrap();
        }

        Some(Self {
            name,
            dest,
            aliases,
            desc,
            required: required.unwrap_or(false),
            placeholder,
            arg_type,
            default,
            num_values,
            value_delimiter,
            last_arg_double_dash,
            leftovers,
            requires,
            conflicts_with,
            required_without,
            required_without_all,
            required_if_eq,
            required_if_eq_all,
        })
    }

    pub fn arg_type(&self) -> SyntaxOptArgType {
        let convert_to_array = if self.leftovers || self.value_delimiter.is_some() {
            true
        } else if let Some(num_values) = self.num_values {
            num_values > 1
        } else {
            false
        };

        if convert_to_array {
            match &self.arg_type {
                SyntaxOptArgType::String => SyntaxOptArgType::ArrayString,
                SyntaxOptArgType::Integer => SyntaxOptArgType::ArrayInteger,
                SyntaxOptArgType::Float => SyntaxOptArgType::ArrayFloat,
                SyntaxOptArgType::Boolean => SyntaxOptArgType::ArrayBoolean,
                SyntaxOptArgType::Enum(possible_values) => {
                    SyntaxOptArgType::ArrayEnum(possible_values.clone())
                }
                _ => self.arg_type.clone(),
            }
        } else {
            self.arg_type.clone()
        }
    }

    pub fn dest(&self) -> String {
        let dest = match self.dest {
            Some(ref dest) => dest.clone(),
            None => self.name.clone(),
        };

        sanitize_str(&dest)
    }

    pub fn all_names(&self) -> Vec<String> {
        let mut names = vec![self.name.clone()];
        names.extend(self.aliases.clone());
        names
    }

    pub fn is_positional(&self) -> bool {
        !self.name.starts_with('-')
    }

    pub fn add_to_argparser(&self, parser: clap::Command) -> clap::Command {
        let mut arg = clap::Arg::new(self.dest());

        // Add the help for the argument
        if let Some(desc) = &self.desc {
            arg = arg.help(desc);
        }

        // Add all the names for that argument
        if self.name.starts_with('-') {
            let trimmed = self.name.trim_start_matches('-').to_string();
            if trimmed.len() == 1 {
                arg = arg.short(trimmed.chars().next().unwrap());
            } else {
                arg = arg.long(&trimmed);
            }

            for alias in &self.aliases {
                // Check if the parameter starts with a dash
                if !alias.starts_with('-') {
                    unreachable!("should not have non-dash aliases")
                }

                // Check if the parameter is short or long (short is one character, long is more)
                let trimmed = alias.trim_start_matches('-').to_string();
                if trimmed.len() == 1 {
                    arg = arg.short(trimmed.chars().next().unwrap());
                } else {
                    arg = arg.long(&trimmed);
                }
            }
        }

        // Set the placeholder if any
        if let Some(placeholder) = &self.placeholder {
            arg = arg.value_name(placeholder);
        }

        // Set the default value
        if let Some(default) = &self.default {
            arg = arg.default_value(default);
        }

        // Set how to parse the values
        if let Some(num_values) = &self.num_values {
            arg = arg.num_args(*num_values);
        }
        if let Some(value_delimiter) = &self.value_delimiter {
            arg = arg.value_delimiter(*value_delimiter);
        }
        if self.last_arg_double_dash {
            arg = arg.last(true);
        }
        if self.leftovers {
            arg = arg.trailing_var_arg(true).allow_hyphen_values(true);
        }

        // Set conflicts and requirements
        for require_arg in &self.requires {
            let require_arg = sanitize_str(require_arg);
            arg = arg.requires(&require_arg);
        }
        for conflict_arg in &self.conflicts_with {
            let conflict_arg = sanitize_str(conflict_arg);
            arg = arg.conflicts_with(&conflict_arg);
        }
        if !self.required_without.is_empty() {
            let required_without = self
                .required_without
                .iter()
                .map(|name| sanitize_str(name))
                .collect::<Vec<String>>();
            arg = arg.required_unless_present_any(&required_without);
        }
        if !self.required_without_all.is_empty() {
            let required_without_all = self
                .required_without_all
                .iter()
                .map(|name| sanitize_str(name))
                .collect::<Vec<String>>();
            arg = arg.required_unless_present_all(&required_without_all);
        }
        if !self.required_if_eq.is_empty() {
            arg = arg.required_if_eq_any(
                self.required_if_eq
                    .iter()
                    .map(|(k, v)| (sanitize_str(k), v.clone()))
                    .collect::<Vec<(String, String)>>(),
            );
        }
        if !self.required_if_eq_all.is_empty() {
            arg = arg.required_if_eq_all(
                self.required_if_eq_all
                    .iter()
                    .map(|(k, v)| (sanitize_str(k), v.clone()))
                    .collect::<Vec<(String, String)>>(),
            );
        }
        if self.required {
            arg = arg.required(true);
        }

        // Set the action, i.e. how the values are stored when the selfeter is used
        match &self.arg_type() {
            SyntaxOptArgType::String
            | SyntaxOptArgType::Integer
            | SyntaxOptArgType::Float
            | SyntaxOptArgType::Boolean
            | SyntaxOptArgType::Enum(_) => {
                arg = arg.action(clap::ArgAction::Set);
            }
            SyntaxOptArgType::ArrayString
            | SyntaxOptArgType::ArrayInteger
            | SyntaxOptArgType::ArrayFloat
            | SyntaxOptArgType::ArrayBoolean
            | SyntaxOptArgType::ArrayEnum(_) => {
                arg = arg.action(clap::ArgAction::Append);
            }
            SyntaxOptArgType::Flag => {
                if str_to_bool(&self.default.clone().unwrap_or_default()).unwrap_or(false) {
                    arg = arg.action(clap::ArgAction::SetFalse);
                } else {
                    arg = arg.action(clap::ArgAction::SetTrue);
                }
            }
            SyntaxOptArgType::Counter => {
                arg = arg.action(clap::ArgAction::Count);
            }
        };

        // Set the validators, i.e. how the values are checked when the parameter is used
        match &self.arg_type() {
            SyntaxOptArgType::Integer | SyntaxOptArgType::ArrayInteger => {
                arg = arg.value_parser(clap::value_parser!(i64));
            }
            SyntaxOptArgType::Float | SyntaxOptArgType::ArrayFloat => {
                arg = arg.value_parser(clap::value_parser!(f64));
            }
            SyntaxOptArgType::Boolean | SyntaxOptArgType::ArrayBoolean => {
                arg = arg.value_parser(clap::value_parser!(bool));
            }
            SyntaxOptArgType::Enum(possible_values)
            | SyntaxOptArgType::ArrayEnum(possible_values) => {
                arg = arg.value_parser(possible_values.clone());
            }
            _ => {}
        }

        parser.arg(arg)
    }

    pub fn add_to_args(
        &self,
        args: &mut BTreeMap<String, String>,
        matches: &clap::ArgMatches,
        override_dest: Option<String>,
    ) {
        let dest = self.dest();
        match &self.arg_type() {
            SyntaxOptArgType::String | SyntaxOptArgType::Enum(_) => {
                extract_one_value_to_env::<String>(
                    matches,
                    &dest,
                    &self.default,
                    "str",
                    args,
                    override_dest,
                );
            }
            SyntaxOptArgType::Integer => {
                extract_one_value_to_env::<i64>(
                    matches,
                    &dest,
                    &self.default,
                    "int",
                    args,
                    override_dest,
                );
            }
            SyntaxOptArgType::Counter => {
                extract_one_value_to_env::<u8>(
                    matches,
                    &dest,
                    &self.default,
                    "int",
                    args,
                    override_dest,
                );
            }
            SyntaxOptArgType::Float => {
                extract_one_value_to_env::<f64>(
                    matches,
                    &dest,
                    &self.default,
                    "float",
                    args,
                    override_dest,
                );
            }
            SyntaxOptArgType::Boolean | SyntaxOptArgType::Flag => {
                let default =
                    str_to_bool(&self.default.clone().unwrap_or_default()).unwrap_or(false);
                let value = *matches.get_one::<bool>(&dest).unwrap_or(&default);
                let env_dest = override_dest.unwrap_or(dest.clone()).to_uppercase();
                args.insert(format!("OMNI_ARG_{}_VALUE", env_dest), value.to_string());
                args.insert(format!("OMNI_ARG_{}_TYPE", env_dest), "bool".to_string());
            }
            SyntaxOptArgType::ArrayString | SyntaxOptArgType::ArrayEnum(_) => {
                extract_many_values_to_env::<String>(
                    matches,
                    &dest,
                    &self.default,
                    "str",
                    args,
                    override_dest,
                );
            }
            SyntaxOptArgType::ArrayInteger => {
                extract_many_values_to_env::<i64>(
                    matches,
                    &dest,
                    &self.default,
                    "int",
                    args,
                    override_dest,
                );
            }
            SyntaxOptArgType::ArrayFloat => {
                extract_many_values_to_env::<f64>(
                    matches,
                    &dest,
                    &self.default,
                    "float",
                    args,
                    override_dest,
                );
            }
            SyntaxOptArgType::ArrayBoolean => {
                extract_many_values_to_env::<bool>(
                    matches,
                    &dest,
                    &self.default,
                    "bool",
                    args,
                    override_dest,
                );
            }
        };
    }
}

fn extract_one_value<T: Any + Clone + Send + Sync + 'static + ToString + FromStr>(
    matches: &clap::ArgMatches,
    dest: &str,
    default: &Option<String>,
) -> String {
    match (matches.get_one::<T>(dest), default) {
        (Some(value), _) => value.to_string(),
        (None, Some(default)) => match default.parse::<T>() {
            Ok(value) => value.to_string(),
            Err(_) => "".to_string(),
        },
        _ => "".to_string(),
    }
}

fn extract_one_value_to_env<T: Any + Clone + Send + Sync + 'static + ToString + FromStr>(
    matches: &clap::ArgMatches,
    dest: &str,
    default: &Option<String>,
    arg_type: &str,
    args: &mut BTreeMap<String, String>,
    override_dest: Option<String>,
) {
    let env_dest = override_dest.unwrap_or(dest.to_string()).to_uppercase();
    args.insert(format!("OMNI_ARG_{}_TYPE", env_dest), arg_type.to_string());

    let value = extract_one_value::<T>(matches, dest, default).to_string();
    if !value.is_empty() {
        args.insert(format!("OMNI_ARG_{}_VALUE", env_dest), value);
    }
}

fn extract_many_values<T: Any + Clone + Send + Sync + 'static + ToString + FromStr>(
    matches: &clap::ArgMatches,
    dest: &str,
    default: &Option<String>,
) -> Vec<String> {
    match (matches.get_many::<T>(dest), default) {
        (Some(values), _) => values
            .collect::<Vec<_>>()
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        (None, Some(default)) => default
            .split(',')
            .flat_map(|part| part.trim().parse::<T>())
            .map(|value| value.to_string())
            .collect(),
        _ => vec![],
    }
}

fn extract_many_values_to_env<T: Any + Clone + Send + Sync + 'static + ToString + FromStr>(
    matches: &clap::ArgMatches,
    dest: &str,
    default: &Option<String>,
    arg_type: &str,
    args: &mut BTreeMap<String, String>,
    override_dest: Option<String>,
) {
    let values = extract_many_values::<T>(matches, dest, default);
    let env_dest = override_dest.unwrap_or(dest.to_string()).to_uppercase();
    for (i, value) in values.iter().enumerate() {
        args.insert(
            format!("OMNI_ARG_{}_VALUE_{}", env_dest, i),
            value.to_string(),
        );
    }
    args.insert(
        format!("OMNI_ARG_{}_TYPE", env_dest),
        format!("{}/{}", arg_type, values.len()),
    );
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum SyntaxOptArgType {
    #[default]
    #[serde(rename = "str", alias = "string")]
    String,
    #[serde(rename = "int", alias = "integer")]
    Integer,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "bool")]
    Boolean,
    #[serde(rename = "flag")]
    Flag,
    #[serde(rename = "count", alias = "counter")]
    Counter,
    #[serde(rename = "enum")]
    Enum(Vec<String>),
    #[serde(rename = "array/str", alias = "array/string")]
    ArrayString,
    #[serde(rename = "array/int", alias = "array/integer")]
    ArrayInteger,
    #[serde(rename = "array/float")]
    ArrayFloat,
    #[serde(rename = "array/bool")]
    ArrayBoolean,
    #[serde(rename = "array/enum")]
    ArrayEnum(Vec<String>),
}

impl SyntaxOptArgType {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn from_str(value: &str) -> Option<Self> {
        let mut is_array = false;

        let normalized = value.trim().to_lowercase();
        let mut value = normalized.trim();

        if value.starts_with("array/") {
            value = &value[6..];
            is_array = true;
        } else if value.starts_with("[") && value.ends_with("]") {
            value = &value[1..value.len() - 1];
            is_array = true;
        }

        let obj = match value.to_lowercase().as_str() {
            "int" | "integer" => {
                if is_array {
                    Self::ArrayInteger
                } else {
                    Self::Integer
                }
            }
            "float" => {
                if is_array {
                    Self::ArrayFloat
                } else {
                    Self::Float
                }
            }
            "bool" | "boolean" => {
                if is_array {
                    Self::ArrayBoolean
                } else {
                    Self::Boolean
                }
            }
            "flag" => {
                if is_array {
                    return None;
                } else {
                    Self::Flag
                }
            }
            "count" | "counter" => {
                if is_array {
                    return None;
                } else {
                    Self::Counter
                }
            }
            "str" | "string" => {
                if is_array {
                    Self::ArrayString
                } else {
                    Self::String
                }
            }
            "array" => Self::ArrayString,
            _ => {
                // If the string is in format array/enum(xx, yy, zz) or enum(xx, yy, zz) or (xx, yy, zz)
                // or [(xx, yy, zz)], then it's an enum and we need to extract the values
                let mut enum_contents = None;

                if value.starts_with("enum(") && value.ends_with(")") {
                    enum_contents = Some(&value[5..value.len() - 1]);
                } else if value.starts_with("(") && value.ends_with(")") {
                    enum_contents = Some(&value[1..value.len() - 1]);
                }

                if let Some(enum_contents) = enum_contents {
                    let values = enum_contents
                        .split(',')
                        .map(|value| value.trim().to_string())
                        .collect::<Vec<String>>();

                    if is_array {
                        Self::ArrayEnum(values)
                    } else {
                        Self::Enum(values)
                    }
                } else {
                    return None;
                }
            }
        };

        Some(obj)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SyntaxGroup {
    pub name: String,
    pub parameters: Vec<String>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub multiple: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub required: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
}

impl Default for SyntaxGroup {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            parameters: vec![],
            multiple: false,
            required: false,
            requires: vec![],
            conflicts_with: vec![],
        }
    }
}

impl SyntaxGroup {
    /// Create a vector of groups from a config value that can contain multiple groups.
    /// This supports the groups being specified as:
    ///
    /// ```yaml
    /// groups:
    ///  - name: group1
    ///    parameters:
    ///    - param1
    ///    - param2
    ///    multiple: true
    ///    required: true
    /// - name: group2
    ///   parameters: param3
    ///   requires: group1
    ///   conflicts_with: group3
    /// - group3:
    ///     parameters: param4
    /// ```
    ///
    /// Or as:
    ///
    /// ```yaml
    /// groups:
    ///   group1:
    ///     parameters:
    ///     - param1
    ///     - param2
    ///     multiple: true
    ///     required: true
    ///   group2:
    ///     parameters: param3
    ///     requires: group1
    ///     conflicts_with: group3
    ///   group3:
    ///     parameters: param4
    /// ```
    ///
    /// The ConfigValue object received is the contents of the `groups` key in the config file.
    pub(super) fn from_config_value_multi(config_value: &ConfigValue) -> Vec<Self> {
        let mut groups = vec![];

        if let Some(array) = config_value.as_array() {
            // If this is an array, we can simply iterate over it and create the groups
            for value in array {
                if let Some(group) = Self::from_config_value(&value, None) {
                    groups.push(group);
                }
            }
        } else if let Some(table) = config_value.as_table() {
            // If this is a table, we need to iterate over the keys and create the groups
            for (name, value) in table {
                if let Some(group) = Self::from_config_value(&value, Some(name.to_string())) {
                    groups.push(group);
                }
            }
        }

        groups
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        name: Option<String>,
    ) -> Option<Self> {
        // Exit early if the value is not a table
        let mut table = match config_value.as_table() {
            Some(table) => table,
            None => return None,
        };

        // Exit early if the table is empty
        if table.is_empty() {
            return None;
        }

        // Handle the group name
        let name = match name {
            Some(name) => name,
            None => {
                if table.len() == 1 {
                    // Extract the only key from the table, this will be the name of the group
                    let key = table.keys().next().unwrap().to_string();
                    // Change the table to be the value of the key, this will be the group's config
                    table = table.get(&key)?.as_table()?;
                    // Return the key as the name of the group
                    key
                } else {
                    table
                        .get("name")
                        .and_then(|value| value.as_str_forced())?
                        .to_string()
                }
            }
        };

        // Handle the group parameters
        let mut parameters = vec![];
        if let Some(parameters_value) = table.get("parameters") {
            if let Some(value) = parameters_value.as_str() {
                parameters.push(value.to_string());
            } else if let Some(array) = parameters_value.as_array() {
                parameters.extend(
                    array
                        .iter()
                        .filter_map(|value| value.as_str_forced().map(|value| value.to_string())),
                );
            }
        }
        // No parameters, skip this group
        if parameters.is_empty() {
            return None;
        }

        // Parse the rest of the group configuration
        let mut multiple = false;
        let mut required = false;
        let mut requires = vec![];
        let mut conflicts_with = vec![];

        if let Some(multiple_value) = table.get("multiple") {
            if let Some(multiple_value) = multiple_value.as_bool_forced() {
                multiple = multiple_value;
            }
        }

        if let Some(required_value) = table.get("required") {
            if let Some(required_value) = required_value.as_bool_forced() {
                required = required_value;
            }
        }

        if let Some(requires_value) = table.get("requires") {
            if let Some(value) = requires_value.as_str_forced() {
                requires.push(value.to_string());
            } else if let Some(array) = requires_value.as_array() {
                for value in array {
                    if let Some(value) = value.as_str_forced() {
                        requires.push(value.to_string());
                    }
                }
            }
        }

        if let Some(conflicts_with_value) = table.get("conflicts_with") {
            if let Some(value) = conflicts_with_value.as_str_forced() {
                conflicts_with.push(value.to_string());
            } else if let Some(array) = conflicts_with_value.as_array() {
                for value in array {
                    if let Some(value) = value.as_str_forced() {
                        conflicts_with.push(value.to_string());
                    }
                }
            }
        }

        Some(Self {
            name,
            parameters,
            multiple,
            required,
            requires,
            conflicts_with,
        })
    }

    fn dest(&self) -> String {
        sanitize_str(&self.name)
    }

    fn add_to_argparser(&self, parser: clap::Command) -> clap::Command {
        let args = self
            .parameters
            .iter()
            .map(|param| sanitize_str(param))
            .collect::<Vec<String>>();

        let mut group = clap::ArgGroup::new(self.dest())
            .args(&args)
            .multiple(self.multiple)
            .required(self.required);

        // Set conflicts and requirements
        for require_arg in &self.requires {
            let require_arg = sanitize_str(require_arg);
            group = group.requires(&require_arg);
        }
        for conflict_arg in &self.conflicts_with {
            let conflict_arg = sanitize_str(conflict_arg);
            group = group.conflicts_with(&conflict_arg);
        }

        parser.group(group)
    }

    fn add_to_args(
        &self,
        args: &mut BTreeMap<String, String>,
        matches: &clap::ArgMatches,
        parameters: &Vec<SyntaxOptArg>,
    ) {
        let dest = self.dest();

        let param_id = match matches.get_one::<clap::Id>(&dest) {
            Some(param_id) => param_id.to_string(),
            None => return,
        };

        let param = match parameters.iter().find(|param| *param.dest() == param_id) {
            Some(param) => param,
            None => return,
        };

        param.add_to_args(args, matches, Some(dest.clone()));
    }
}

fn sanitize_str(s: &str) -> String {
    let mut prev_is_sanitized = false;
    let s = s
        .chars()
        // Replace all non-alphanumeric characters with _
        .flat_map(|c| {
            if c.is_alphanumeric() {
                prev_is_sanitized = false;
                Some(c)
            } else if !prev_is_sanitized {
                prev_is_sanitized = true;
                Some('_')
            } else {
                None
            }
        })
        .collect::<String>();

    s.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disable_colors() {
        std::env::set_var("NO_COLOR", "true");
    }

    mod command_syntax {
        use super::*;

        mod check_parameters_unique_names {
            use super::*;

            #[test]
            fn test_params_dest() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            dest: Some("paramdest".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            dest: Some("paramdest".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let errmsg = "identifier paramdest is defined more than once";
                assert_eq!(
                    syntax.check_parameters_unique_names(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_params_names() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            aliases: vec!["--param2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let errmsg = "name --param2 is defined more than once";
                assert_eq!(
                    syntax.check_parameters_unique_names(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_params_and_groups() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        ..SyntaxOptArg::default()
                    }],
                    groups: vec![SyntaxGroup {
                        name: "param1".to_string(),
                        parameters: vec!["--param1".to_string()],
                        ..SyntaxGroup::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "identifier param1 is defined more than once";
                assert_eq!(
                    syntax.check_parameters_unique_names(),
                    Err(errmsg.to_string())
                );
            }
        }

        mod check_parameters_references {
            use super::*;

            #[test]
            fn test_param_requires() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        requires: vec!["--param2".to_string()],
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group param2 specified in requires for param1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_param_conflicts_with() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        conflicts_with: vec!["--param2".to_string()],
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group param2 specified in conflicts_with for param1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_param_required_without() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        required_without: vec!["--param2".to_string()],
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group param2 specified in required_without for param1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_param_required_without_all() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        required_without_all: vec!["--param2".to_string()],
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group param2 specified in required_without_all for param1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_param_required_if_eq() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        required_if_eq: HashMap::from_iter(vec![(
                            "param2".to_string(),
                            "value".to_string(),
                        )]),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group param2 specified in required_if_eq for param1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_param_required_if_eq_all() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        required_if_eq_all: HashMap::from_iter(vec![(
                            "param2".to_string(),
                            "value".to_string(),
                        )]),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group param2 specified in required_if_eq_all for param1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_group_parameters() {
                let syntax = CommandSyntax {
                    groups: vec![SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string()],
                        ..SyntaxGroup::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group param1 specified in parameters for group1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_group_requires_group_exists() {
                let syntax = CommandSyntax {
                    groups: vec![
                        SyntaxGroup {
                            name: "group1".to_string(),
                            parameters: vec![],
                            requires: vec!["group2".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group2".to_string(),
                            parameters: vec![],
                            ..SyntaxGroup::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                assert_eq!(syntax.check_parameters_references(), Ok(()));
            }

            #[test]
            fn test_group_requires_param_exists() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        ..SyntaxOptArg::default()
                    }],
                    groups: vec![SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec![],
                        requires: vec!["param1".to_string()],
                        ..SyntaxGroup::default()
                    }],
                    ..CommandSyntax::default()
                };

                assert_eq!(syntax.check_parameters_references(), Ok(()));
            }

            #[test]
            fn test_group_requires() {
                let syntax = CommandSyntax {
                    groups: vec![SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec![],
                        requires: vec!["group2".to_string()],
                        ..SyntaxGroup::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group group2 specified in requires for group1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_group_conflicts_with() {
                let syntax = CommandSyntax {
                    groups: vec![SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec![],
                        conflicts_with: vec!["group2".to_string()],
                        ..SyntaxGroup::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "parameter or group group2 specified in conflicts_with for group1 does not exist";
                assert_eq!(
                    syntax.check_parameters_references(),
                    Err(errmsg.to_string())
                );
            }
        }

        mod check_parameters_leftovers {
            use super::*;

            #[test]
            fn test_use_more_than_once() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "param1".to_string(),
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "param2".to_string(),
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let errmsg = "only one argument can use leftovers; found param1, param2";
                assert_eq!(syntax.check_parameters_leftovers(), Err(errmsg.to_string()));
            }

            #[test]
            fn test_use_before_last_positional() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "param1".to_string(),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "param2".to_string(),
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "param3".to_string(),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let errmsg = "only the last positional argument can use leftovers";
                assert_eq!(syntax.check_parameters_leftovers(), Err(errmsg.to_string()));
            }

            #[test]
            fn test_use_with_non_positional() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "param1".to_string(),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let errmsg = "only positional arguments can use leftovers; found --param2";
                assert_eq!(syntax.check_parameters_leftovers(), Err(errmsg.to_string()));
            }
        }

        mod check_parameters_last {
            use super::*;

            #[test]
            fn test_non_positional() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        last_arg_double_dash: true,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "only positional arguments can use last; found --param1";
                assert_eq!(syntax.check_parameters_last(), Err(errmsg.to_string()));
            }
        }

        mod check_parameters_counter {
            use super::*;

            #[test]
            fn test_positional() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "param1".to_string(),
                        arg_type: SyntaxOptArgType::Counter,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "counter argument param1 cannot be positional";
                assert_eq!(syntax.check_parameters_counter(), Err(errmsg.to_string()));
            }

            #[test]
            fn test_num_values() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        arg_type: SyntaxOptArgType::Counter,
                        num_values: Some(1),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "counter argument --param1 cannot have a num_values (counters do not take any values)";
                assert_eq!(syntax.check_parameters_counter(), Err(errmsg.to_string()));
            }
        }

        mod parse_args {
            use super::*;

            fn check_expectations(
                syntax: &CommandSyntax,
                expectations: &Vec<(&[&str], Option<&str>)>,
            ) {
                for (argv, expectation) in expectations {
                    let parsed_args = syntax.internal_parse_args(
                        argv.iter().map(|s| s.to_string()).collect(),
                        vec!["test".to_string()],
                    );
                    match &expectation {
                        Some(errmsg) => {
                            assert_eq!((argv, parsed_args), (argv, Err(errmsg.to_string())))
                        }
                        None => {
                            if let Err(ref e) = parsed_args {
                                panic!("case with args {:?} should have succeeded but failed with error: {}", argv, e);
                            }
                        }
                    }
                }
            }

            #[test]
            fn test_simple() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(
                    ["--param1", "value1", "--param2", "42"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "param1 param2"),
                    ("OMNI_ARG_PARAM1_TYPE", "str"),
                    ("OMNI_ARG_PARAM1_VALUE", "value1"),
                    ("OMNI_ARG_PARAM2_TYPE", "int"),
                    ("OMNI_ARG_PARAM2_VALUE", "42"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_string_value() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        arg_type: SyntaxOptArgType::String,
                        required: true,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations = vec![
                    (vec!["--param1", "value1"], "value1"),
                    (vec!["--param1", ""], ""),
                    (vec!["--param1", "1"], "1"),
                ];

                for (argv, value) in expectations {
                    let args = match syntax.internal_parse_args(
                        argv.iter().map(|s| s.to_string()).collect(),
                        vec!["test".to_string()],
                    ) {
                        Ok(args) => args,
                        Err(e) => panic!("{}", e),
                    };

                    let mut expectations =
                        vec![("OMNI_ARG_LIST", "param1"), ("OMNI_ARG_PARAM1_TYPE", "str")];
                    if !value.is_empty() {
                        expectations.push(("OMNI_ARG_PARAM1_VALUE", value));
                    }

                    assert_eq!((&argv, args.len()), (&argv, expectations.len()));
                    for (key, value) in expectations {
                        assert_eq!(
                            (&argv, key, args.get(key)),
                            (&argv, key, Some(&value.to_string()))
                        );
                    }
                }
            }

            #[test]
            fn test_unexpected_argument() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                match syntax.internal_parse_args(
                    ["unexpected", "--param1", "value1", "--param2", "42"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(_) => panic!("should have failed"),
                    Err(e) => assert!(e.to_string().contains("unexpected argument 'unexpected'")),
                }
            }

            #[test]
            fn test_param_default() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--str".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            default: Some("default1".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--int".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            default: Some("42".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--float".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            desc: Some("takes a float".to_string()),
                            default: Some("3.14".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--bool".to_string(),
                            arg_type: SyntaxOptArgType::Boolean,
                            desc: Some("takes a boolean".to_string()),
                            default: Some("true".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--enum".to_string(),
                            arg_type: SyntaxOptArgType::Enum(vec![
                                "a".to_string(),
                                "b".to_string(),
                            ]),
                            desc: Some("takes an enum".to_string()),
                            default: Some("a".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--flag".to_string(),
                            arg_type: SyntaxOptArgType::Flag,
                            desc: Some("takes a flag (default to false)".to_string()),
                            default: Some("false".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--no-flag".to_string(),
                            arg_type: SyntaxOptArgType::Flag,
                            desc: Some("takes a flag (default to true)".to_string()),
                            default: Some("true".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(vec![], vec!["test".to_string()]) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "str int float bool enum flag no_flag"),
                    ("OMNI_ARG_STR_TYPE", "str"),
                    ("OMNI_ARG_STR_VALUE", "default1"),
                    ("OMNI_ARG_INT_TYPE", "int"),
                    ("OMNI_ARG_INT_VALUE", "42"),
                    ("OMNI_ARG_FLOAT_TYPE", "float"),
                    ("OMNI_ARG_FLOAT_VALUE", "3.14"),
                    ("OMNI_ARG_BOOL_TYPE", "bool"),
                    ("OMNI_ARG_BOOL_VALUE", "true"),
                    ("OMNI_ARG_ENUM_TYPE", "str"),
                    ("OMNI_ARG_ENUM_VALUE", "a"),
                    ("OMNI_ARG_FLAG_TYPE", "bool"),
                    ("OMNI_ARG_FLAG_VALUE", "false"),
                    ("OMNI_ARG_NO_FLAG_TYPE", "bool"),
                    ("OMNI_ARG_NO_FLAG_VALUE", "true"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_param_default_array_with_value_delimiter() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--arr-str".to_string(),
                            arg_type: SyntaxOptArgType::ArrayString,
                            default: Some("default1,default2".to_string()),
                            value_delimiter: Some(','),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--arr-int".to_string(),
                            arg_type: SyntaxOptArgType::ArrayInteger,
                            default: Some("42|43|44".to_string()),
                            value_delimiter: Some('|'),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--arr-float".to_string(),
                            arg_type: SyntaxOptArgType::ArrayFloat,
                            default: Some("3.14/2.71".to_string()),
                            value_delimiter: Some('/'),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--arr-bool".to_string(),
                            arg_type: SyntaxOptArgType::ArrayBoolean,
                            default: Some("true%false".to_string()),
                            value_delimiter: Some('%'),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--arr-enum".to_string(),
                            arg_type: SyntaxOptArgType::ArrayEnum(vec![
                                "a".to_string(),
                                "b".to_string(),
                            ]),
                            default: Some("a,b,a,a".to_string()),
                            value_delimiter: Some(','),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(vec![], vec!["test".to_string()]) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    (
                        "OMNI_ARG_LIST",
                        "arr_str arr_int arr_float arr_bool arr_enum",
                    ),
                    ("OMNI_ARG_ARR_STR_TYPE", "str/2"),
                    ("OMNI_ARG_ARR_STR_VALUE_0", "default1"),
                    ("OMNI_ARG_ARR_STR_VALUE_1", "default2"),
                    ("OMNI_ARG_ARR_INT_TYPE", "int/3"),
                    ("OMNI_ARG_ARR_INT_VALUE_0", "42"),
                    ("OMNI_ARG_ARR_INT_VALUE_1", "43"),
                    ("OMNI_ARG_ARR_INT_VALUE_2", "44"),
                    ("OMNI_ARG_ARR_FLOAT_TYPE", "float/2"),
                    ("OMNI_ARG_ARR_FLOAT_VALUE_0", "3.14"),
                    ("OMNI_ARG_ARR_FLOAT_VALUE_1", "2.71"),
                    ("OMNI_ARG_ARR_BOOL_TYPE", "bool/2"),
                    ("OMNI_ARG_ARR_BOOL_VALUE_0", "true"),
                    ("OMNI_ARG_ARR_BOOL_VALUE_1", "false"),
                    ("OMNI_ARG_ARR_ENUM_TYPE", "str/4"),
                    ("OMNI_ARG_ARR_ENUM_VALUE_0", "a"),
                    ("OMNI_ARG_ARR_ENUM_VALUE_1", "b"),
                    ("OMNI_ARG_ARR_ENUM_VALUE_2", "a"),
                    ("OMNI_ARG_ARR_ENUM_VALUE_3", "a"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_param_value_delimiter_on_non_array() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        name: "--param1".to_string(),
                        arg_type: SyntaxOptArgType::String,
                        value_delimiter: Some(','),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(
                    ["--param1", "value1,value2"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "param1"),
                    ("OMNI_ARG_PARAM1_TYPE", "str/2"),
                    ("OMNI_ARG_PARAM1_VALUE_0", "value1"),
                    ("OMNI_ARG_PARAM1_VALUE_1", "value2"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_param_num_values() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            num_values: Some(2),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            num_values: Some(3),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(
                    ["--param1", "value1", "value2", "--param2", "42", "43", "44"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "param1 param2"),
                    ("OMNI_ARG_PARAM1_TYPE", "str/2"),
                    ("OMNI_ARG_PARAM1_VALUE_0", "value1"),
                    ("OMNI_ARG_PARAM1_VALUE_1", "value2"),
                    ("OMNI_ARG_PARAM2_TYPE", "int/3"),
                    ("OMNI_ARG_PARAM2_VALUE_0", "42"),
                    ("OMNI_ARG_PARAM2_VALUE_1", "43"),
                    ("OMNI_ARG_PARAM2_VALUE_2", "44"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_param_last() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "param2".to_string(),
                            arg_type: SyntaxOptArgType::ArrayString,
                            last_arg_double_dash: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(
                    ["value1", "--", "value2", "value3"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "param1 param2"),
                    ("OMNI_ARG_PARAM1_TYPE", "str"),
                    ("OMNI_ARG_PARAM1_VALUE", "value1"),
                    ("OMNI_ARG_PARAM2_TYPE", "str/2"),
                    ("OMNI_ARG_PARAM2_VALUE_0", "value2"),
                    ("OMNI_ARG_PARAM2_VALUE_1", "value3"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_param_last_single_value() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "param2".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            last_arg_double_dash: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = syntax.internal_parse_args(
                    ["value1", "--", "value2", "value3"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                );

                assert_eq!(
                    args,
                    Err("the argument '[param2]' cannot be used multiple times".to_string())
                );
            }

            #[test]
            fn test_param_leftovers() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "param2".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(
                    ["value1", "value2", "value3"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "param1 param2"),
                    ("OMNI_ARG_PARAM1_TYPE", "str"),
                    ("OMNI_ARG_PARAM1_VALUE", "value1"),
                    ("OMNI_ARG_PARAM2_TYPE", "str/2"),
                    ("OMNI_ARG_PARAM2_VALUE_0", "value2"),
                    ("OMNI_ARG_PARAM2_VALUE_1", "value3"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_param_leftovers_allow_hyphens() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "param2".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.internal_parse_args(
                    ["value1", "--value2", "value3"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "param1 param2"),
                    ("OMNI_ARG_PARAM1_TYPE", "str"),
                    ("OMNI_ARG_PARAM1_VALUE", "value1"),
                    ("OMNI_ARG_PARAM2_TYPE", "str/2"),
                    ("OMNI_ARG_PARAM2_VALUE_0", "--value2"),
                    ("OMNI_ARG_PARAM2_VALUE_1", "value3"),
                ];

                assert_eq!(args.len(), expectations.len());
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
            }

            #[test]
            fn test_param_required() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&[], Some("the following required arguments were not provided:   --param1 <param1>")),
                    (&["--param2", "42"], Some("the following required arguments were not provided:   --param1 <param1>")),
                    (&["--param1", "value1"], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_group_multiple() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            desc: Some("takes a float".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param4".to_string(),
                            arg_type: SyntaxOptArgType::Boolean,
                            desc: Some("takes a boolean".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![
                        SyntaxGroup {
                            name: "group1".to_string(),
                            parameters: vec!["--param1".to_string(), "--param2".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group2".to_string(),
                            parameters: vec!["--param3".to_string(), "--param4".to_string()],
                            multiple: true,
                            ..SyntaxGroup::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&[], None),
                    (&["--param1", "value1"], None),
                    (&["--param2", "42"], None),
                    (&["--param3", "3.14"], None),
                    (&["--param4", "true"], None),
                    (&["--param1", "value1", "--param3", "3.14"], None),
                    (&["--param1", "value1", "--param2", "42"], Some("the argument '--param1 <param1>' cannot be used with '--param2 <param2>'")),
                    (&["--param3", "3.14", "--param4", "true"], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_group_required() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            desc: Some("takes a float".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string(), "--param2".to_string()],
                        required: true,
                        ..SyntaxGroup::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&[], Some("the following required arguments were not provided:   <--param1 <param1>|--param2 <param2>>")),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided:   <--param1 <param1>|--param2 <param2>>")),
                    (&["--param1", "value1", "--param3", "3.14"], None),
                    (&["--param2", "42", "--param3", "3.14"], None),
                    (&["--param1", "value1"], None),
                    (&["--param2", "42"], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_group_requires() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![
                        SyntaxGroup {
                            name: "group1".to_string(),
                            parameters: vec!["--param1".to_string()],
                            requires: vec!["param2".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group3".to_string(),
                            parameters: vec!["--param3".to_string()],
                            requires: vec!["group1".to_string()],
                            ..SyntaxGroup::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param2", "42"], None),
                    (
                        &["--param1", "value1"],
                        Some("the following required arguments were not provided:   --param2 <param2>"),
                    ),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided:   <--param1 <param1>>")),
                    (&["--param3", "3.14", "--param2", "42"], Some("the following required arguments were not provided:   <--param1 <param1>>")),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14"], None),
                    (&[], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_group_conflicts_with() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![
                        SyntaxGroup {
                            name: "group1".to_string(),
                            parameters: vec!["--param1".to_string()],
                            conflicts_with: vec!["group2".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group2".to_string(),
                            parameters: vec!["--param2".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group3".to_string(),
                            parameters: vec!["--param3".to_string()],
                            conflicts_with: vec!["param1".to_string()],
                            ..SyntaxGroup::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&[], None),
                    (&["--param1", "value1"], None),
                    (&["--param2", "42"], None),
                    (&["--param3", "3.14"], None),
                    (&["--param2", "42", "--param3", "3.14"], None),
                    (&["--param1", "value1", "--param2", "42"], Some("the argument '--param1 <param1>' cannot be used with '--param2 <param2>'")),
                    (&["--param1", "value1", "--param3", "3.14"], Some("the argument '--param1 <param1>' cannot be used with '--param3 <param3>'")),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_requires() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            requires: vec!["param2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            requires: vec!["group2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param4".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            requires: vec!["param1".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param5".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            requires: vec!["group1".to_string()],
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![
                        SyntaxGroup {
                            name: "group1".to_string(),
                            parameters: vec!["--param1".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group2".to_string(),
                            parameters: vec!["--param2".to_string()],
                            ..SyntaxGroup::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param2", "42"], None),
                    (
                        &["--param1", "value1"],
                        Some("the following required arguments were not provided:   --param2 <param2>"),
                    ),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided:   <--param2 <param2>>")),
                    (&["--param3", "3.14", "--param2", "42"], None),
                    (&["--param4", "10"], Some("the following required arguments were not provided:   --param2 <param2>   --param1 <param1>")),
                    (&["--param4", "10", "--param1", "value1"], Some("the following required arguments were not provided:   --param2 <param2>")),
                    (&["--param4", "10", "--param1", "value1", "--param2", "42"], None),
                    (&["--param5", "20"], Some("the following required arguments were not provided:   <--param1 <param1>>")),
                    (&["--param5", "20", "--param1", "value1"], Some("the following required arguments were not provided:   --param2 <param2>")),
                    (&["--param5", "20", "--param2", "42"], Some("the following required arguments were not provided:   <--param1 <param1>>")),
                    (&["--param5", "20", "--param1", "value1", "--param2", "42"], None),
                    (&[], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_conflicts_with() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            conflicts_with: vec!["param2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            conflicts_with: vec!["group2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![
                        SyntaxGroup {
                            name: "group1".to_string(),
                            parameters: vec!["--param1".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group2".to_string(),
                            parameters: vec!["--param2".to_string()],
                            ..SyntaxGroup::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&[], None),
                    (&["--param1", "value1"], None),
                    (&["--param2", "42"], None),
                    (&["--param3", "3.14"], None),
                    (&["--param1", "value1", "--param3", "3.14"], None),
                    (&["--param1", "value1", "--param2", "42"], Some("the argument '--param1 <param1>' cannot be used with '--param2 <param2>'")),
                    (&["--param2", "42", "--param3", "3.14"], Some("the argument '--param3 <param3>' cannot be used with '--param2 <param2>'")),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14"], Some("the argument '--param1 <param1>' cannot be used with '--param2 <param2>'")),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required_without() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            required_without: vec!["param2".to_string(), "param3".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param4".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            required_without: vec!["group1".to_string()],
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string()],
                        ..SyntaxGroup::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param1", "value1"], None),
                    (&["--param2", "42"], Some("the following required arguments were not provided:   --param4 <param4>")),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided:   --param4 <param4>")),
                    (&["--param2", "42", "--param3", "43"], Some("the following required arguments were not provided:   --param4 <param4>")),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "10"], None),
                    (&[], Some("the following required arguments were not provided:   --param1 <param1>   --param4 <param4>")),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required_without_all() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            required_without_all: vec!["param2".to_string(), "param3".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param4".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            required_without_all: vec!["group5".to_string(), "group2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param5".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    groups: vec![
                        SyntaxGroup {
                            name: "group5".to_string(),
                            parameters: vec!["--param5".to_string()],
                            ..SyntaxGroup::default()
                        },
                        SyntaxGroup {
                            name: "group2".to_string(),
                            parameters: vec!["--param2".to_string()],
                            ..SyntaxGroup::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param1", "value1"], Some("the following required arguments were not provided:   --param4 <param4>")),
                    (&["--param2", "42"], Some("the following required arguments were not provided:   --param1 <param1>   --param4 <param4>")),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided:   --param1 <param1>   --param4 <param4>")),
                    (&["--param2", "42", "--param3", "43"], Some("the following required arguments were not provided:   --param4 <param4>")),
                    (&["--param1", "value1", "--param2", "42"], Some("the following required arguments were not provided:   --param4 <param4>")),
                    (&["--param1", "value1", "--param4", "10"], None),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "10", "--param5", "20"], None),
                    (&["--param2", "42", "--param3", "3.14", "--param5", "20"], None),
                    (&[], Some("the following required arguments were not provided:   --param1 <param1>   --param4 <param4>")),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required_if_eq() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            required_if_eq: HashMap::from_iter(vec![(
                                "param2".to_string(),
                                "42".to_string(),
                            )]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            required_if_eq: HashMap::from_iter(vec![(
                                "param4".to_string(),
                                "true".to_string(),
                            )]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param4".to_string(),
                            arg_type: SyntaxOptArgType::Boolean,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param1", "value1"], None),
                    (&["--param2", "42"], Some("the following required arguments were not provided:   --param1 <param1>")),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param3", "3.14"], None),
                    (&["--param4", "true"], Some("the following required arguments were not provided:   --param3 <param3>")),
                    (&["--param3", "3.14", "--param4", "true"], None),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "true"], None),
                    (&[], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required_if_eq_all() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            name: "--param1".to_string(),
                            arg_type: SyntaxOptArgType::String,
                            required_if_eq_all: HashMap::from_iter(vec![
                                ("param2".to_string(), "42".to_string()),
                                ("param3".to_string(), "3.14".to_string()),
                            ]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param2".to_string(),
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param3".to_string(),
                            arg_type: SyntaxOptArgType::Float,
                            required_if_eq_all: HashMap::from_iter(vec![(
                                "param4".to_string(),
                                "true".to_string(),
                            )]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            name: "--param4".to_string(),
                            arg_type: SyntaxOptArgType::Boolean,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param1", "value1"], None),
                    (&["--param2", "42"], None),
                    (&["--param3", "3.14"], None),
                    (&["--param2", "42", "--param3", "3.14"], Some("the following required arguments were not provided:   --param1 <param1>")),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param1", "value1", "--param3", "3.14"], None),
                    (&["--param1", "value1", "--param4", "true"], Some("the following required arguments were not provided:   --param3 <param3>")),
                    (&["--param3", "3.14"], None),
                    (&["--param4", "true"], Some("the following required arguments were not provided:   --param3 <param3>")),
                    (&["--param3", "3.14", "--param4", "true"], None),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "true"], None),
                    (&[], None),
                ];

                check_expectations(&syntax, &expectations);
            }
        }
    }
}
