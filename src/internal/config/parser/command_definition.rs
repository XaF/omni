use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils as cache_utils;
use crate::internal::commands::utils::str_to_bool;
use crate::internal::commands::HelpCommand;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::ParseArgsErrorKind;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::colors::StringColor;

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
    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let desc = config_value.get_as_str_or_none("desc", &format!("{}.desc", error_key), errors);

        let run = config_value
            .get_as_str_or_none("run", &format!("{}.run", error_key), errors)
            .unwrap_or_else(|| {
                errors.push(ConfigErrorKind::MissingKey {
                    key: format!("{}.run", error_key),
                });
                "true".to_string()
            });

        let aliases =
            config_value.get_as_str_array("aliases", &format!("{}.aliases", error_key), errors);

        let syntax = match config_value.get("syntax") {
            Some(value) => {
                CommandSyntax::from_config_value(&value, &format!("{}.syntax", error_key), errors)
            }
            None => None,
        };

        let category =
            config_value.get_as_str_array("category", &format!("{}.category", error_key), errors);
        let category = if category.is_empty() {
            None
        } else {
            Some(category)
        };

        let dir = config_value.get_as_str_or_none("dir", &format!("{}.dir", error_key), errors);

        let subcommands = match config_value.get("subcommands") {
            Some(value) => {
                let mut subcommands = HashMap::new();
                if let Some(table) = value.as_table() {
                    for (key, value) in table {
                        subcommands.insert(
                            key.to_string(),
                            CommandDefinition::from_config_value(
                                &value,
                                &format!("{}.{}", error_key, key),
                                errors,
                            ),
                        );
                    }
                } else {
                    errors.push(ConfigErrorKind::ValueType {
                        key: format!("{}.subcommands", error_key),
                        found: value.as_serde_yaml(),
                        expected: "table".to_string(),
                    });
                }
                Some(subcommands)
            }
            None => None,
        };

        let argparser = config_value.get_as_bool_or_default(
            "argparser",
            false, // Disable argparser by default
            &format!("{}.argparser", error_key),
            errors,
        );

        Self {
            desc,
            run,
            aliases,
            syntax,
            category,
            dir,
            subcommands,
            argparser,
            source: config_value.get_source().clone(),
            scope: config_value.current_scope().clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct CommandSyntax {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SyntaxOptArg>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<SyntaxGroup>,
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
        if let Some(command_syntax) =
            CommandSyntax::from_config_value(&config_value, "", &mut vec![])
        {
            Ok(command_syntax)
        } else {
            Err(serde::de::Error::custom("invalid command syntax"))
        }
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Option<Self> {
        let mut usage = None;
        let mut parameters = vec![];
        let mut groups = vec![];

        if let Some(array) = config_value.as_array() {
            parameters.extend(array.iter().enumerate().filter_map(|(idx, value)| {
                SyntaxOptArg::from_config_value(
                    value,
                    None,
                    &format!("{}[{}]", error_key, idx),
                    errors,
                )
            }));
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
                            .enumerate()
                            .filter_map(|(idx, value)| {
                                SyntaxOptArg::from_config_value(
                                    value,
                                    required,
                                    &format!("{}.{}[{}]", error_key, key, idx),
                                    errors,
                                )
                            })
                            .collect::<Vec<SyntaxOptArg>>();
                        parameters.extend(arguments);
                    } else if let Some(arg) = SyntaxOptArg::from_config_value(
                        value,
                        required,
                        &format!("{}.{}", error_key, key),
                        errors,
                    ) {
                        parameters.push(arg);
                    } else {
                        errors.push(ConfigErrorKind::ValueType {
                            key: format!("{}.{}", error_key, key),
                            found: value.as_serde_yaml(),
                            expected: "array or table".to_string(),
                        });
                    }
                }
            }

            if let Some(value) = table.get("groups") {
                groups = SyntaxGroup::from_config_value_multi(
                    value,
                    &format!("{}.groups", error_key),
                    errors,
                );
            }

            if let Some(value) = table.get("usage") {
                if let Some(value) = value.as_str_forced() {
                    usage = Some(value.to_string());
                } else {
                    errors.push(ConfigErrorKind::ValueType {
                        key: format!("{}.usage", error_key),
                        found: value.as_serde_yaml(),
                        expected: "string".to_string(),
                    });
                }
            }
        } else if let Some(value) = config_value.as_str_forced() {
            usage = Some(value.to_string());
        } else {
            errors.push(ConfigErrorKind::ValueType {
                key: error_key.to_string(),
                found: config_value.as_serde_yaml(),
                expected: "string, array or table".to_string(),
            });
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
    /// It corresponds to using 'trailing_var_arg' in clap
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
                    .map(|param| param.name().light_yellow())
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
                    .map(|param| param.name().light_yellow())
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
            .filter(|param| param.last_arg_double_hyphen);

        // Check if any is a non-positional argument
        let nonpositional_last = params.clone().filter(|param| !param.is_positional());
        if nonpositional_last.clone().count() > 0 {
            return Err(format!(
                "only positional arguments can use {}; found {}",
                "last".light_yellow(),
                nonpositional_last
                    .map(|param| param.name().light_yellow())
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
                    "{}: counter argument cannot be positional",
                    param.name().light_yellow()
                ));
            }

            if param.num_values.is_some() {
                return Err(format!(
                    "{}: counter argument cannot have a num_values (counters do not take any values)",
                    param.name().light_yellow()
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
                    .keys()
                    .map(|k| sanitize_str(k))
                    .collect::<Vec<_>>()
                    .iter(),
                &available_references,
                "required_if_eq",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param.required_if_eq_all.keys().map(|k| sanitize_str(k)),
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

    /// Allow hyphen values requires that the argument can take a value.
    /// It will thus panic if:
    /// - Set when num_values is set to 0
    /// - Set on a counter
    /// - Set on a flag
    fn check_parameters_allow_hyphen_values(&self) -> Result<(), String> {
        // Grab all the counter params
        let params = self
            .parameters
            .iter()
            .filter(|param| param.allow_hyphen_values);

        for param in params {
            if let Some(SyntaxOptArgNumValues::Exactly(0)) = param.num_values {
                return Err(format!(
                    "{}: cannot use {} with 'num_values=0'",
                    param.name().light_yellow(),
                    "allow_hyphen_values".light_yellow(),
                ));
            }

            match param.arg_type {
                SyntaxOptArgType::Flag | SyntaxOptArgType::Counter => {
                    return Err(format!(
                        "{}: cannot use {} on a {}",
                        param.name().light_yellow(),
                        "allow_hyphen_values".light_yellow(),
                        param.arg_type.to_str(),
                    ))
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Positional parameters have some constraints that could lead the
    /// building of the argument parser to panic:
    /// - If a non-required positional argument appears before a required one
    /// - If a num_values > 1 positional argument appears before a non-required
    ///   one, the latter must have last=true or required=true
    /// - If using num_values=0 or any number of values lower than 1 for a required
    ///   positional argument
    fn check_parameters_positional(&self) -> Result<(), String> {
        let mut prev_positional_with_num_values: Option<String> = None;
        let mut prev_positional_without_required: Option<String> = None;

        for param in self.parameters.iter().filter(|param| param.is_positional()) {
            if !param.required {
                if !param.last_arg_double_hyphen {
                    if let Some(prev) = prev_positional_with_num_values {
                        return Err(format!(
                            "{}: positional need to be required or use '{}' if appearing after {} with num_values > 1",
                            param.name().light_yellow(),
                            "last=true".light_yellow(),
                            prev.light_yellow(),
                        ));
                    }
                }

                if prev_positional_without_required.is_none() {
                    prev_positional_without_required = Some(param.name().clone());
                }
            } else if let Some(prev) = prev_positional_without_required {
                return Err(format!(
                    "{}: required positional argument cannot appear after non-required one {}",
                    param.name().light_yellow(),
                    prev.light_yellow(),
                ));
            } else if let Some(
                SyntaxOptArgNumValues::Exactly(0)
                | SyntaxOptArgNumValues::AtMost(0)
                | SyntaxOptArgNumValues::Between(_, 0),
            ) = param.num_values
            {
                return Err(format!(
                    "{}: positional argument cannot have 'num_values=0'",
                    param.name().light_yellow(),
                ));
            }

            if param.num_values.is_some() && prev_positional_with_num_values.is_none() {
                prev_positional_with_num_values = Some(param.name().clone());
            }
        }

        Ok(())
    }

    /// The flag parameters have some constraints that could lead the
    /// building of the argument parser to panic:
    /// - If a flag has num_values set
    fn check_parameters_flag(&self) -> Result<(), String> {
        for param in self
            .parameters
            .iter()
            .filter(|param| param.arg_type == SyntaxOptArgType::Flag)
        {
            if param.num_values.is_some() {
                return Err(format!(
                    "{}: flag argument cannot have 'num_values' set",
                    param.name().light_yellow(),
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
        self.check_parameters_allow_hyphen_values()?;
        self.check_parameters_positional()?;
        self.check_parameters_flag()?;

        Ok(())
    }

    pub fn argparser(&self, called_as: Vec<String>) -> Result<clap::Command, String> {
        let mut parser = clap::Command::new(called_as.join(" "))
            .disable_help_subcommand(true)
            .disable_version_flag(true);

        self.check_parameters()?;

        for param in &self.parameters {
            parser = param.add_to_argparser(parser);
        }

        for group in &self.groups {
            parser = group.add_to_argparser(parser);
        }

        Ok(parser)
    }

    pub fn parse_args_typed(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> Result<BTreeMap<String, ParseArgsValue>, ParseArgsErrorKind> {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let parser = match self.argparser(called_as.clone()) {
            Ok(parser) => parser,
            Err(err) => {
                return Err(ParseArgsErrorKind::ParserBuildError(err));
            }
        };

        let matches = match parser.try_get_matches_from(&parse_argv) {
            Err(err) => match err.kind() {
                clap::error::ErrorKind::DisplayHelp => {
                    HelpCommand::new().exec_with_exit_code(called_as, 0);
                    unreachable!("help command should have exited");
                }
                clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec_with_exit_code(called_as, 1);
                    unreachable!("help command should have exited");
                }
                clap::error::ErrorKind::DisplayVersion => {
                    unreachable!("version flag is disabled");
                }
                _ => {
                    return Err(ParseArgsErrorKind::ArgumentParsingError(err));
                }
            },
            Ok(matches) => matches,
        };

        let mut args = BTreeMap::new();

        for param in &self.parameters {
            param.add_to_args(&mut args, &matches, None);
        }

        for group in &self.groups {
            group.add_to_args(&mut args, &matches, &self.parameters);
        }

        Ok(args)
    }

    pub fn parse_args(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> Result<BTreeMap<String, String>, ParseArgsErrorKind> {
        let typed_args = self.parse_args_typed(argv, called_as)?;

        let mut args = BTreeMap::new();
        for (key, value) in typed_args {
            value.export_to_env(&key, &mut args);
        }

        let mut all_args = Vec::new();
        for param in &self.parameters {
            all_args.push(param.dest());
        }
        for group in &self.groups {
            all_args.push(group.dest());
        }
        args.insert("OMNI_ARG_LIST".to_string(), all_args.join(" "));

        Ok(args)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SyntaxOptArg {
    #[serde(alias = "name")]
    pub names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub required: bool,
    #[serde(alias = "placeholder", skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<String>,
    #[serde(rename = "type", skip_serializing_if = "SyntaxOptArgType::is_default")]
    pub arg_type: SyntaxOptArgType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_missing_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_values: Option<SyntaxOptArgNumValues>,
    #[serde(rename = "delimiter", skip_serializing_if = "Option::is_none")]
    pub value_delimiter: Option<char>,
    #[serde(rename = "last", skip_serializing_if = "cache_utils::is_false")]
    pub last_arg_double_hyphen: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub leftovers: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub allow_hyphen_values: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub allow_negative_numbers: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub group_occurrences: bool,
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
            names: vec![],
            dest: None,
            desc: None,
            required: false,
            placeholders: vec![],
            arg_type: SyntaxOptArgType::String,
            default: None,
            default_missing_value: None,
            num_values: None,
            value_delimiter: None,
            last_arg_double_hyphen: false,
            leftovers: false,
            allow_hyphen_values: false,
            allow_negative_numbers: false,
            group_occurrences: false,
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
    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        required: Option<bool>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Option<Self> {
        let mut names;
        let mut arg_type;
        let mut placeholders;
        let mut leftovers;

        let mut desc = None;
        let mut dest = None;
        let mut required = required;
        let mut default = None;
        let mut default_missing_value = None;
        let mut num_values = None;
        let mut value_delimiter = None;
        let mut last_arg_double_hyphen = false;
        let mut allow_hyphen_values = false;
        let mut allow_negative_numbers = false;
        let mut group_occurrences = false;
        let mut requires = vec![];
        let mut conflicts_with = vec![];
        let mut required_without = vec![];
        let mut required_without_all = vec![];
        let mut required_if_eq = HashMap::new();
        let mut required_if_eq_all = HashMap::new();

        if let Some(table) = config_value.as_table() {
            let value_for_details;

            if let Some(name_value) = table.get("name") {
                if let Some(name_value) = name_value.as_str() {
                    (names, arg_type, placeholders, leftovers) = parse_arg_name(&name_value);
                    value_for_details = Some(config_value.clone());
                } else {
                    errors.push(ConfigErrorKind::ValueType {
                        key: format!("{}.name", error_key),
                        found: name_value.as_serde_yaml(),
                        expected: "string".to_string(),
                    });
                    return None;
                }
            } else if table.len() == 1 {
                if let Some((key, value)) = table.into_iter().next() {
                    (names, arg_type, placeholders, leftovers) = parse_arg_name(&key);
                    value_for_details = Some(value);
                } else {
                    return None;
                }
            } else {
                errors.push(ConfigErrorKind::MissingKey {
                    key: format!("{}.name", error_key),
                });
                return None;
            }

            if let Some(value_for_details) = value_for_details {
                if let Some(value_str) = value_for_details.as_str() {
                    desc = Some(value_str.to_string());
                } else if let Some(value_table) = value_for_details.as_table() {
                    desc = value_for_details.get_as_str_or_none(
                        "desc",
                        &format!("{}.desc", error_key),
                        errors,
                    );
                    dest = value_for_details.get_as_str_or_none(
                        "dest",
                        &format!("{}.dest", error_key),
                        errors,
                    );

                    if required.is_none() {
                        required = Some(value_for_details.get_as_bool_or_default(
                            "required",
                            false,
                            &format!("{}.required", error_key),
                            errors,
                        ));
                    }

                    // Try to load the placeholders from the placeholders key,
                    // if not found, try to load it from the placeholder key
                    for key in &["placeholders", "placeholder"] {
                        let ph = value_for_details.get_as_str_array(
                            key,
                            &format!("{}.{}", error_key, key),
                            errors,
                        );
                        if !ph.is_empty() {
                            placeholders = ph;
                            break;
                        }
                    }

                    default = value_for_details.get_as_str_or_none(
                        "default",
                        &format!("{}.default", error_key),
                        errors,
                    );
                    default_missing_value = value_for_details.get_as_str_or_none(
                        "default_missing_value",
                        &format!("{}.default_missing_value", error_key),
                        errors,
                    );
                    num_values = SyntaxOptArgNumValues::from_config_value(
                        value_table.get("num_values"),
                        &format!("{}.num_values", error_key),
                        errors,
                    );
                    value_delimiter = value_for_details
                        .get_as_str_or_none(
                            "delimiter",
                            &format!("{}.delimiter", error_key),
                            errors,
                        )
                        .and_then(|value| {
                            value.chars().next().or_else(|| {
                                errors.push(ConfigErrorKind::ValueType {
                                    key: format!("{}.delimiter", error_key),
                                    found: serde_yaml::Value::String(value),
                                    expected: "non-empty string".to_string(),
                                });
                                None
                            })
                        });
                    last_arg_double_hyphen = value_for_details.get_as_bool_or_default(
                        "last",
                        false,
                        &format!("{}.last", error_key),
                        errors,
                    );
                    leftovers = value_for_details.get_as_bool_or_default(
                        "leftovers",
                        false,
                        &format!("{}.leftovers", error_key),
                        errors,
                    );
                    allow_hyphen_values = value_for_details.get_as_bool_or_default(
                        "allow_hyphen_values",
                        false,
                        &format!("{}.allow_hyphen_values", error_key),
                        errors,
                    );
                    allow_negative_numbers = value_for_details.get_as_bool_or_default(
                        "allow_negative_numbers",
                        false,
                        &format!("{}.allow_negative_numbers", error_key),
                        errors,
                    );
                    group_occurrences = value_for_details.get_as_bool_or_default(
                        "group_occurrences",
                        false,
                        &format!("{}.group_occurrences", error_key),
                        errors,
                    );

                    arg_type = SyntaxOptArgType::from_config_value(
                        value_table.get("type"),
                        value_table.get("values"),
                        value_delimiter,
                        &format!("{}.type", error_key),
                        errors,
                    )
                    .unwrap_or(SyntaxOptArgType::String);

                    requires = value_for_details.get_as_str_array(
                        "requires",
                        &format!("{}.requires", error_key),
                        errors,
                    );

                    conflicts_with = value_for_details.get_as_str_array(
                        "conflicts_with",
                        &format!("{}.conflicts_with", error_key),
                        errors,
                    );

                    required_without = value_for_details.get_as_str_array(
                        "required_without",
                        &format!("{}.required_without", error_key),
                        errors,
                    );

                    required_without_all = value_for_details.get_as_str_array(
                        "required_without_all",
                        &format!("{}.required_without_all", error_key),
                        errors,
                    );

                    if let Some(required_if_eq_value) = value_table.get("required_if_eq") {
                        if let Some(value) = required_if_eq_value.as_table() {
                            for (key, value) in value {
                                if let Some(value) = value.as_str_forced() {
                                    required_if_eq.insert(key.to_string(), value.to_string());
                                } else {
                                    errors.push(ConfigErrorKind::ValueType {
                                        key: format!("{}.required_if_eq.{}", error_key, key),
                                        found: value.as_serde_yaml(),
                                        expected: "string".to_string(),
                                    });
                                }
                            }
                        } else {
                            errors.push(ConfigErrorKind::ValueType {
                                key: format!("{}.required_if_eq", error_key),
                                found: required_if_eq_value.as_serde_yaml(),
                                expected: "table".to_string(),
                            });
                        }
                    }

                    if let Some(required_if_eq_all_value) = value_table.get("required_if_eq_all") {
                        if let Some(value) = required_if_eq_all_value.as_table() {
                            for (key, value) in value {
                                if let Some(value) = value.as_str_forced() {
                                    required_if_eq_all.insert(key.to_string(), value.to_string());
                                } else {
                                    errors.push(ConfigErrorKind::ValueType {
                                        key: format!("{}.required_if_eq_all.{}", error_key, key),
                                        found: value.as_serde_yaml(),
                                        expected: "string".to_string(),
                                    });
                                }
                            }
                        } else {
                            errors.push(ConfigErrorKind::ValueType {
                                key: format!("{}.required_if_eq_all", error_key),
                                found: required_if_eq_all_value.as_serde_yaml(),
                                expected: "table".to_string(),
                            });
                        }
                    }

                    let aliases = value_for_details.get_as_str_array(
                        "aliases",
                        &format!("{}.aliases", error_key),
                        errors,
                    );
                    names.extend(aliases);
                }
            }
        } else if let Some(value) = config_value.as_str() {
            (names, arg_type, placeholders, leftovers) = parse_arg_name(&value);
        } else {
            errors.push(ConfigErrorKind::ValueType {
                key: error_key.to_string(),
                found: config_value.as_serde_yaml(),
                expected: "string or table".to_string(),
            });
            return None;
        }

        Some(Self {
            names,
            dest,
            desc,
            required: required.unwrap_or(false),
            placeholders,
            arg_type,
            default,
            default_missing_value,
            num_values,
            value_delimiter,
            last_arg_double_hyphen,
            leftovers,
            allow_hyphen_values,
            allow_negative_numbers,
            group_occurrences,
            requires,
            conflicts_with,
            required_without,
            required_without_all,
            required_if_eq,
            required_if_eq_all,
        })
    }

    pub fn arg_type(&self) -> SyntaxOptArgType {
        let convert_to_array = self.leftovers || self.value_delimiter.is_some();

        if convert_to_array {
            match &self.arg_type {
                SyntaxOptArgType::String
                | SyntaxOptArgType::Integer
                | SyntaxOptArgType::Float
                | SyntaxOptArgType::Boolean
                | SyntaxOptArgType::Enum(_) => {
                    SyntaxOptArgType::Array(Box::new(self.arg_type.clone()))
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
            None => self.name().clone(),
        };

        sanitize_str(&dest)
    }

    fn organized_names(
        &self,
    ) -> (
        String,
        Option<String>,
        Option<String>,
        Vec<String>,
        Vec<String>,
    ) {
        let long_names = self
            .names
            .iter()
            .filter(|name| name.starts_with("--"))
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let (main_long, long_names) = long_names
            .split_first()
            .map(|(f, r)| (Some(f.clone()), r.to_vec()))
            .unwrap_or((None, vec![]));

        let short_names = self
            .names
            .iter()
            .filter(|name| name.starts_with('-') && !name.starts_with("--"))
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let (main_short, short_names) = short_names
            .split_first()
            .map(|(f, r)| (Some(f.clone()), r.to_vec()))
            .unwrap_or((None, vec![]));

        let main = if let Some(main_long) = &main_long {
            main_long.clone()
        } else if let Some(main_short) = &main_short {
            main_short.clone()
        } else {
            self.names
                .first()
                .expect("name should have at least one value")
                .clone()
        };

        (main, main_long, main_short, long_names, short_names)
    }

    pub fn name(&self) -> String {
        let (main, _, _, _, _) = self.organized_names();
        main
    }

    pub fn all_names(&self) -> Vec<String> {
        self.names.clone()
    }

    pub fn is_positional(&self) -> bool {
        !self.name().starts_with('-')
    }

    pub fn takes_value(&self) -> bool {
        if matches!(
            self.arg_type(),
            SyntaxOptArgType::Flag | SyntaxOptArgType::Counter
        ) {
            return false;
        }

        if let Some(SyntaxOptArgNumValues::Exactly(0)) = self.num_values {
            return false;
        }

        true
    }

    /// Returns the representation of that argument for the
    /// 'usage' string in the help message
    pub fn usage(&self) -> String {
        self.help_name(false, true)
    }

    /// Returns the representation of that argument for the help message
    /// This will include:
    /// - For a positional, only the placeholder "num_values" times
    /// - For an optional, the main long and main short, with the placeholder "num_values" times
    ///
    /// The "include_short" parameter influences if the short is shown or not for an optional.
    /// The "use_colors" parameter influences if the output should be colored or not.
    pub fn help_name(&self, include_short: bool, use_colors: bool) -> String {
        let mut help_name = String::new();

        if self.is_positional() {
            let placeholders = if self.placeholders.is_empty() {
                vec![sanitize_str(&self.name()).to_uppercase()]
            } else {
                self.placeholders.clone()
            };

            let placeholders = placeholders
                .iter()
                .map(|ph| {
                    if self.required {
                        format!("<{}>", ph)
                    } else {
                        format!("[{}]", ph)
                    }
                })
                .map(|ph| if use_colors { ph.light_cyan() } else { ph })
                .collect::<Vec<_>>();

            let (min_num, max_num) = match &self.num_values {
                Some(SyntaxOptArgNumValues::Exactly(n)) => (*n, Some(*n)),
                Some(SyntaxOptArgNumValues::AtLeast(min)) => (*min, None),
                Some(SyntaxOptArgNumValues::AtMost(max)) => (0, Some(*max)),
                Some(SyntaxOptArgNumValues::Any) => (0, None),
                Some(SyntaxOptArgNumValues::Between(min, max)) => (*min, Some(*max)),
                None => (1, Some(1)),
            };

            // Get the max between min and 1
            let min_placeholders = std::cmp::max(min_num, 1);
            let repr = placeholders
                .iter()
                .cycle()
                .take(min_placeholders)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");

            // If the max is None or greater than min, or if the arg type is an array
            // we need to add "..." to the end
            let repr =
                if self.arg_type().is_array() || max_num.is_none() || max_num.unwrap() > min_num {
                    format!("{}...", repr)
                } else {
                    repr
                };

            help_name.push_str(&repr);
        } else {
            // Split the short and long names, and only keep the first of each (return Option<_>)
            let all_names = self.all_names();
            let (short_name, long_name): (Vec<_>, Vec<_>) =
                all_names.iter().partition(|name| !name.starts_with("--"));
            let short_name = short_name.first();
            let long_name = long_name.first();

            if include_short || long_name.is_none() {
                if let Some(short_name) = short_name {
                    let short_name = if use_colors {
                        short_name.bold().light_cyan()
                    } else {
                        short_name.to_string()
                    };
                    help_name.push_str(&short_name);

                    if long_name.is_some() {
                        help_name.push_str(", ");
                    }
                }
            }
            if let Some(long_name) = long_name {
                let long_name = if use_colors {
                    long_name.bold().light_cyan()
                } else {
                    long_name.to_string()
                };
                help_name.push_str(&long_name);
            }

            if self.takes_value() {
                let placeholders = if self.placeholders.is_empty() {
                    vec![sanitize_str(&self.name()).to_uppercase()]
                } else {
                    self.placeholders.clone()
                };

                let (min_num, max_num) = match &self.num_values {
                    Some(SyntaxOptArgNumValues::Exactly(n)) => (*n, Some(*n)),
                    Some(SyntaxOptArgNumValues::AtLeast(min)) => (*min, None),
                    Some(SyntaxOptArgNumValues::AtMost(max)) => (0, Some(*max)),
                    Some(SyntaxOptArgNumValues::Any) => (0, None),
                    Some(SyntaxOptArgNumValues::Between(min, max)) => (*min, Some(*max)),
                    None => (1, Some(1)),
                };

                let repr = match (min_num, max_num) {
                    (0, Some(0)) => "".to_string(),
                    (1, Some(1)) => {
                        let repr = format!(
                            "<{}>",
                            placeholders
                                .first()
                                .expect("there should be at least one placeholder")
                        );
                        if use_colors {
                            repr.light_cyan()
                        } else {
                            repr
                        }
                    }
                    (min, Some(max)) if min == max => {
                        // Placeholders can be N elements, e.g. A, B, C
                        // We want to go over placeholders for M values, e.g. A B C A B C if M > N,
                        // or A B C if M == N, or A B if M < N
                        placeholders
                            .iter()
                            .cycle()
                            .take(min)
                            .map(|repr| format!("<{}>", repr))
                            .map(|repr| if use_colors { repr.light_cyan() } else { repr })
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                    (0, Some(1)) => {
                        let repr = format!(
                            "[{}]",
                            placeholders
                                .first()
                                .expect("there should be at least one placeholder")
                        );
                        if use_colors {
                            repr.light_cyan()
                        } else {
                            repr
                        }
                    }
                    (0, _) => {
                        let repr = format!(
                            "[{}]",
                            placeholders
                                .first()
                                .expect("there should be at least one placeholder")
                        );
                        let repr = if use_colors { repr.light_cyan() } else { repr };
                        format!("{}...", repr)
                    }
                    (min, _) => {
                        let repr = placeholders
                            .iter()
                            .cycle()
                            .take(min)
                            .map(|repr| format!("<{}>", repr))
                            .map(|repr| if use_colors { repr.light_cyan() } else { repr })
                            .collect::<Vec<_>>()
                            .join(" ");
                        format!("{}...", repr)
                    }
                };

                if !repr.is_empty() {
                    help_name.push(' ');
                    help_name.push_str(&repr);
                }
            } else if matches!(self.arg_type, SyntaxOptArgType::Counter) {
                help_name.push_str("...");
            }
        }

        help_name
    }

    /// Returns the description of that argument for the help message
    pub fn help_desc(&self) -> String {
        let mut help_desc = String::new();

        // Add the description if any
        if let Some(desc) = &self.desc {
            help_desc.push_str(desc);
        }

        // Add the default value if any
        if !matches!(self.arg_type, SyntaxOptArgType::Flag) {
            if let Some(default) = &self.default {
                if !default.is_empty() {
                    if !help_desc.is_empty() {
                        help_desc.push(' ');
                    }
                    help_desc
                        .push_str(&format!("[{}: {}]", "default".italic(), default).light_black());
                }
            }

            if let Some(default_missing_value) = &self.default_missing_value {
                if !default_missing_value.is_empty() {
                    if !help_desc.is_empty() {
                        help_desc.push(' ');
                    }
                    help_desc.push_str(
                        &format!(
                            "[{}: {}]",
                            "default missing value".italic(),
                            default_missing_value
                        )
                        .light_black(),
                    );
                }
            }
        }

        // Add the possible values if any
        if let Some(possible_values) = self.arg_type().possible_values() {
            if !help_desc.is_empty() {
                help_desc.push(' ');
            }
            help_desc.push_str(
                &format!(
                    "[{}: {}]",
                    "possible values".italic(),
                    possible_values.join(", ")
                )
                .light_black(),
            );
        }

        // Add the aliases if any
        let (_, _, _, long_aliases, short_aliases) = self.organized_names();

        if !long_aliases.is_empty() {
            if !help_desc.is_empty() {
                help_desc.push(' ');
            }

            help_desc.push_str(
                &format!("[{}: {}]", "aliases".italic(), long_aliases.join(", ")).light_black(),
            );
        }

        if !short_aliases.is_empty() {
            if !help_desc.is_empty() {
                help_desc.push(' ');
            }

            help_desc.push_str(
                &format!(
                    "[{}: {}]",
                    "short aliases".italic(),
                    short_aliases.join(", ")
                )
                .light_black(),
            );
        }

        help_desc
    }

    pub fn add_to_argparser(&self, parser: clap::Command) -> clap::Command {
        let mut arg = clap::Arg::new(self.dest());

        // Add the help for the argument
        if let Some(desc) = &self.desc {
            arg = arg.help(desc);
        }

        // Add all the names for that argument
        if !self.is_positional() {
            let (_, main_long, main_short, long_names, short_names) = self.organized_names();

            if let Some(main_long) = &main_long {
                if sanitize_str(main_long).is_empty() {
                    // TODO: raise error ?
                    return parser;
                }

                let long = main_long.trim_start_matches("-").to_string();
                arg = arg.long(long);
            }

            if let Some(main_short) = &main_short {
                if sanitize_str(main_short).is_empty() {
                    // TODO: raise error ?
                    return parser;
                }

                let short = main_short
                    .trim_start_matches("-")
                    .chars()
                    .next()
                    .expect("short name should have at least one character");
                arg = arg.short(short);
            }

            for long_name in &long_names {
                if sanitize_str(long_name).is_empty() {
                    continue;
                }

                let long = long_name.trim_start_matches("-").to_string();
                arg = arg.visible_alias(long);
            }

            for short_name in &short_names {
                if sanitize_str(short_name).is_empty() {
                    continue;
                }

                let short = short_name
                    .trim_start_matches("-")
                    .chars()
                    .next()
                    .expect("short name should have at least one character");
                arg = arg.visible_short_alias(short);
            }
        }

        // Set the placeholder if any
        if !self.placeholders.is_empty() {
            let placeholders = match &self.num_values {
                Some(n) => match n.max() {
                    Some(max) => self
                        .placeholders
                        .iter()
                        .cycle()
                        .take(max)
                        .map(|ph| ph.to_string())
                        .collect::<Vec<_>>(),
                    None => self.placeholders.clone(),
                },
                None => self.placeholders.clone(),
            };
            arg = arg.value_names(placeholders);
        }

        // Set the default value
        if let Some(default) = &self.default {
            arg = arg.default_value(default);
        }

        // Set the default missing value
        if let Some(default_missing_value) = &self.default_missing_value {
            arg = arg.default_missing_value(default_missing_value);
        }

        // Set how to parse the values
        if let Some(num_values) = &self.num_values {
            arg = arg.num_args(*num_values);
        }
        if let Some(value_delimiter) = &self.value_delimiter {
            arg = arg.value_delimiter(*value_delimiter);
        }
        if self.last_arg_double_hyphen {
            arg = arg.last(true);
        }
        if self.leftovers {
            arg = arg.trailing_var_arg(true);
        }
        if self.allow_hyphen_values {
            arg = arg.allow_hyphen_values(true);
        }
        if self.allow_negative_numbers {
            arg = arg.allow_negative_numbers(true);
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
            SyntaxOptArgType::Array(_) => {
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
        match &self.arg_type().terminal_type() {
            SyntaxOptArgType::Integer => {
                arg = arg.value_parser(clap::value_parser!(i64));
            }
            SyntaxOptArgType::Float => {
                arg = arg.value_parser(clap::value_parser!(f64));
            }
            SyntaxOptArgType::Boolean => {
                arg = arg.value_parser(clap::value_parser!(bool));
            }
            SyntaxOptArgType::Enum(possible_values) => {
                arg = arg.value_parser(possible_values.clone());
            }
            _ => {}
        }

        parser.arg(arg)
    }

    pub fn add_to_args(
        &self,
        args: &mut BTreeMap<String, ParseArgsValue>,
        matches: &clap::ArgMatches,
        override_dest: Option<String>,
    ) {
        let dest = self.dest();

        // has_occurrences is when an argument can take multiple values
        let has_occurrences = self
            .num_values
            .as_ref()
            .map_or(false, |num_values| num_values.is_many());

        // has_multi is when an argument can be called multiple times
        let has_multi = self.arg_type().is_array();

        match &self.arg_type().terminal_type() {
            SyntaxOptArgType::String | SyntaxOptArgType::Enum(_) => {
                extract_value_to_typed::<String>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                );
            }
            SyntaxOptArgType::Integer => {
                extract_value_to_typed::<i64>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                );
            }
            SyntaxOptArgType::Counter => {
                extract_value_to_typed::<u8>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                );
            }
            SyntaxOptArgType::Float => {
                extract_value_to_typed::<f64>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                );
            }
            SyntaxOptArgType::Boolean | SyntaxOptArgType::Flag => {
                let default = Some(
                    str_to_bool(&self.default.clone().unwrap_or_default())
                        .unwrap_or(false)
                        .to_string(),
                );
                extract_value_to_typed::<bool>(
                    matches,
                    &dest,
                    &default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                );
            }
            SyntaxOptArgType::Array(_) => unreachable!("array type should be handled differently"),
        };
    }
}

trait ParserExtractType<T> {
    type BaseType;
    type Output;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output;
}

impl<T: Into<ParseArgsValue> + Clone + FromStr + Send + Sync + 'static> ParserExtractType<T>
    for Option<T>
{
    type BaseType = T;
    type Output = Option<T>;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output {
        match (matches.get_one::<T>(dest), default) {
            (Some(value), _) => Some(value.clone()),
            (None, Some(default)) => match default.parse::<T>() {
                Ok(value) => Some(value),
                Err(_) => None,
            },
            _ => None,
        }
    }
}

impl<T: Into<ParseArgsValue> + Clone + FromStr + Send + Sync + 'static> ParserExtractType<T>
    for Vec<Option<T>>
{
    type BaseType = T;
    type Output = Vec<Option<T>>;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output {
        match (matches.get_many::<T>(dest), default) {
            (Some(values), _) => values
                .collect::<Vec<_>>()
                .into_iter()
                .map(|value| Some(value.clone()))
                .collect(),
            (None, Some(default)) => default
                .split(',')
                .flat_map(|part| part.trim().parse::<T>())
                .map(|value| Some(value.clone()))
                .collect(),
            _ => vec![],
        }
    }
}

impl<T: Into<ParseArgsValue> + Clone + FromStr + Send + Sync + 'static> ParserExtractType<T>
    for Vec<Vec<Option<T>>>
{
    type BaseType = T;
    type Output = Vec<Vec<Option<T>>>;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output {
        match (matches.get_occurrences(dest), default) {
            (Some(occurrences), _) => occurrences
                .into_iter()
                .map(|values| {
                    values
                        .into_iter()
                        .map(|value: &T| Some(value.clone()))
                        .collect()
                })
                .collect(),
            (None, Some(default)) => vec![default
                .split(',')
                .flat_map(|part| part.trim().parse::<T>().map(|value| Some(value.clone())))
                .collect()],
            _ => vec![],
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn extract_value_to_typed<T>(
    matches: &clap::ArgMatches,
    dest: &str,
    default: &Option<String>,
    args: &mut BTreeMap<String, ParseArgsValue>,
    override_dest: Option<String>,
    has_occurrences: bool,
    has_multi: bool,
    group_occurrences: bool,
) where
    // W: ParserExtractType<T>,
    T: Into<ParseArgsValue> + Clone + Send + Sync + FromStr + 'static,
    ParseArgsValue: From<Option<T>>,
    ParseArgsValue: From<Vec<Option<T>>>,
    ParseArgsValue: From<Vec<Vec<Option<T>>>>,
{
    let arg_dest = override_dest.unwrap_or(dest.to_string());

    let value = if has_occurrences && has_multi && group_occurrences {
        let value = <Vec<Vec<Option<T>>> as ParserExtractType<T>>::extract(matches, dest, default);
        ParseArgsValue::from(value)
    } else if has_multi || has_occurrences {
        let value = <Vec<Option<T>> as ParserExtractType<T>>::extract(matches, dest, default);
        ParseArgsValue::from(value)
    } else {
        let value = <Option<T> as ParserExtractType<T>>::extract(matches, dest, default);
        ParseArgsValue::from(value)
    };

    args.insert(arg_dest, value);
}

pub fn parse_arg_name(arg_name: &str) -> (Vec<String>, SyntaxOptArgType, Vec<String>, bool) {
    let mut names = Vec::new();
    let mut arg_type = SyntaxOptArgType::String;
    let mut placeholders = vec![];
    let mut leftovers = false;

    // Parse the argument name; it can be a single name or multiple names separated by commas.
    // There can be short names (starting with `-`) and long names (starting with `--`).
    // Each name can have one or more placeholders, or the placeholders can be put at the end.
    // The placeholders are separated by a space from the name, and by a space from each other.
    // If the argument name does not start with `-`, only this value will be kept as part of
    // the names and the others will be ignored.
    let def_parts: Vec<&str> = arg_name.split(',').map(str::trim).collect();

    for part in def_parts {
        let name_parts = part.splitn(2, [' ', '\t', '=']).collect::<Vec<&str>>();
        if name_parts.is_empty() {
            continue;
        }

        let name = name_parts[0];
        let (name, ends_with_dots) = if name.ends_with("...") {
            (name.trim_end_matches("..."), true)
        } else {
            (name, false)
        };

        if name.starts_with('-') {
            if name_parts.len() > 1 {
                placeholders.extend(
                    name_parts[1]
                        .split_whitespace()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<String>>(),
                );
            }

            if ends_with_dots {
                // If the name ends with `...`, we consider it a counter
                arg_type = SyntaxOptArgType::Counter;
            }

            names.push(name.to_string());
        } else {
            names.clear();
            names.push(name.to_string());

            if ends_with_dots {
                // If the name ends with `...`, we consider it as a last argument
                leftovers = true;
            }

            if name_parts.len() > 1 {
                placeholders.push(
                    name_parts[1]
                        .split_whitespace()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                );
            }

            // If we have a parameter without a leading `-`, we stop parsing
            // the rest of the arg name since this is a positional argument
            break;
        }
    }

    (names, arg_type, placeholders, leftovers)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Copy)]
pub enum SyntaxOptArgNumValues {
    Any,
    Exactly(usize),
    AtLeast(usize),
    AtMost(usize),
    Between(usize, usize),
}

impl fmt::Display for SyntaxOptArgNumValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, ".."),
            Self::Exactly(value) => write!(f, "{}", value),
            Self::AtLeast(min) => write!(f, "{}..", min),
            Self::AtMost(max) => write!(f, "..={}", max),
            Self::Between(min, max) => write!(f, "{}..={}", min, max),
        }
    }
}

impl From<SyntaxOptArgNumValues> for clap::builder::ValueRange {
    fn from(val: SyntaxOptArgNumValues) -> Self {
        match val {
            SyntaxOptArgNumValues::Any => clap::builder::ValueRange::from(..),
            SyntaxOptArgNumValues::Exactly(value) => clap::builder::ValueRange::from(value),
            SyntaxOptArgNumValues::AtLeast(min) => clap::builder::ValueRange::from(min..),
            SyntaxOptArgNumValues::AtMost(max) => clap::builder::ValueRange::from(..=max),
            SyntaxOptArgNumValues::Between(min, max) => clap::builder::ValueRange::from(min..=max),
        }
    }
}

impl From<std::ops::RangeToInclusive<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeToInclusive<usize>) -> Self {
        let max = range.end;
        Self::AtMost(max)
    }
}

impl From<std::ops::RangeTo<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeTo<usize>) -> Self {
        let max = range.end;
        Self::AtMost(max - 1)
    }
}

impl From<std::ops::RangeFrom<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeFrom<usize>) -> Self {
        let min = range.start;
        Self::AtLeast(min)
    }
}

impl From<std::ops::RangeInclusive<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeInclusive<usize>) -> Self {
        let (min, max) = range.into_inner();
        Self::Between(min, max)
    }
}

impl From<std::ops::Range<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::Range<usize>) -> Self {
        let (min, max) = (range.start, range.end);
        Self::Between(min, max)
    }
}

impl From<std::ops::RangeFull> for SyntaxOptArgNumValues {
    fn from(_: std::ops::RangeFull) -> Self {
        Self::Any
    }
}

impl From<usize> for SyntaxOptArgNumValues {
    fn from(value: usize) -> Self {
        Self::Exactly(value)
    }
}

impl SyntaxOptArgNumValues {
    pub fn from_str(
        value: &str,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Option<Self> {
        let value = value.trim();

        if value.contains("..") {
            let mut parts = value.split("..");

            let min = parts.next()?.trim();
            let max = parts.next()?.trim();
            let (max, max_inclusive) = if let Some(max) = max.strip_prefix('=') {
                (max, true)
            } else {
                (max, false)
            };

            let max = match max {
                "" => None,
                value => match value.parse::<usize>() {
                    Ok(value) => Some(value),
                    Err(_) => {
                        errors.push(ConfigErrorKind::ValueType {
                            key: error_key.to_string(),
                            found: serde_yaml::Value::String(value.to_string()),
                            expected: "positive integer".to_string(),
                        });
                        return None;
                    }
                },
            };

            let min = match min {
                "" => None,
                value => match value.parse::<usize>() {
                    Ok(value) => Some(value),
                    Err(_) => {
                        errors.push(ConfigErrorKind::ValueType {
                            key: error_key.to_string(),
                            found: serde_yaml::Value::String(value.to_string()),
                            expected: "positive integer".to_string(),
                        });
                        return None;
                    }
                },
            };

            match (min, max, max_inclusive) {
                (None, None, _) => Some(Self::Any),
                (None, Some(max), true) => Some(Self::AtMost(max)),
                (None, Some(max), false) => {
                    if max > 0 {
                        Some(Self::AtMost(max - 1))
                    } else {
                        errors.push(ConfigErrorKind::InvalidRange {
                            key: error_key.to_string(),
                            min: 0,
                            max: 0,
                        });
                        None
                    }
                }
                (Some(min), None, _) => Some(Self::AtLeast(min)),
                (Some(min), Some(max), true) => {
                    if min <= max {
                        Some(Self::Between(min, max))
                    } else {
                        errors.push(ConfigErrorKind::InvalidRange {
                            key: error_key.to_string(),
                            min: min,
                            max: max + 1,
                        });
                        None
                    }
                }
                (Some(min), Some(max), false) => {
                    if min < max {
                        Some(Self::Between(min, max - 1))
                    } else {
                        errors.push(ConfigErrorKind::InvalidRange {
                            key: error_key.to_string(),
                            min: min,
                            max: max,
                        });
                        None
                    }
                }
            }
        } else {
            let value = match value.parse::<usize>() {
                Ok(value) => Some(value),
                Err(_) => {
                    errors.push(ConfigErrorKind::ValueType {
                        key: error_key.to_string(),
                        found: serde_yaml::Value::String(value.to_string()),
                        expected: "positive integer".to_string(),
                    });
                    None
                }
            }?;
            Some(Self::Exactly(value))
        }
    }

    fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Option<Self> {
        let config_value = config_value?;

        if let Some(value) = config_value.as_integer() {
            Some(Self::Exactly(value as usize))
        } else if let Some(value) = config_value.as_str_forced() {
            Self::from_str(&value, error_key, errors)
        } else {
            errors.push(ConfigErrorKind::ValueType {
                key: error_key.to_string(),
                found: config_value.as_serde_yaml(),
                expected: "string or integer".to_string(),
            });
            None
        }
    }

    fn is_many(&self) -> bool {
        match self {
            Self::Any => true,
            Self::Exactly(value) => *value > 1,
            Self::AtLeast(_min) => true, // AtLeast is always many since it is not bounded by a maximum
            Self::AtMost(max) => *max > 1,
            Self::Between(_min, max) => *max > 1,
        }
    }

    fn max(&self) -> Option<usize> {
        match self {
            Self::Any => None,
            Self::Exactly(value) => Some(*value),
            Self::AtLeast(_min) => None,
            Self::AtMost(max) => Some(*max),
            Self::Between(_min, max) => Some(*max),
        }
    }
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
    #[serde(rename = "array")]
    Array(Box<SyntaxOptArgType>),
}

impl fmt::Display for SyntaxOptArgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl SyntaxOptArgType {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "int",
            Self::Float => "float",
            Self::Boolean => "bool",
            Self::Flag => "flag",
            Self::Counter => "counter",
            Self::Enum(_) => "enum",
            Self::Array(inner) => match **inner {
                Self::String => "array/str",
                Self::Integer => "array/int",
                Self::Float => "array/float",
                Self::Boolean => "array/bool",
                Self::Enum(_) => "array/enum",
                _ => unimplemented!("unsupported array type: {:?}", self),
            },
        }
    }

    fn from_config_value(
        config_value_type: Option<&ConfigValue>,
        config_value_values: Option<&ConfigValue>,
        value_delimiter: Option<char>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Option<Self> {
        let config_value_type = config_value_type?;

        let obj = Self::from_str(
            &config_value_type.as_str_forced().or_else(|| {
                errors.push(ConfigErrorKind::ValueType {
                    key: error_key.to_string(),
                    found: config_value_type.as_serde_yaml(),
                    expected: "string".to_string(),
                });
                None
            })?,
            error_key,
            errors,
        )?;

        match obj {
            Self::Enum(values) if values.is_empty() => {
                if let Some(values) = config_value_values {
                    if let Some(array) = values.as_array() {
                        let values = array
                            .iter()
                            .filter_map(|value| value.as_str_forced())
                            .collect::<Vec<String>>();
                        return Some(Self::Enum(values));
                    } else if let Some(value) = values.as_str_forced() {
                        if let Some(value_delimiter) = value_delimiter {
                            let values = value
                                .split(value_delimiter)
                                .map(|value| value.to_string())
                                .collect::<Vec<String>>();
                            return Some(Self::Enum(values));
                        } else {
                            return Some(Self::Enum(vec![value.to_string()]));
                        }
                    }
                }
            }
            _ => return Some(obj),
        }

        None
    }

    pub fn from_str(
        value: &str,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Option<Self> {
        let mut is_array = false;

        let normalized = value.trim().to_lowercase();
        let mut value = normalized.trim();

        if value.starts_with("array/") {
            value = &value[6..];
            is_array = true;
        } else if value.starts_with("[") && value.ends_with("]") {
            value = &value[1..value.len() - 1];
            is_array = true;
        } else if value == "array" {
            return Some(Self::Array(Box::new(Self::String)));
        }

        let obj = match value.to_lowercase().as_str() {
            "int" | "integer" => Self::Integer,
            "float" => Self::Float,
            "bool" | "boolean" => Self::Boolean,
            "flag" => Self::Flag,
            "count" | "counter" => Self::Counter,
            "str" | "string" => Self::String,
            "enum" => Self::Enum(vec![]),
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
                        .filter(|value| !value.is_empty())
                        .collect::<Vec<String>>();

                    Self::Enum(values)
                } else {
                    errors.push(ConfigErrorKind::InvalidValue {
                        key: error_key.to_string(),
                        found: serde_yaml::Value::String(value.to_string()),
                        expected: vec![
                            "int".to_string(),
                            "float".to_string(),
                            "bool".to_string(),
                            "flag".to_string(),
                            "count".to_string(),
                            "str".to_string(),
                            "enum or enum(xx, yy, zz)".to_string(),
                            "array/<type>".to_string(),
                        ],
                    });

                    return None;
                }
            }
        };

        if is_array {
            Some(Self::Array(Box::new(obj)))
        } else {
            Some(obj)
        }
    }

    pub fn terminal_type(&self) -> &Self {
        match self {
            Self::Array(inner) => inner.terminal_type(),
            _ => self,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    pub fn possible_values(&self) -> Option<Vec<String>> {
        match self.terminal_type() {
            Self::Enum(values) => Some(values.clone()),
            Self::Boolean => Some(vec!["true".to_string(), "false".to_string()]),
            _ => None,
        }
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
    pub(super) fn from_config_value_multi(
        config_value: &ConfigValue,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Vec<Self> {
        let mut groups = vec![];

        if let Some(array) = config_value.as_array() {
            // If this is an array, we can simply iterate over it and create the groups
            for (idx, value) in array.iter().enumerate() {
                if let Some(group) = Self::from_config_value(
                    &value,
                    None,
                    &format!("{}[{}]", error_key, idx),
                    errors,
                ) {
                    groups.push(group);
                }
            }
        } else if let Some(table) = config_value.as_table() {
            // If this is a table, we need to iterate over the keys and create the groups
            for (name, value) in table {
                if let Some(group) = Self::from_config_value(
                    &value,
                    Some(name.to_string()),
                    &format!("{}.{}", error_key, name),
                    errors,
                ) {
                    groups.push(group);
                }
            }
        } else {
            errors.push(ConfigErrorKind::ValueType {
                key: error_key.to_string(),
                found: config_value.as_serde_yaml(),
                expected: "array or table".to_string(),
            });
        }

        groups
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        name: Option<String>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Option<Self> {
        // Exit early if the value is not a table
        let table = if let Some(table) = config_value.as_table() {
            // Exit early if the table is empty
            if table.is_empty() {
                errors.push(ConfigErrorKind::MissingKey {
                    key: format!("{}.name", error_key),
                });
                errors.push(ConfigErrorKind::MissingKey {
                    key: format!("{}.parameters", error_key),
                });
                return None;
            }
            table
        } else {
            errors.push(ConfigErrorKind::ValueType {
                key: error_key.to_string(),
                found: config_value.as_serde_yaml(),
                expected: "table".to_string(),
            });
            return None;
        };

        let mut config_value = config_value;
        let mut error_key = error_key.to_string();

        // Handle the group name
        let name = match name {
            Some(name) => name,
            None => {
                if table.len() == 1 {
                    // Extract the only key from the table, this will be the name of the group
                    let key = table.keys().next().unwrap().to_string();

                    // Change the config to be the value of the key, this will be the group's config
                    config_value = table.get(&key)?;
                    error_key = format!("{}.{}", error_key, key);

                    // Exit early if the value is not a table
                    if !config_value.is_table() {
                        errors.push(ConfigErrorKind::ValueType {
                            key: error_key,
                            found: config_value.as_serde_yaml(),
                            expected: "table".to_string(),
                        });
                        return None;
                    }

                    // Return the key as the name of the group
                    key
                } else if let Some(name) = config_value.get("name") {
                    if let Some(name) = name.as_str_forced() {
                        name.to_string()
                    } else {
                        errors.push(ConfigErrorKind::ValueType {
                            key: format!("{}.name", error_key),
                            found: name.as_serde_yaml(),
                            expected: "string".to_string(),
                        });
                        return None;
                    }
                } else {
                    errors.push(ConfigErrorKind::MissingKey {
                        key: format!("{}.name", error_key),
                    });
                    return None;
                }
            }
        };

        // Handle the group parameters
        let parameters = config_value.get_as_str_array(
            "parameters",
            &format!("{}.parameters", error_key),
            errors,
        );
        // No parameters, skip this group
        if parameters.is_empty() {
            errors.push(ConfigErrorKind::MissingKey {
                key: format!("{}.parameters", error_key),
            });
            return None;
        }

        // Parse the rest of the group configuration
        let multiple = config_value.get_as_bool_or_default(
            "multiple",
            false,
            &format!("{}.multiple", error_key),
            errors,
        );

        let required = config_value.get_as_bool_or_default(
            "required",
            false,
            &format!("{}.required", error_key),
            errors,
        );

        let requires =
            config_value.get_as_str_array("requires", &format!("{}.requires", error_key), errors);

        let conflicts_with = config_value.get_as_str_array(
            "conflicts_with",
            &format!("{}.conflicts_with", error_key),
            errors,
        );

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
        args: &mut BTreeMap<String, ParseArgsValue>,
        matches: &clap::ArgMatches,
        parameters: &[SyntaxOptArg],
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
                            names: vec!["--param1".to_string()],
                            dest: Some("paramdest".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
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
                            names: vec!["--param1".to_string(), "--param2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                        names: vec!["--param1".to_string()],
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
                            names: vec!["param1".to_string()],
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
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
                            names: vec!["param1".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param3".to_string()],
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
                            names: vec!["param1".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
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
                        names: vec!["--param1".to_string()],
                        last_arg_double_hyphen: true,
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
                        names: vec!["param1".to_string()],
                        arg_type: SyntaxOptArgType::Counter,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "param1: counter argument cannot be positional";
                assert_eq!(syntax.check_parameters_counter(), Err(errmsg.to_string()));
            }

            #[test]
            fn test_num_values() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Counter,
                        num_values: Some(SyntaxOptArgNumValues::Exactly(1)),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "--param1: counter argument cannot have a num_values (counters do not take any values)";
                assert_eq!(syntax.check_parameters_counter(), Err(errmsg.to_string()));
            }
        }

        mod check_parameters_allow_hyphen_values {
            use super::*;

            #[test]
            fn test_num_values() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        num_values: Some(SyntaxOptArgNumValues::Exactly(0)),
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "--param1: cannot use allow_hyphen_values with 'num_values=0'";
                assert_eq!(
                    syntax.check_parameters_allow_hyphen_values(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_counter() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Counter,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "--param1: cannot use allow_hyphen_values on a counter";
                assert_eq!(
                    syntax.check_parameters_allow_hyphen_values(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_flag() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Flag,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "--param1: cannot use allow_hyphen_values on a flag";
                assert_eq!(
                    syntax.check_parameters_allow_hyphen_values(),
                    Err(errmsg.to_string())
                );
            }
        }

        mod check_parameters_positional {
            use super::*;

            #[test]
            fn test_positional_required_before_non_required() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let errmsg =
                    "param2: required positional argument cannot appear after non-required one param1";
                assert_eq!(
                    syntax.check_parameters_positional(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_positional_num_values() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let errmsg = "param2: positional need to be required or use 'last=true' if appearing after param1 with num_values > 1";
                assert_eq!(
                    syntax.check_parameters_positional(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_positional_num_values_ok_if_required() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                assert_eq!(syntax.check_parameters_positional(), Ok(()));
            }

            #[test]
            fn test_positional_num_values_ok_if_last() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            last_arg_double_hyphen: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                assert_eq!(syntax.check_parameters_positional(), Ok(()));
            }

            #[test]
            fn test_positional_required_num_values_exactly_zero() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        required: true,
                        num_values: Some(SyntaxOptArgNumValues::Exactly(0)),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "param1: positional argument cannot have 'num_values=0'";
                assert_eq!(
                    syntax.check_parameters_positional(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_positional_required_num_values_at_most_zero() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        required: true,
                        num_values: Some(SyntaxOptArgNumValues::AtMost(0)),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "param1: positional argument cannot have 'num_values=0'";
                assert_eq!(
                    syntax.check_parameters_positional(),
                    Err(errmsg.to_string())
                );
            }

            #[test]
            fn test_positional_required_num_values_between_max_zero() {
                disable_colors();

                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        required: true,
                        num_values: Some(SyntaxOptArgNumValues::Between(0, 0)),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let errmsg = "param1: positional argument cannot have 'num_values=0'";
                assert_eq!(
                    syntax.check_parameters_positional(),
                    Err(errmsg.to_string())
                );
            }
        }

        mod parse_args {
            use super::*;

            fn check_expectations(
                syntax: &CommandSyntax,
                expectations: &Vec<(&[&str], Option<&str>)>,
            ) {
                for (argv, expectation) in expectations {
                    let parsed_args = syntax.parse_args(
                        argv.iter().map(|s| s.to_string()).collect(),
                        vec!["test".to_string()],
                    );
                    match &expectation {
                        Some(errmsg) => match &parsed_args {
                            Ok(_args) => {
                                panic!("case with args {:?} should have failed but succeeded", argv)
                            }
                            Err(e) => assert_eq!((argv, e.simple()), (argv, errmsg.to_string())),
                        },
                        None => {
                            if let Err(ref e) = parsed_args {
                                panic!("case with args {:?} should have succeeded but failed with error: {}", argv, e);
                            }
                        }
                    }
                }
            }

            fn check_type_expectations(
                arg_name: &str,
                arg_type: &str,
                syntax: &CommandSyntax,
                expectations: &Vec<(Vec<&str>, Result<&str, &str>)>,
            ) {
                for (argv, expectation) in expectations {
                    let args = match syntax.parse_args(
                        argv.iter().map(|s| s.to_string()).collect(),
                        vec!["test".to_string()],
                    ) {
                        Ok(args) => {
                            if expectation.is_err() {
                                panic!("{:?} should have failed", argv)
                            }
                            args
                        }
                        Err(e) => {
                            if let Err(expect_err) = &expectation {
                                assert_eq!((&argv, e.simple()), (&argv, expect_err.to_string()));
                                continue;
                            }
                            panic!("{:?} should have succeeded, instead: {}", argv, e);
                        }
                    };

                    let value = expectation.expect("should not get here if not Ok");

                    let type_var = format!("OMNI_ARG_{}_TYPE", arg_name.to_uppercase());
                    let value_var = format!("OMNI_ARG_{}_VALUE", arg_name.to_uppercase());

                    let mut expectations = vec![("OMNI_ARG_LIST", arg_name), (&type_var, arg_type)];
                    if !value.is_empty() {
                        expectations.push((&value_var, value));
                    }

                    let expectations_len = expectations.len();
                    for (key, value) in expectations {
                        assert_eq!(
                            (&argv, key, args.get(key)),
                            (&argv, key, Some(&value.to_string()))
                        );
                    }
                    assert_eq!((&argv, args.len()), (&argv, expectations_len));
                }
            }

            #[test]
            fn test_simple() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(
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
            fn test_value_string() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                    (vec!["--param1", "value1"], Ok("value1")),
                    (vec!["--param1", ""], Ok("")),
                    (vec!["--param1", "1"], Ok("1")),
                    (vec!["--param1", "value1,value2"], Ok("value1,value2")),
                ];

                check_type_expectations("param1", "str", &syntax, &expectations);
            }

            #[test]
            fn test_value_int() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                    (vec!["--param1", "1"], Ok("1")),
                    (vec!["--param1", "10"], Ok("10")),
                    (vec!["--param1", "0"], Ok("0")),
                    (vec!["--param1", "-100"], Ok("-100")),
                    (vec!["--param1", ""], Err("invalid value '' for '--param1 <param1>': cannot parse integer from empty string")),
                    (vec!["--param1", "1.2"], Err("invalid value '1.2' for '--param1 <param1>': invalid digit found in string")),
                    (vec!["--param1", "1,2"], Err("invalid value '1,2' for '--param1 <param1>': invalid digit found in string")),
                ];

                check_type_expectations("param1", "int", &syntax, &expectations);
            }

            #[test]
            fn test_value_float() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                    (vec!["--param1", "1.978326"], Ok("1.978326")),
                    (vec!["--param1", "10"], Ok("10")),
                    (vec!["--param1", "0"], Ok("0")),
                    (vec!["--param1", "-100.4"], Ok("-100.4")),
                    (vec!["--param1", ""], Err("invalid value '' for '--param1 <param1>': cannot parse float from empty string")),
                    (vec!["--param1", "1.2"], Ok("1.2")),
                    (vec!["--param1", "1,2"], Err("invalid value '1,2' for '--param1 <param1>': invalid float literal")),
                ];

                check_type_expectations("param1", "float", &syntax, &expectations);
            }

            #[test]
            fn test_value_bool() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Boolean,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                    (vec!["--param1", "true"], Ok("true")),
                    (vec!["--param1", "false"], Ok("false")),
                    (vec!["--param1", ""], Err("a value is required for '--param1 <param1>' but none was supplied [possible values: true, false]")),
                    (vec!["--param1", "TRUE"], Err("invalid value 'TRUE' for '--param1 <param1>' [possible values: true, false]")),
                    (vec!["--param1", "no"], Err("invalid value 'no' for '--param1 <param1>' [possible values: true, false]")),
                    (vec!["--param1", "1"], Err("invalid value '1' for '--param1 <param1>' [possible values: true, false]")),
                    (vec!["--param1", "0"], Err("invalid value '0' for '--param1 <param1>' [possible values: true, false]")),
                    (vec!["--param1", "on"], Err("invalid value 'on' for '--param1 <param1>' [possible values: true, false]")),
                    (vec!["--param1", "off"], Err("invalid value 'off' for '--param1 <param1>' [possible values: true, false]")),
                ];

                check_type_expectations("param1", "bool", &syntax, &expectations);
            }

            #[test]
            fn test_value_enum() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Enum(vec![
                            "a".to_string(),
                            "b".to_string(),
                            "c".to_string(),
                        ]),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                    (vec!["--param1", "a"], Ok("a")),
                    (vec!["--param1", "b"], Ok("b")),
                    (vec!["--param1", "c"], Ok("c")),
                    (vec!["--param1", "d"], Err("invalid value 'd' for '--param1 <param1>' [possible values: a, b, c]")),
                    (vec!["--param1", ""], Err("a value is required for '--param1 <param1>' but none was supplied [possible values: a, b, c]")),
                ];

                check_type_expectations("param1", "str", &syntax, &expectations);
            }

            #[test]
            fn test_value_flag() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Flag,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                    (vec!["--param1"], Ok("true")),
                    (vec![], Ok("false")),
                    (vec!["--param1", "c"], Err("unexpected argument 'c' found")),
                    (vec!["--param1", ""], Err("unexpected argument '' found")),
                ];

                check_type_expectations("param1", "bool", &syntax, &expectations);
            }

            #[test]
            fn test_value_counter() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--count".to_string(), "-c".to_string()],
                        arg_type: SyntaxOptArgType::Counter,
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                    (vec![], Ok("0")),
                    (vec!["--count"], Ok("1")),
                    (vec!["--count", "--count"], Ok("2")),
                    (vec!["-c", "-c", "-c"], Ok("3")),
                    (vec!["-cc", "-c"], Ok("3")),
                    (vec!["-ccc"], Ok("3")),
                    (
                        vec!["--count", "blah"],
                        Err("unexpected argument 'blah' found"),
                    ),
                    (vec!["--count", ""], Err("unexpected argument '' found")),
                ];

                check_type_expectations("count", "int", &syntax, &expectations);
            }

            #[test]
            fn test_unexpected_argument() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                match syntax.parse_args(
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
                            names: vec!["--str".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            default: Some("default1".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--int".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            default: Some("42".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--float".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            desc: Some("takes a float".to_string()),
                            default: Some("3.14".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--bool".to_string()],
                            arg_type: SyntaxOptArgType::Boolean,
                            desc: Some("takes a boolean".to_string()),
                            default: Some("true".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--enum".to_string()],
                            arg_type: SyntaxOptArgType::Enum(vec![
                                "a".to_string(),
                                "b".to_string(),
                            ]),
                            desc: Some("takes an enum".to_string()),
                            default: Some("a".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--flag".to_string()],
                            arg_type: SyntaxOptArgType::Flag,
                            desc: Some("takes a flag (default to false)".to_string()),
                            default: Some("false".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--no-flag".to_string()],
                            arg_type: SyntaxOptArgType::Flag,
                            desc: Some("takes a flag (default to true)".to_string()),
                            default: Some("true".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(vec![], vec!["test".to_string()]) {
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
                            names: vec!["--arr-str".to_string()],
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                            default: Some("default1,default2".to_string()),
                            value_delimiter: Some(','),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--arr-int".to_string()],
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Integer)),
                            default: Some("42|43|44".to_string()),
                            value_delimiter: Some('|'),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--arr-float".to_string()],
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Float)),
                            default: Some("3.14/2.71".to_string()),
                            value_delimiter: Some('/'),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--arr-bool".to_string()],
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Boolean)),
                            default: Some("true%false".to_string()),
                            value_delimiter: Some('%'),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--arr-enum".to_string()],
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Enum(
                                vec!["a".to_string(), "b".to_string()],
                            ))),
                            default: Some("a,b,a,a".to_string()),
                            value_delimiter: Some(','),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(vec![], vec!["test".to_string()]) {
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

                let expect_len = expectations.len();
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
                assert_eq!(args.len(), expect_len);
            }

            #[test]
            fn test_param_default_missing_value() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--str".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                            default_missing_value: Some("default1".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--int".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                            default_missing_value: Some("42".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--float".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            desc: Some("takes a float".to_string()),
                            num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                            default_missing_value: Some("3.14".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--bool".to_string()],
                            arg_type: SyntaxOptArgType::Boolean,
                            desc: Some("takes a boolean".to_string()),
                            num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                            default_missing_value: Some("true".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--enum".to_string()],
                            arg_type: SyntaxOptArgType::Enum(vec![
                                "a".to_string(),
                                "b".to_string(),
                            ]),
                            desc: Some("takes an enum".to_string()),
                            num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                            default_missing_value: Some("a".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let argv = ["--str", "--int", "--float", "--bool", "--enum"];

                let args = match syntax.parse_args(
                    argv.iter().map(|s| s.to_string()).collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "str int float bool enum"),
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
                ];

                let expectations_len = expectations.len();
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
                assert_eq!(args.len(), expectations_len);
            }

            #[test]
            fn test_param_value_delimiter_on_non_array() {
                let syntax = CommandSyntax {
                    parameters: vec![SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        value_delimiter: Some(','),
                        ..SyntaxOptArg::default()
                    }],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(
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
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            num_values: Some(SyntaxOptArgNumValues::Exactly(3)),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(
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
            fn test_param_num_values_at_most() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--exactly".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--at-most-3".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            num_values: Some(SyntaxOptArgNumValues::AtMost(3)),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--at-least-2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            num_values: Some(SyntaxOptArgNumValues::AtLeast(2)),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--between-2-4".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            num_values: Some(SyntaxOptArgNumValues::Between(2, 4)),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--any".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            num_values: Some(SyntaxOptArgNumValues::Any),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let test_cases: Vec<(Vec<&str>, Option<&str>)> = vec![
                    (vec!["--exactly"], Some("a value is required for '--exactly <exactly> <exactly>' but none was supplied")),
                    (vec!["--exactly", "1"], Some("2 values required for '--exactly <exactly> <exactly>' but 1 was provided")),
                    (vec!["--exactly", "1", "2"], None),
                    (vec!["--exactly", "1", "2", "3"], Some("unexpected argument '3' found")),
                    (vec!["--at-most-3"], None),
                    (vec!["--at-most-3", "1"], None),
                    (vec!["--at-most-3", "1", "2"], None),
                    (vec!["--at-most-3", "1", "2", "3"], None),
                    (vec!["--at-most-3", "1", "2", "3", "4"], Some("unexpected argument '4' found")),
                    (vec!["--at-least-2"], Some("a value is required for '--at-least-2 <at_least_2> <at_least_2>...' but none was supplied")),
                    (vec!["--at-least-2", "1"], Some("2 values required by '--at-least-2 <at_least_2> <at_least_2>...'; only 1 was provided")),
                    (vec!["--at-least-2", "1", "2"], None),
                    (vec!["--at-least-2", "1", "2", "3"], None),
                    (vec!["--at-least-2", "1", "2", "3", "4"], None),
                    (vec!["--between-2-4"], Some("a value is required for '--between-2-4 <between_2_4> <between_2_4>...' but none was supplied")),
                    (vec!["--between-2-4", "1"], Some("2 values required by '--between-2-4 <between_2_4> <between_2_4>...'; only 1 was provided")),
                    (vec!["--between-2-4", "1", "2"], None),
                    (vec!["--between-2-4", "1", "2", "3"], None),
                    (vec!["--between-2-4", "1", "2", "3", "4"], None),
                    (vec!["--between-2-4", "1", "2", "3", "4", "5"], Some("unexpected argument '5' found")),
                    (vec!["--any"], None),
                    (vec!["--any", "1"], None),
                    (vec!["--any", "1", "2", "3", "4", "5", "6", "7", "8", "9"], None),
                ];

                for (i, (argv, error)) in test_cases.iter().enumerate() {
                    match syntax.parse_args(
                        argv.iter().map(|s| s.to_string()).collect(),
                        vec!["test".to_string()],
                    ) {
                        Ok(args) => {
                            if error.is_some() {
                                panic!(
                                    "case {} with argv {:?} should have failed, instead: {:?}",
                                    i, argv, args
                                );
                            }

                            let mut expectations = vec![(
                                "OMNI_ARG_LIST".to_string(),
                                "exactly at_most_3 at_least_2 between_2_4 any".to_string(),
                            )];

                            let params = &[
                                ("--exactly", "exactly"),
                                ("--at-most-3", "at_most_3"),
                                ("--at-least-2", "at_least_2"),
                                ("--between-2-4", "between_2_4"),
                                ("--any", "any"),
                            ];

                            for (param, env_name) in params {
                                // Get the position of the parameter in argv
                                let pos = argv.iter().position(|s| s == param);
                                let values = match pos {
                                    Some(pos) => {
                                        // Take all values until the next value with --
                                        argv.iter()
                                            .skip(pos + 1)
                                            .take_while(|s| !s.starts_with("--"))
                                            .collect::<Vec<_>>()
                                    }
                                    None => vec![],
                                };

                                // Add the type and values to the expectations
                                let type_var = format!("OMNI_ARG_{}_TYPE", env_name.to_uppercase());
                                expectations.push((type_var, format!("int/{}", values.len())));

                                for (i, value) in values.iter().enumerate() {
                                    let value_var =
                                        format!("OMNI_ARG_{}_VALUE_{}", env_name.to_uppercase(), i);
                                    expectations.push((value_var, value.to_string()));
                                }
                            }

                            // Validate that the expectations are met
                            let expect_len = expectations.len();
                            for (key, value) in expectations {
                                assert_eq!(
                                    (&argv, &key, args.get(&key)),
                                    (&argv, &key, Some(&value.to_string()))
                                );
                            }
                            assert_eq!((&argv, args.len()), (&argv, expect_len));
                        }
                        Err(e) => {
                            if let Some(errmsg) = error {
                                assert_eq!(e.simple(), errmsg.to_string());
                                continue;
                            }
                            panic!(
                                "case {} with argv {:?} should have succeeded, instead: {}",
                                i, argv, e
                            );
                        }
                    }
                }
            }

            #[test]
            fn test_param_group_occurrences() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--group".to_string()],
                            num_values: Some(SyntaxOptArgNumValues::AtLeast(1)),
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                            group_occurrences: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--no-group".to_string()],
                            num_values: Some(SyntaxOptArgNumValues::AtLeast(1)),
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let argv = vec![
                    "--group",
                    "group1.1",
                    "group1.2",
                    "--no-group",
                    "no-group1.1",
                    "no-group1.2",
                    "--group",
                    "group2.1",
                    "--no-group",
                    "no-group2.1",
                    "--group",
                    "group3.1",
                    "group3.2",
                    "group3.3",
                    "--no-group",
                    "no-group3.1",
                    "no-group3.2",
                    "no-group3.3",
                ];

                let args = match syntax.parse_args(
                    argv.iter().map(|s| s.to_string()).collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => args,
                    Err(e) => panic!("{}", e),
                };

                let expectations = vec![
                    ("OMNI_ARG_LIST", "group no_group"),
                    ("OMNI_ARG_GROUP_TYPE", "str/3/3"),
                    ("OMNI_ARG_GROUP_TYPE_0", "str/2"),
                    ("OMNI_ARG_GROUP_VALUE_0_0", "group1.1"),
                    ("OMNI_ARG_GROUP_VALUE_0_1", "group1.2"),
                    ("OMNI_ARG_GROUP_TYPE_1", "str/1"),
                    ("OMNI_ARG_GROUP_VALUE_1_0", "group2.1"),
                    ("OMNI_ARG_GROUP_TYPE_2", "str/3"),
                    ("OMNI_ARG_GROUP_VALUE_2_0", "group3.1"),
                    ("OMNI_ARG_GROUP_VALUE_2_1", "group3.2"),
                    ("OMNI_ARG_GROUP_VALUE_2_2", "group3.3"),
                    ("OMNI_ARG_NO_GROUP_TYPE", "str/6"),
                    ("OMNI_ARG_NO_GROUP_VALUE_0", "no-group1.1"),
                    ("OMNI_ARG_NO_GROUP_VALUE_1", "no-group1.2"),
                    ("OMNI_ARG_NO_GROUP_VALUE_2", "no-group2.1"),
                    ("OMNI_ARG_NO_GROUP_VALUE_3", "no-group3.1"),
                    ("OMNI_ARG_NO_GROUP_VALUE_4", "no-group3.2"),
                    ("OMNI_ARG_NO_GROUP_VALUE_5", "no-group3.3"),
                ];

                eprintln!("{:?}", args);

                let expectations_len = expectations.len();
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
                assert_eq!(args.len(), expectations_len);
            }

            #[test]
            fn test_param_last() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                            last_arg_double_hyphen: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(
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
                            names: vec!["param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            last_arg_double_hyphen: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = syntax.parse_args(
                    ["value1", "--", "value2", "value3"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                );

                match args {
                    Ok(_) => panic!("should have failed"),
                    Err(e) => assert_eq!(
                        e.simple(),
                        "the argument '[param2]' cannot be used multiple times"
                    ),
                }
            }

            #[test]
            fn test_param_leftovers() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(
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
            fn test_param_leftovers_no_allow_hyphens() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            leftovers: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = syntax.parse_args(
                    ["value1", "--value2", "value3"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    vec!["test".to_string()],
                );

                match args {
                    Ok(_) => panic!("should have failed"),
                    Err(e) => assert_eq!(e.simple(), "unexpected argument '--value2' found"),
                }
            }

            #[test]
            fn test_param_leftovers_allow_hyphens() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["param2".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            leftovers: true,
                            allow_hyphen_values: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(
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
            fn test_param_allow_negative_numbers() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            allow_negative_numbers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            allow_negative_numbers: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let args = match syntax.parse_args(
                    ["--param1", "-42", "--param2", "-3.14"]
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
                    ("OMNI_ARG_PARAM1_TYPE", "int"),
                    ("OMNI_ARG_PARAM1_VALUE", "-42"),
                    ("OMNI_ARG_PARAM2_TYPE", "float"),
                    ("OMNI_ARG_PARAM2_VALUE", "-3.14"),
                ];

                let expectations_len = expectations.len();
                for (key, value) in expectations {
                    assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
                }
                assert_eq!(args.len(), expectations_len);
            }

            #[test]
            fn test_param_allow_negative_numbers_scenarios() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            allow_negative_numbers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            allow_negative_numbers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            allow_hyphen_values: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param4".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            allow_hyphen_values: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param5".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            allow_negative_numbers: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param6".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            allow_hyphen_values: true,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param1", "42"], None),
                    (&["--param2", "3.14"], None),
                    (&["--param3", "42"], None),
                    (&["--param4", "3.14"], None),
                    (&["--param5", "42"], None),
                    (&["--param6", "3.14"], None),
                    (&["--param1", "-42"], None),
                    (&["--param2", "-3.14"], None),
                    (&["--param3", "-42"], None),
                    (&["--param4", "-3.14"], None),
                    (&["--param5", "-42"], None),
                    (&["--param6", "-3.14"], None),
                    (
                        &["--param1", "-blah"],
                        Some("unexpected argument '-b' found"),
                    ),
                    (
                        &["--param2", "-blah"],
                        Some("unexpected argument '-b' found"),
                    ),
                    (
                        &["--param3", "-blah"],
                        Some("invalid value '-blah' for '--param3 <param3>': invalid digit found in string"),
                    ),
                    (
                        &["--param4", "-blah"],
                        Some("invalid value '-blah' for '--param4 <param4>': invalid float literal"),
                    ),
                    (
                        &["--param5", "-blah"],
                        Some("unexpected argument '-b' found"),
                    ),
                    (&["--param6", "-blah"], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            required: true,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (
                        &[],
                        Some(
                            "the following required arguments were not provided: --param1 <param1>",
                        ),
                    ),
                    (
                        &["--param2", "42"],
                        Some(
                            "the following required arguments were not provided: --param1 <param1>",
                        ),
                    ),
                    (&["--param1", "value1"], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_group_multiple() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            desc: Some("takes a float".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param4".to_string()],
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
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            desc: Some("takes an int".to_string()),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
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
                    (&[], Some("the following required arguments were not provided: <--param1 <param1>|--param2 <param2>>")),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided: <--param1 <param1>|--param2 <param2>>")),
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
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
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
                        Some("the following required arguments were not provided: --param2 <param2>"),
                    ),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided: <--param1 <param1>>")),
                    (&["--param3", "3.14", "--param2", "42"], Some("the following required arguments were not provided: <--param1 <param1>>")),
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
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
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
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            requires: vec!["param2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            requires: vec!["group2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param4".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            requires: vec!["param1".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param5".to_string()],
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
                        Some("the following required arguments were not provided: --param2 <param2>"),
                    ),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided: <--param2 <param2>>")),
                    (&["--param3", "3.14", "--param2", "42"], None),
                    (&["--param4", "10"], Some("the following required arguments were not provided: --param2 <param2> --param1 <param1>")),
                    (&["--param4", "10", "--param1", "value1"], Some("the following required arguments were not provided: --param2 <param2>")),
                    (&["--param4", "10", "--param1", "value1", "--param2", "42"], None),
                    (&["--param5", "20"], Some("the following required arguments were not provided: <--param1 <param1>>")),
                    (&["--param5", "20", "--param1", "value1"], Some("the following required arguments were not provided: --param2 <param2>")),
                    (&["--param5", "20", "--param2", "42"], Some("the following required arguments were not provided: <--param1 <param1>>")),
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
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            conflicts_with: vec!["param2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
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
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            required_without: vec!["param2".to_string(), "param3".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param4".to_string()],
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
                    (&["--param2", "42"], Some("the following required arguments were not provided: --param4 <param4>")),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided: --param4 <param4>")),
                    (&["--param2", "42", "--param3", "43"], Some("the following required arguments were not provided: --param4 <param4>")),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "10"], None),
                    (&[], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required_without_all() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            required_without_all: vec!["param2".to_string(), "param3".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            required: false,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param4".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            required_without_all: vec!["group5".to_string(), "group2".to_string()],
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param5".to_string()],
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
                    (&["--param1", "value1"], Some("the following required arguments were not provided: --param4 <param4>")),
                    (&["--param2", "42"], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
                    (&["--param3", "3.14"], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
                    (&["--param2", "42", "--param3", "43"], Some("the following required arguments were not provided: --param4 <param4>")),
                    (&["--param1", "value1", "--param2", "42"], Some("the following required arguments were not provided: --param4 <param4>")),
                    (&["--param1", "value1", "--param4", "10"], None),
                    (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "10", "--param5", "20"], None),
                    (&["--param2", "42", "--param3", "3.14", "--param5", "20"], None),
                    (&[], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required_if_eq() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            required_if_eq: HashMap::from_iter(vec![(
                                "param2".to_string(),
                                "42".to_string(),
                            )]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            required_if_eq: HashMap::from_iter(vec![(
                                "param4".to_string(),
                                "true".to_string(),
                            )]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param4".to_string()],
                            arg_type: SyntaxOptArgType::Boolean,
                            ..SyntaxOptArg::default()
                        },
                    ],
                    ..CommandSyntax::default()
                };

                let expectations: Vec<(&[&str], Option<&str>)> = vec![
                    (&["--param1", "value1"], None),
                    (
                        &["--param2", "42"],
                        Some(
                            "the following required arguments were not provided: --param1 <param1>",
                        ),
                    ),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param3", "3.14"], None),
                    (
                        &["--param4", "true"],
                        Some(
                            "the following required arguments were not provided: --param3 <param3>",
                        ),
                    ),
                    (&["--param3", "3.14", "--param4", "true"], None),
                    (
                        &[
                            "--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4",
                            "true",
                        ],
                        None,
                    ),
                    (&[], None),
                ];

                check_expectations(&syntax, &expectations);
            }

            #[test]
            fn test_param_required_if_eq_all() {
                let syntax = CommandSyntax {
                    parameters: vec![
                        SyntaxOptArg {
                            names: vec!["--param1".to_string()],
                            arg_type: SyntaxOptArgType::String,
                            required_if_eq_all: HashMap::from_iter(vec![
                                ("param2".to_string(), "42".to_string()),
                                ("param3".to_string(), "3.14".to_string()),
                            ]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param2".to_string()],
                            arg_type: SyntaxOptArgType::Integer,
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param3".to_string()],
                            arg_type: SyntaxOptArgType::Float,
                            required_if_eq_all: HashMap::from_iter(vec![(
                                "param4".to_string(),
                                "true".to_string(),
                            )]),
                            ..SyntaxOptArg::default()
                        },
                        SyntaxOptArg {
                            names: vec!["--param4".to_string()],
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
                    (
                        &["--param2", "42", "--param3", "3.14"],
                        Some(
                            "the following required arguments were not provided: --param1 <param1>",
                        ),
                    ),
                    (&["--param1", "value1", "--param2", "42"], None),
                    (&["--param1", "value1", "--param3", "3.14"], None),
                    (
                        &["--param1", "value1", "--param4", "true"],
                        Some(
                            "the following required arguments were not provided: --param3 <param3>",
                        ),
                    ),
                    (&["--param3", "3.14"], None),
                    (
                        &["--param4", "true"],
                        Some(
                            "the following required arguments were not provided: --param3 <param3>",
                        ),
                    ),
                    (&["--param3", "3.14", "--param4", "true"], None),
                    (
                        &[
                            "--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4",
                            "true",
                        ],
                        None,
                    ),
                    (&[], None),
                ];

                check_expectations(&syntax, &expectations);
            }
        }
    }

    mod parse_arg_name {
        use super::*;

        #[test]
        fn test_simple_positional() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("arg");
            assert_eq!(names, vec!["arg"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert!(placeholders.is_empty());
            assert!(!leftovers);
        }

        #[test]
        fn test_short_option() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("-a");
            assert_eq!(names, vec!["-a"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert!(placeholders.is_empty());
            assert!(!leftovers);
        }

        #[test]
        fn test_long_option() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("--option");
            assert_eq!(names, vec!["--option"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert!(placeholders.is_empty());
            assert!(!leftovers);
        }

        #[test]
        fn test_multiple_names() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("-a, --alpha");
            assert_eq!(names, vec!["-a", "--alpha"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert!(placeholders.is_empty());
            assert!(!leftovers);
        }

        #[test]
        fn test_counter_option() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("--count...");
            assert_eq!(names, vec!["--count"]);
            assert_eq!(arg_type, SyntaxOptArgType::Counter);
            assert!(placeholders.is_empty());
            assert!(!leftovers);
        }

        #[test]
        fn test_positional_with_placeholder() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("arg PLACEHOLDER");
            assert_eq!(names, vec!["arg"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["PLACEHOLDER"]);
            assert!(!leftovers);
        }

        #[test]
        fn test_option_with_placeholder() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("--option VALUE");
            assert_eq!(names, vec!["--option"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["VALUE"]);
            assert!(!leftovers);
        }

        #[test]
        fn test_multiple_placeholders() {
            let (names, arg_type, placeholders, leftovers) =
                parse_arg_name("--option FIRST SECOND");
            assert_eq!(names, vec!["--option"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["FIRST", "SECOND"]);
            assert!(!leftovers);
        }

        #[test]
        fn test_leftovers_positional() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("args...");
            assert_eq!(names, vec!["args"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert!(placeholders.is_empty());
            assert!(leftovers);
        }

        #[test]
        fn test_multiple_names_with_placeholder_at_the_end() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("-f, --file FILENAME");
            assert_eq!(names, vec!["-f", "--file"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["FILENAME"]);
            assert!(!leftovers);
        }

        #[test]
        fn test_multiple_names_with_placeholders_for_each() {
            let (names, arg_type, placeholders, leftovers) =
                parse_arg_name("-f FILENAME1, --file FILENAME2");
            assert_eq!(names, vec!["-f", "--file"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["FILENAME1", "FILENAME2"]);
            assert!(!leftovers);
        }

        #[test]
        fn test_equals_separator() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("--option=VALUE");
            assert_eq!(names, vec!["--option"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["VALUE"]);
            assert!(!leftovers);
        }

        #[test]
        fn test_empty_input() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("");
            assert_eq!(names, vec![""]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert!(placeholders.is_empty());
            assert!(!leftovers);
        }

        #[test]
        fn test_whitespace_handling() {
            let (names, arg_type, placeholders, leftovers) = parse_arg_name("  --option  VALUE  ");
            assert_eq!(names, vec!["--option"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["VALUE"]);
            assert!(!leftovers);
        }

        #[test]
        fn test_multiple_names_whitespace() {
            let (names, arg_type, placeholders, leftovers) =
                parse_arg_name("-f,   --file,  -F  FILENAME");
            assert_eq!(names, vec!["-f", "--file", "-F"]);
            assert_eq!(arg_type, SyntaxOptArgType::String);
            assert_eq!(placeholders, vec!["FILENAME"]);
            assert!(!leftovers);
        }
    }
}
