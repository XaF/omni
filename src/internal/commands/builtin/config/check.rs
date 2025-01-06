use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::exit;

use itertools::Itertools;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::frompath::PathCommand;
use crate::internal::commands::Command;
use crate::internal::config::parser::ConfigError;
use crate::internal::config::parser::ConfigErrorHandler;
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
    include_packages: bool,
    global_scope: bool,
    local_scope: bool,
    default_scope: bool,
    ignore_errors: HashSet<String>,
    select_errors: HashSet<String>,
    patterns: Vec<String>,
    output: ConfigCheckCommandOutput,
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

        let output = match args.get("output") {
            Some(ParseArgsValue::SingleString(Some(value))) => match value.as_str() {
                "json" => ConfigCheckCommandOutput::Json,
                "plain" => ConfigCheckCommandOutput::Plain,
                _ => unreachable!("unknown value for output"),
            },
            _ => ConfigCheckCommandOutput::Plain,
        };

        Self {
            search_paths,
            config_files,
            include_packages,
            global_scope,
            local_scope,
            default_scope,
            ignore_errors,
            select_errors,
            patterns,
            output,
        }
    }
}

#[derive(Debug, Clone)]
enum ConfigCheckCommandOutput {
    Plain,
    Json,
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
                "Check the configuration files and commands in the omnipath for errors\n",
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
                    names: vec!["-p".to_string(), "--include-packages".to_string()],
                    desc: Some("Include package errors in the check.".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
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
                    names: vec!["-o".to_string(), "--output".to_string()],
                    desc: Some("Output format".to_string()),
                    arg_type: SyntaxOptArgType::Enum(vec!["json".to_string(), "plain".to_string()]),
                    default: Some("plain".to_string()),
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

        let error_handler = ConfigErrorHandler::new();

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
            let _config = OmniConfig::from_config_value(
                &loader.raw_config,
                &error_handler.with_file(file.clone()),
            );
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

        for entry in search_paths {
            let path = PathBuf::from(&entry);
            if !path.exists() {
                error_handler
                    .with_file(entry)
                    .error(ConfigErrorKind::OmniPathNotFound);

                continue;
            }

            let path_error_handler = error_handler.with_file(&entry);
            for command in PathCommand::aggregate_with_errors(&[entry], &path_error_handler) {
                command.check_errors(&path_error_handler);
            }
        }

        // Filter and sort the errors
        let errors = error_handler
            .errors()
            .into_iter()
            .filter(|e| {
                args.include_packages || !PathBuf::from(e.file()).starts_with(package_root_path())
            })
            .filter(|e| check_allowed(e.file(), &args.patterns))
            .filter(|e| check_selected(e, &args.select_errors, &args.ignore_errors))
            .filter(|e| !is_path_gitignored(e.file()).unwrap_or(false))
            .sorted()
            .collect::<Vec<_>>();

        // Print the errors
        match args.output {
            ConfigCheckCommandOutput::Plain => {
                for error in errors.iter() {
                    eprintln!("{}", error);
                }
            }
            ConfigCheckCommandOutput::Json => match serde_json::to_string_pretty(&errors) {
                Ok(json) => println!("{}", json),
                Err(e) => {
                    omni_error!(format!("Error while serializing the errors to JSON: {}", e));
                }
            },
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

fn check_selected(
    error: &ConfigError,
    select_errors: &HashSet<String>,
    ignore_errors: &HashSet<String>,
) -> bool {
    // Filter according to the error code
    let errorcode = error.errorcode().to_uppercase();

    // Get the longest selected entry from which the error starts with
    let selected_level: i8 = select_errors
        .iter()
        .filter(|e| errorcode.starts_with(e.to_uppercase().as_str()))
        .map(|e| e.len() as i8)
        .max()
        .unwrap_or(if select_errors.is_empty() { 0 } else { -1 });

    // Skip this error if the selected_level < 0
    if selected_level < 0 || (error.default_ignored() && selected_level < 4) {
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
