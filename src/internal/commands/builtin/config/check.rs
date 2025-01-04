use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::exit;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::frompath::PathCommand;
use crate::internal::commands::path::omnipath_entries;
use crate::internal::commands::Command;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigLoader;
use crate::internal::config::OmniConfig;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;

#[derive(Debug, Clone)]
struct ConfigCheckCommandArgs {
    search_paths: HashSet<String>,
}

impl From<BTreeMap<String, ParseArgsValue>> for ConfigCheckCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let search_paths = match args.get("search_path") {
            Some(ParseArgsValue::ManyString(search_paths)) => {
                search_paths.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        Self { search_paths }
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
            parameters: vec![SyntaxOptArg {
                names: vec!["-P".to_string(), "--search-path".to_string()],
                desc: Some(
                    concat!(
                        "Extra path to search git repositories to tidy up ",
                        "(repeat as many times as you need)",
                    )
                    .to_string(),
                ),
                arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
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

        // TODO(2025-01-03): Implement the following:
        // - Allow to specify files to check against for configuration
        // - Allow to specify dirs or files to check against for commands
        // - If no --search-path or --config-file is passed:
        //   - When in a workdir, default to check locally only unless
        //     --global is passed
        //   - When not in a workdir, default to check globally
        // - Add error codes
        // - Allow to select/deselect specific error codes

        // Get all the available configuration files
        let config_files = ConfigLoader::all_config_files();
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

        let omnipath = omnipath_entries()
            .into_iter()
            .map(|entry| entry.full_path)
            .collect::<Vec<_>>();

        let paths = omnipath
            .iter()
            .filter_map(|entry| {
                let path = PathBuf::from(&entry);
                if path.exists() {
                    Some(entry.to_string())
                } else {
                    errors.push(ErrorFormatter::new(
                        Some(entry.to_string()),
                        None,
                        None,
                        "Not found".to_string(),
                    ));
                    None
                }
            })
            .collect::<Vec<_>>();

        for command in PathCommand::aggregate_with_errors(&paths, &mut |e| {
            errors.push(ErrorFormatter::new_from_error(None, e))
        }) {
            // Load the file details
            for err in command.errors().iter() {
                errors.push(ErrorFormatter::new_from_error(
                    Some(command.source().to_string()),
                    err.clone(),
                ));
            }
        }

        // Print the errors after sorting them
        errors.sort();
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

#[derive(PartialEq, Eq, Debug)]
struct ErrorFormatter {
    file: Option<String>,
    lineno: Option<usize>,
    errorcode: Option<String>,
    message: String,
}

impl ErrorFormatter {
    fn new(
        file: Option<String>,
        lineno: Option<usize>,
        errorcode: Option<String>,
        message: String,
    ) -> Self {
        Self {
            file,
            lineno,
            errorcode,
            message,
        }
    }

    fn new_from_error(file: Option<String>, error: ConfigErrorKind) -> Self {
        let file = match file {
            Some(file) => Some(file),
            None => match error.path() {
                Some(path) => Some(path.to_string()),
                None => None,
            },
        };
        let lineno = error.lineno();
        let errorcode = error.errorcode().map(|s| s.to_string());

        let message = match error {
            ConfigErrorKind::OmniPathFileNotExecutable { path } => "Not executable".to_string(),
            _ => error.to_string(),
        };

        Self {
            file,
            lineno,
            errorcode,
            message,
        }
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

        if let Some(file) = &self.file {
            parts.push(file.light_blue());
        }

        if let Some(errorcode) = &self.errorcode {
            parts.push(errorcode.red());
        }

        if let Some(lineno) = &self.lineno {
            parts.push(format!("{}", lineno).light_green());
        }

        parts.push(self.message.clone());

        write!(f, "{}", parts.join(":"))
    }
}
