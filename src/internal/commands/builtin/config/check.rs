use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::exit;

use itertools::Itertools;
use serde::Serialize;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::frompath::PathCommand;
use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::commands::Command;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::utils::check_allowed;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigLoader;
use crate::internal::config::ConfigScope;
use crate::internal::config::OmniConfig;
use crate::internal::config::SyntaxGroup;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::omnipath_env;
use crate::internal::git::is_path_gitignored;
use crate::internal::git::package_root_path;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;

#[derive(Debug, Clone)]
struct ConfigCheckCommandArgs {
    search_paths: HashSet<String>,
    config_files: HashSet<String>,
    ignore_errors: HashSet<String>,
    select_errors: HashSet<String>,
    patterns: Vec<String>,
    include_packages: bool,
    global_scope: bool,
    local_scope: bool,
    default_scope: bool,
}

impl From<BTreeMap<String, ParseArgsValue>> for ConfigCheckCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let search_paths = match args.get("search_path") {
            Some(ParseArgsValue::ManyString(search_paths)) => {
                search_paths.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let config_files = match args.get("config_file") {
            Some(ParseArgsValue::ManyString(config_files)) => {
                config_files.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let ignore_errors = match args.get("ignore") {
            Some(ParseArgsValue::ManyString(ignore_errors)) => {
                ignore_errors.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let select_errors = match args.get("select") {
            Some(ParseArgsValue::ManyString(select_errors)) => {
                select_errors.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let patterns = match args.get("pattern") {
            Some(ParseArgsValue::ManyString(patterns)) => {
                patterns.iter().flat_map(|v| v.clone()).collect()
            }
            _ => Vec::new(),
        };

        let include_packages = matches!(
            args.get("include_packages"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );

        let global_scope = matches!(
            args.get("global"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let local_scope = matches!(
            args.get("local"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let default_scope = !global_scope && !local_scope;

        Self {
            search_paths,
            config_files,
            ignore_errors,
            select_errors,
            patterns,
            include_packages,
            global_scope,
            local_scope,
            default_scope,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigCheckCommand {}

impl ConfigCheckCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for ConfigCheckCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["config".to_string(), "check".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Check the configuration files and commands in the path for errors\n",
                "\n",
                "This allows to report any error or potential error in the ",
                "configuration, or in any metadata for commands in the omnipath.\n",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["-P".to_string(), "--search-path".to_string()],
                    desc: Some(
                        concat!(
                            "Path to check for commands.\n",
                            "\n",
                            "Can be used multiple times. If not passed, ",
                            "worktree-defined paths are used if in a worktree, ",
                            "or the omnipath otherwise.\n",
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-C".to_string(), "--config-file".to_string()],
                    desc: Some(
                        concat!(
                            "Configuration file to check.\n",
                            "\n",
                            "Can be used multiple times. If not passed, ",
                            "the default configuration files loaded by omni ",
                            "are checked.\n",
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--global".to_string()],
                    desc: Some(
                        "Check the global configuration files and omnipath only.".to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--local".to_string()],
                    desc: Some(
                        "Check the local configuration files and omnipath only.".to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--ignore".to_string()],
                    desc: Some("Error codes to ignore".to_string()),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    value_delimiter: Some(','),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--select".to_string()],
                    desc: Some("Error codes to select".to_string()),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    value_delimiter: Some(','),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--pattern".to_string()],
                    desc: Some(
                        concat!(
                            "Pattern of files to include (or exclude, if starting ",
                            "by '!') in the check.\n",
                            "\n",
                            "Allows for glob patterns to be used. If not passed, ",
                            "all files are included.\n",
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-p".to_string(), "--include-packages".to_string()],
                    desc: Some("Include package errors in the check.".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
            ],
            groups: vec![SyntaxGroup {
                name: "scope".to_string(),
                parameters: vec!["--global".to_string(), "--local".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        let command = Command::Builtin(self.clone_boxed());
        let args = ConfigCheckCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        let mut errors = vec![];

        let wd = workdir(".");
        let wd_root = wd.root();

        if args.local_scope && wd_root.is_none() {
            omni_error!("Not in a worktree");
            exit(1);
        }

        // Get all the available configuration files
        let config_files: Vec<(String, ConfigScope)> = if !args.config_files.is_empty() {
            args.config_files
                .into_iter()
                .map(|file| (file, ConfigScope::Null))
                .collect()
        } else {
            ConfigLoader::all_config_files()
                .into_iter()
                .filter(|(_file, scope)| match scope {
                    ConfigScope::System => args.global_scope || args.default_scope,
                    ConfigScope::User => args.global_scope || args.default_scope,
                    ConfigScope::Workdir => args.local_scope || args.default_scope,
                    ConfigScope::Null => args.local_scope || args.default_scope,
                    ConfigScope::Default => true,
                })
                .collect()
        };

        for (file, scope) in config_files {
            let loader = ConfigLoader::new_from_file(&file, scope);
            let _config = OmniConfig::from_config_value(&loader.raw_config, &mut |e| {
                errors.push(ErrorFormatter::new_from_error(Some(file.clone()), e))
            });
        }

        // Now go over all the paths in the omnipath, so we can report:
        // - Files without `chmod +x`
        // - Files with missing metadata
        // - Errors in the metadata files (yaml)
        // - Errors in the metadata headers

        let search_paths = if !args.search_paths.is_empty() {
            args.search_paths
        } else {
            // Use the configuration files to get the paths
            let config_files: Vec<_> = ConfigLoader::all_config_files()
                .into_iter()
                .filter(|(_file, scope)| match scope {
                    ConfigScope::System => args.global_scope || args.default_scope,
                    ConfigScope::User => args.global_scope || args.default_scope,
                    ConfigScope::Workdir => args.local_scope || args.default_scope,
                    ConfigScope::Null => args.local_scope || args.default_scope,
                    ConfigScope::Default => true,
                })
                .collect();

            // Load the selected configuration files
            let mut loader = ConfigLoader::new_empty();
            for (file, scope) in config_files {
                loader.import_config_file(&file, scope);
            }
            let config: OmniConfig = loader.into();

            // Prepare the path list
            let mut paths = vec![];
            let mut seen = HashSet::new();

            // Read the prepend paths
            for path in config.path.prepend {
                if seen.insert(path.to_string()) {
                    paths.push(path.to_string());
                }
            }

            // If global, read the environment paths
            if args.global_scope || args.default_scope {
                for path in omnipath_env() {
                    if !path.is_empty() && seen.insert(path.clone()) {
                        paths.push(path.clone());
                    }
                }
            }

            // Read the append paths
            for path in config.path.append {
                if seen.insert(path.to_string()) {
                    paths.push(path.to_string());
                }
            }

            // TODO: If local, try and apply the `suggest_config` so that
            // we can evaluate any path that would be suggested to be added

            // Return all those paths
            paths.into_iter().collect()
        };

        let paths = search_paths
            .iter()
            .filter_map(|entry| {
                let path = PathBuf::from(&entry);
                if path.exists() {
                    Some(entry.to_string())
                } else {
                    errors.push(ErrorFormatter::new(
                        Some(entry.to_string()),
                        None,
                        "P001",
                        "Path not found",
                    ));
                    None
                }
            })
            .collect::<Vec<_>>();

        for path in paths {
            for command in PathCommand::aggregate_with_errors(&[path.clone()], &mut |e| {
                errors.push(ErrorFormatter::new_from_error(Some(path.clone()), e))
            }) {
                // Load the file details
                for err in command.errors().iter() {
                    errors.push(ErrorFormatter::new_from_error(
                        Some(command.source().to_string()),
                        err.clone(),
                    ));
                }
            }
        }

        // Filter and sort the errors
        let errors = errors
            .iter()
            .filter(|e| {
                args.include_packages || !PathBuf::from(e.file()).starts_with(package_root_path())
            })
            .filter(|e| check_allowed(&e.file(), &args.patterns))
            .filter(|e| e.selected(&args.select_errors, &args.ignore_errors))
            .filter(|e| !is_path_gitignored(&e.file()).unwrap_or(false))
            .sorted()
            .collect::<Vec<_>>();

        // Print the errors
        for error in errors.iter() {
            eprintln!("{}", error);
        }

        // Exit with the appropriate code
        exit(if errors.is_empty() { 0 } else { 1 });
    }

    fn autocompletion(&self) -> bool {
        false
    }

    fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        Err(())
    }
}

#[derive(PartialEq, Eq, Debug, Serialize)]
struct ErrorFormatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lineno: Option<usize>,
    errorcode: String,
    message: String,
    #[serde(skip)]
    default_ignored: bool,
}

impl ErrorFormatter {
    fn new(file: Option<String>, lineno: Option<usize>, errorcode: &str, message: &str) -> Self {
        Self {
            file,
            lineno,
            errorcode: errorcode.to_string(),
            message: message.to_string(),
            default_ignored: false,
        }
    }

    fn new_from_error(file: Option<String>, error: ConfigErrorKind) -> Self {
        let file = match error.path() {
            Some(path) => Some(path.to_string()),
            None => file,
        };
        let lineno = error.lineno();
        let errorcode = error.errorcode().to_string();
        let default_ignored = matches!(error, ConfigErrorKind::MetadataHeaderMissingSyntax);

        let message = match error {
            ConfigErrorKind::OmniPathFileNotExecutable { .. } => "Not executable".to_string(),
            _ => error.to_string(),
        };

        Self {
            file,
            lineno,
            errorcode,
            message,
            default_ignored,
        }
    }

    fn file(&self) -> String {
        match &self.file {
            Some(file) => file.clone(),
            None => "<unknown>".to_string(),
        }
    }

    fn errorcode(&self) -> &str {
        &self.errorcode
    }

    fn selected(&self, select_errors: &HashSet<String>, ignore_errors: &HashSet<String>) -> bool {
        // Filter according to the error code
        let errorcode = self.errorcode().to_uppercase();

        // Get the longest selected entry from which the error starts with
        let selected_level: i8 = select_errors
            .iter()
            .filter(|e| errorcode.starts_with(e.to_uppercase().as_str()))
            .map(|e| e.len() as i8)
            .max()
            .unwrap_or(if select_errors.is_empty() { 0 } else { -1 });

        // Skip this error if the selected_level < 0
        if selected_level < 0 || (self.default_ignored && selected_level < 4) {
            return false;
        }

        // Get the longest ignored entry from which the error starts with
        let ignored_level: i8 = ignore_errors
            .iter()
            .filter(|e| errorcode.starts_with(e.to_uppercase().as_str()))
            .map(|e| e.len() as i8)
            .max()
            .unwrap_or(-1);

        // Skip this error if the ignored_level >= selected_level
        if ignored_level >= selected_level {
            return false;
        }

        true
    }
}

impl Ord for ErrorFormatter {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file
            .cmp(&other.file)
            .then(self.lineno.cmp(&other.lineno))
            .then(self.errorcode.cmp(&other.errorcode))
            .then(self.message.cmp(&other.message))
    }
}

impl PartialOrd for ErrorFormatter {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for ErrorFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut parts = vec![];

        let path = abs_or_rel_path(&self.file());
        parts.push(path.light_blue());
        parts.push(self.errorcode().red());

        if let Some(lineno) = &self.lineno {
            parts.push(format!("{}", lineno).light_green());
        }

        parts.push(self.message.clone());

        write!(f, "{}", parts.join(":"))
    }
}
