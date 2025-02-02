use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value as YamlValue;
use walkdir::WalkDir;

use crate::internal::commands::base::AutocompleteParameter;
use crate::internal::commands::base::CommandAutocompletion;
use crate::internal::commands::fromconfig::ConfigCommand;
use crate::internal::commands::path::omnipath;
use crate::internal::commands::utils::str_to_bool;
use crate::internal::commands::utils::SplitOnSeparators;
use crate::internal::commands::Command;
use crate::internal::config;
use crate::internal::config::config_loader;
use crate::internal::config::loader::WORKDIR_CONFIG_FILES;
use crate::internal::config::parser::parse_arg_name;
use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::utils::is_executable;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigExtendOptions;
use crate::internal::config::ConfigScope;
use crate::internal::config::OmniConfig;
use crate::internal::config::SyntaxGroup;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgNumValues;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::git::package_path_from_handle;
use crate::internal::workdir;
use crate::internal::ConfigLoader;

#[derive(Debug, Clone)]
pub struct PathCommand {
    name: Vec<String>,
    source: String,
    aliases: BTreeMap<Vec<String>, String>,
    file_details: OnceCell<Option<PathCommandFileDetails>>,
}

impl PathCommand {
    pub fn all() -> Vec<Command> {
        Self::aggregate_commands_from_path(&omnipath(), &ConfigErrorHandler::noop())
    }

    pub fn aggregate_with_errors(
        paths: &[String],
        error_handler: &ConfigErrorHandler,
    ) -> Vec<Command> {
        Self::aggregate_commands_from_path(paths, error_handler)
    }

    pub fn local() -> Vec<Command> {
        // Check if we are in a work directory
        let workdir = workdir(".");
        let (wd_id, wd_root) = match (workdir.id(), workdir.root()) {
            (Some(id), Some(root)) => (id, root),
            _ => return vec![],
        };

        // Since we're prioritizing local, we want to make sure we consider
        // the local suggestions for the configuration; this means we will
        // handle suggested configuration even if not applied globally before
        // going over the omnipath.
        let cfg = config(".");
        let suggest_config_value = cfg.suggest_config.config();
        let local_config: OmniConfig = if suggest_config_value.is_null() {
            cfg
        } else {
            let mut local_config = config_loader(".").raw_config.clone();
            local_config.extend(
                suggest_config_value.clone(),
                ConfigExtendOptions::new(),
                vec![],
            );
            local_config.into()
        };

        // Get the package and worktree paths for the current repo
        // TODO: make it work from a package path to include existing
        //       paths from the worktree too
        let worktree_path = Some(PathBuf::from(wd_root));
        let package_path = package_path_from_handle(&wd_id);
        let expected_path = PathBuf::from(wd_root);

        // Now we can extract the different values that would be applied to
        // the path that are actually matching the current work directory;
        // note we will consider both path that are matching the current work
        // directory but also convert any path that would match the package
        // path for the same work directory.
        let local_paths = local_config
            .path
            .prepend
            .iter()
            .chain(local_config.path.append.iter())
            .filter_map(|path_entry| {
                if !path_entry.is_valid() {
                    return None;
                }

                let pathbuf = PathBuf::from(&path_entry.full_path);

                if let Some(worktree_path) = &worktree_path {
                    if let Ok(suffix) = pathbuf.strip_prefix(worktree_path) {
                        return Some(expected_path.join(suffix).to_string_lossy().to_string());
                    }
                }

                if let Some(package_path) = &package_path {
                    if let Ok(suffix) = pathbuf.strip_prefix(package_path) {
                        return Some(expected_path.join(suffix).to_string_lossy().to_string());
                    }
                }

                None
            })
            .collect::<Vec<String>>();

        Self::aggregate_commands_from_path(&local_paths, &ConfigErrorHandler::noop())
    }

    fn aggregate_commands_from_path(
        paths: &[String],
        error_handler: &ConfigErrorHandler,
    ) -> Vec<Command> {
        let mut all_commands: Vec<Command> = Vec::new();
        let mut known_sources: HashMap<String, usize> = HashMap::new();

        for path in paths {
            // If this is a file, we either need to load configuration commands
            // from an omni configuration file, or we can just skip to the next
            // entry in the path list
            let pathobj = Path::new(path);
            if pathobj.is_file() {
                // Check if this is an omni configuration file
                if WORKDIR_CONFIG_FILES.iter().any(|f| path.ends_with(f)) {
                    let loader = ConfigLoader::new_from_file(
                        path,
                        // We just consider it's workdir scope, but shouldn't be
                        // important as we do not do anything specific with that
                        // configuration besides reading the commands
                        ConfigScope::Workdir,
                    );
                    let file_config = OmniConfig::from_config_value(
                        &loader.raw_config,
                        &error_handler.with_file(path.clone()),
                    );

                    all_commands.extend(
                        ConfigCommand::all_commands(file_config.commands.clone(), vec![])
                            .into_iter()
                            .filter(|cmd| cmd.export())
                            .map(|cmd| cmd.into()),
                    );
                }

                continue;
            }

            // Aggregate all the files first, since WalkDir does not sort the list
            let mut files_to_process = Vec::new();
            for entry in WalkDir::new(path).follow_links(true).into_iter().flatten() {
                let filetype = entry.file_type();
                let filepath = entry.path();

                if !filetype.is_file() {
                    continue;
                }

                if !is_executable(filepath) {
                    error_handler
                        .with_file(filepath.to_string_lossy().to_string())
                        .error(ConfigErrorKind::OmniPathFileNotExecutable);

                    continue;
                }

                files_to_process.push(filepath.to_path_buf());
            }

            // Sort the files by path
            files_to_process.sort();

            // Process the files
            for filepath in files_to_process {
                let mut partitions = filepath
                    .strip_prefix(format!("{}/", path))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .split('/')
                    .collect::<Vec<&str>>();

                let num_partitions = partitions.len();

                // For each partition that is not the last one, remove
                // the suffix `.d` if it exists
                for partition in &mut partitions[0..num_partitions - 1] {
                    if partition.ends_with(".d") {
                        *partition = &partition[0..partition.len() - 2];
                    }
                }

                // For the last partition, remove any file extension
                if let Some(filename) = partitions.last_mut() {
                    if let Some(dotpos) = filename.rfind('.') {
                        *filename = &filename[0..dotpos];
                    }
                }

                let new_command = PathCommand::new(
                    partitions.iter().map(|s| s.to_string()).collect(),
                    filepath.to_str().unwrap().to_string(),
                );

                // Check if the source is already known
                if let Some(idx) = known_sources.get_mut(&new_command.real_source()) {
                    // Add this command's name to the command's aliases
                    let cmd: &mut _ = &mut all_commands[*idx];
                    match cmd {
                        Command::FromPath(cmd) => {
                            cmd.add_alias(new_command.name(), Some(new_command.source()))
                        }
                        _ => unreachable!(),
                    }
                } else {
                    // Add the new command
                    known_sources.insert(new_command.real_source(), all_commands.len());
                    all_commands.push(new_command.into());
                }
            }
        }

        all_commands
    }

    pub fn new(name: Vec<String>, source: String) -> Self {
        Self {
            name,
            source,
            aliases: BTreeMap::new(),
            file_details: OnceCell::new(),
        }
    }

    pub fn name(&self) -> Vec<String> {
        self.name.clone()
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        self.aliases.keys().cloned().collect()
    }

    fn add_alias(&mut self, alias: Vec<String>, source: Option<String>) {
        if alias == self.name {
            return;
        }

        if self.aliases.keys().any(|a| a == &alias) {
            return;
        }

        self.aliases
            .insert(alias, source.unwrap_or(self.source.clone()));
    }

    pub fn source(&self) -> String {
        self.source.clone()
    }

    fn real_source(&self) -> String {
        if let Ok(canon) = std::fs::canonicalize(&self.source) {
            canon.to_str().unwrap().to_string()
        } else {
            self.source.clone()
        }
    }

    pub fn help(&self) -> Option<String> {
        self.file_details().and_then(|details| details.help.clone())
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        self.file_details()
            .and_then(|details| details.syntax.clone())
    }

    pub fn category(&self) -> Option<Vec<String>> {
        self.file_details()
            .and_then(|details| details.category.clone())
    }

    pub fn argparser(&self) -> bool {
        self.file_details()
            .map(|details| details.argparser)
            .unwrap_or(false)
    }

    pub fn tags(&self) -> BTreeMap<String, String> {
        self.file_details()
            .map(|details| details.tags.clone())
            .unwrap_or_default()
    }

    pub fn exec(&self, argv: Vec<String>, called_as: Option<Vec<String>>) {
        // Get the source of the command as called
        let source = called_as.map_or(self.source.clone(), |called_as| {
            self.aliases
                .get(&called_as)
                .cloned()
                .unwrap_or(self.source.clone())
        });

        // Execute the command
        let err = ProcessCommand::new(source).args(argv).exec();

        panic!("Something went wrong: {:?}", err);
    }

    pub fn autocompletion(&self) -> CommandAutocompletion {
        self.file_details()
            .map(|details| details.autocompletion)
            .unwrap_or(CommandAutocompletion::Null)
    }

    pub fn autocomplete(
        &self,
        comp_cword: usize,
        argv: Vec<String>,
        parameter: Option<AutocompleteParameter>,
    ) -> Result<(), ()> {
        let mut command = ProcessCommand::new(self.source.clone());
        command.arg("--complete");
        command.args(argv);
        command.env("COMP_CWORD", comp_cword.to_string());
        if let Some(param) = parameter {
            command.env("OMNI_COMP_VALUE_OF", param.name);
            command.env("OMNI_COMP_VALUE_START_INDEX", param.index.to_string());
        }

        match command.output() {
            Ok(output) => {
                println!("{}", String::from_utf8_lossy(&output.stdout));
                Ok(())
            }
            Err(_) => Err(()),
        }
    }

    pub fn requires_sync_update(&self) -> bool {
        self.file_details()
            .map(|details| details.sync_update)
            .unwrap_or(false)
    }

    fn file_details(&self) -> Option<&PathCommandFileDetails> {
        self.file_details
            .get_or_init(|| {
                PathCommandFileDetails::from_file(&self.source, &ConfigErrorHandler::noop())
            })
            .as_ref()
    }

    pub fn check_errors(&self, error_handler: &ConfigErrorHandler) {
        if PathCommandFileDetails::from_file(&self.source, error_handler).is_none() {
            error_handler.error(ConfigErrorKind::OmniPathFileFailedToLoadMetadata);
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PathCommandFileDetails {
    category: Option<Vec<String>>,
    help: Option<String>,
    autocompletion: CommandAutocompletion,
    syntax: Option<CommandSyntax>,
    tags: BTreeMap<String, String>,
    sync_update: bool,
    argparser: bool,
}

impl<'de> Deserialize<'de> for PathCommandFileDetails {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::deserialize_with_errors(deserializer, &ConfigErrorHandler::noop())
    }
}

impl<'de> PathCommandFileDetails {
    fn deserialize_with_errors<D>(
        deserializer: D,
        error_handler: &ConfigErrorHandler,
    ) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = YamlValue::deserialize(deserializer)?;

        if let YamlValue::Mapping(ref mut map) = value {
            // Deserialize the autocompletion field, which can be either a
            // boolean or a string representing a boolean or 'partial'
            // The result is stored as a CommandAutocompletion enum
            // where 'true' is Full, 'partial' is Partial, and 'false' is Null
            let autocompletion: CommandAutocompletion = map
                .remove(YamlValue::String("autocompletion".to_string()))
                .map_or(CommandAutocompletion::Null, |v| match v {
                    YamlValue::Bool(b) => CommandAutocompletion::from(b),
                    YamlValue::String(s) => CommandAutocompletion::from(s),
                    _ => {
                        error_handler
                            .with_key("autocompletion")
                            .with_expected(vec!["boolean", "string"])
                            .with_actual(v.to_owned())
                            .error(ConfigErrorKind::InvalidValueType);

                        CommandAutocompletion::Null
                    }
                });

            // Deserialize the booleans
            let sync_update = map
                .remove(YamlValue::String("sync_update".to_string()))
                .is_some_and(|v| match bool::deserialize(v.clone()) {
                    Ok(b) => b,
                    Err(_err) => {
                        error_handler
                            .with_key("sync_update")
                            .with_expected("boolean")
                            .with_actual(v.to_owned())
                            .error(ConfigErrorKind::InvalidValueType);

                        false
                    }
                });
            let argparser = map
                .remove(YamlValue::String("argparser".to_string()))
                .is_some_and(|v| match bool::deserialize(v.clone()) {
                    Ok(b) => b,
                    Err(_err) => {
                        error_handler
                            .with_key("argparser")
                            .with_expected("boolean")
                            .with_actual(v.to_owned())
                            .error(ConfigErrorKind::InvalidValueType);

                        false
                    }
                });

            // Deserialize the help message
            let help = map
                .remove(YamlValue::String("help".to_string()))
                .and_then(|v| match String::deserialize(v.clone()) {
                    Ok(s) => Some(s),
                    Err(_err) => {
                        error_handler
                            .with_key("help")
                            .with_expected("string")
                            .with_actual(v.to_owned())
                            .error(ConfigErrorKind::InvalidValueType);

                        None
                    }
                });

            // Deserialize the category
            let category = map
                .remove(YamlValue::String("category".to_string()))
                .and_then(|v| match YamlValue::deserialize(v.clone()) {
                    Ok(value) => match value {
                        YamlValue::String(s) => Some(
                            s.split(',')
                                .map(|s| s.trim().to_string())
                                .collect::<Vec<String>>(),
                        ),
                        YamlValue::Sequence(s) => Some(
                            s.iter()
                                .enumerate()
                                .filter_map(|(idx, entry)| match entry {
                                    YamlValue::String(s) => Some(s.trim().to_string()),
                                    YamlValue::Number(n) => Some(n.to_string()),
                                    YamlValue::Bool(b) => Some(b.to_string()),
                                    _ => {
                                        error_handler
                                            .with_key("category")
                                            .with_index(idx)
                                            .with_expected("string")
                                            .with_actual(entry.to_owned())
                                            .error(ConfigErrorKind::InvalidValueType);

                                        None
                                    }
                                })
                                .collect::<Vec<String>>(),
                        ),
                        _ => {
                            error_handler
                                .with_key("category")
                                .with_expected(vec!["string", "sequence"])
                                .with_actual(value.to_owned())
                                .error(ConfigErrorKind::InvalidValueType);

                            None
                        }
                    },
                    Err(_err) => {
                        error_handler
                            .with_key("category")
                            .with_expected(vec!["string", "sequence"])
                            .with_actual(v.to_owned())
                            .error(ConfigErrorKind::InvalidValueType);

                        None
                    }
                });

            // Deserialize the syntax
            let syntax = map
                .remove(YamlValue::String("syntax".to_string()))
                .and_then(
                    |v| match CommandSyntax::deserialize(v.clone(), error_handler) {
                        Ok(s) => Some(s),
                        Err(_err) => {
                            error_handler
                                .with_key("syntax")
                                .with_expected("table")
                                .with_actual(v.to_owned())
                                .error(ConfigErrorKind::InvalidValueType);

                            None
                        }
                    },
                );

            // Deserialize the tags
            let tags = map
                .remove(YamlValue::String("tags".to_string()))
                .and_then(
                    |v| match BTreeMap::<String, String>::deserialize(v.clone()) {
                        Ok(t) => Some(t),
                        Err(_err) => {
                            error_handler
                                .with_key("tags")
                                .with_expected("table")
                                .with_actual(v.to_owned())
                                .error(ConfigErrorKind::InvalidValueType);

                            None
                        }
                    },
                )
                .unwrap_or_default();

            Ok(Self {
                autocompletion,
                sync_update,
                argparser,
                help,
                category,
                syntax,
                tags,
            })
        } else {
            error_handler
                .with_expected("table")
                .with_actual(value)
                .error(ConfigErrorKind::InvalidValueType);

            Ok(Self::default())
        }
    }
}

impl PathCommandFileDetails {
    pub fn from_file(path: &str, error_handler: &ConfigErrorHandler) -> Option<Self> {
        if let Some(details) = Self::from_metadata_file(path, error_handler) {
            return Some(details);
        }

        if let Some(details) = Self::from_source_file(path, error_handler) {
            return Some(details);
        }

        None
    }

    pub fn from_metadata_file(path: &str, error_handler: &ConfigErrorHandler) -> Option<Self> {
        // The metadata file for `file.ext` can be either
        // `file.ext.metadata.yaml` or `file.metadata.yaml`
        let mut metadata_files = vec![format!("{}.metadata.yaml", path)];
        if let Some(dotpos) = path.rfind('.') {
            metadata_files.push(format!("{}.metadata.yaml", &path[0..dotpos]));
        }

        for metadata_file in metadata_files {
            let path = Path::new(&metadata_file);

            // Check if the metadata file exists
            if !path.exists() {
                continue;
            }

            if let Ok(file) = File::open(path) {
                let deserializer = serde_yaml::Deserializer::from_reader(file);
                if let Ok(mut md) = Self::deserialize_with_errors(
                    deserializer,
                    &error_handler.with_file(metadata_file),
                ) {
                    // If the help is not empty, split it into lines
                    if let Some(help) = &mut md.help {
                        *help = handle_color_codes(help.clone());
                    }

                    return Some(md);
                }
            }
        }

        None
    }

    fn parse_header_group(
        group_name: &str,
        value: &str,
        error_handler: &ConfigErrorHandler,
    ) -> Option<SyntaxGroup> {
        if group_name.is_empty() {
            return None;
        }

        let mut parameters = vec![];
        let mut required = false;
        let mut multiple = false;
        let mut requires = vec![];
        let mut conflicts_with = vec![];

        let mut value_parts = SplitOnSeparators::new(value, &[':', '\n']);

        while let Some(part) = value_parts.next() {
            let part = part.trim();
            if part.is_empty() {
                error_handler
                    .with_context("group", group_name)
                    .error(ConfigErrorKind::MetadataHeaderGroupEmptyPart);
                continue;
            }

            if let Some((key, value)) = part.split_once('=') {
                let key = key.to_lowercase();
                if !key.contains(' ') {
                    let value = value.trim();

                    match key.as_str() {
                        "required" => required = str_to_bool(value).unwrap_or(false),
                        "multiple" => multiple = str_to_bool(value).unwrap_or(false),
                        "requires" | "conflicts_with" => {
                            let args = value
                                .split(' ')
                                .map(|s| s.trim().to_lowercase())
                                .collect::<Vec<String>>();

                            match key.as_str() {
                                "requires" => requires.extend(args),
                                "conflicts_with" => conflicts_with.extend(args),
                                _ => unreachable!(),
                            }
                        }
                        _ => {
                            error_handler
                                .with_context("group", group_name)
                                .with_context("config_key", key)
                                .error(ConfigErrorKind::MetadataHeaderGroupUnknownConfigKey);
                        }
                    }

                    // We have a key-value pair, so we can continue to the next part
                    continue;
                }
            }

            let mut rest = part.to_string();
            rest.push_str(value_parts.remainder());

            parameters = shell_words::split(&rest)
                .unwrap_or_else(|_| vec![])
                .iter()
                .map(|s| s.to_string())
                .collect();
        }

        if parameters.is_empty() {
            error_handler
                .with_context("group", group_name)
                .error(ConfigErrorKind::MetadataHeaderGroupMissingParameters);
        }

        Some(SyntaxGroup {
            name: group_name.to_string(),
            parameters,
            required,
            multiple,
            requires,
            conflicts_with,
        })
    }

    fn parse_header_arg(
        required: bool,
        arg_name: &str,
        value: &str,
        error_handler: &ConfigErrorHandler,
    ) -> Option<SyntaxOptArg> {
        if arg_name.is_empty() {
            return None;
        }

        // Prepare all arguments
        let mut dest = None;
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
        let mut description = String::new();

        // Parse the argument name
        let (names, arg_type, placeholders, mut leftovers) = parse_arg_name(arg_name);
        let mut arg_type = arg_type.to_string();

        // Now parse the rest of the string
        // Split the value over either `:` or `\n`
        let mut value_parts = SplitOnSeparators::new(value, &[':', '\n']);

        // Go over the parts until we have a non-empty part without a <key>=<value> pair,
        // or a <key> that contains spaces, at which point we can cram all the leftover
        // parts into the description
        while let Some(part) = value_parts.next() {
            let part = part.trim();
            if part.is_empty() {
                error_handler
                    .with_context("parameter", arg_name)
                    .error(ConfigErrorKind::MetadataHeaderParameterEmptyPart);

                continue;
            }

            if let Some((key, value)) = part.split_once('=') {
                let key = key.to_lowercase();
                if !key.contains(' ') {
                    let value = value.trim();

                    match key.as_str() {
                        "default" => default = Some(value.to_string()),
                        "default_missing_value" => default_missing_value = Some(value.to_string()),
                        "dest" => dest = Some(value.to_string()),
                        "type" => arg_type = value.to_string(),
                        "num_values" => {
                            if let Some(num) = SyntaxOptArgNumValues::from_str(
                                value,
                                &error_handler.with_key("num_values"),
                            ) {
                                num_values = Some(num)
                            }
                        }
                        "delimiter" => {
                            if value.len() == 1 {
                                value_delimiter = Some(value.chars().next().unwrap());
                            } else {
                                error_handler
                                    .with_context("parameter", arg_name)
                                    .with_context("key", key)
                                    .with_context("value", value)
                                    .error(ConfigErrorKind::MetadataHeaderParameterInvalidKeyValue);
                            }
                        }
                        "last" => last_arg_double_hyphen = str_to_bool(value).unwrap_or(false),
                        "leftovers" => leftovers = str_to_bool(value).unwrap_or(false),
                        "allow_hyphen_values" | "allow_hyphen" => {
                            allow_hyphen_values = str_to_bool(value).unwrap_or(false)
                        }
                        "allow_negative_numbers" | "negative_numbers" => {
                            allow_negative_numbers = str_to_bool(value).unwrap_or(false)
                        }
                        "group_occurrences" => {
                            group_occurrences = str_to_bool(value).unwrap_or(false)
                        }
                        "requires"
                        | "conflicts_with"
                        | "required_without"
                        | "required_without_all" => {
                            let args = value
                                .split(' ')
                                .map(|s| s.trim().to_lowercase())
                                .collect::<Vec<String>>();

                            match key.as_str() {
                                "requires" => requires.extend(args),
                                "conflicts_with" => conflicts_with.extend(args),
                                "required_without" => required_without.extend(args),
                                "required_without_all" => required_without_all.extend(args),
                                _ => unreachable!(),
                            }
                        }
                        "required_if_eq" | "required_if_eq_all" => {
                            if let Ok(args) = shell_words::split(value) {
                                for arg in args {
                                    let mut parts = arg.splitn(2, '=');

                                    let arg = match parts.next() {
                                        Some(arg) => arg.to_lowercase(),
                                        None => continue,
                                    };

                                    let value = parts.next().unwrap_or("").to_string();

                                    // Unquote the value if needed
                                    let value = if (value.starts_with('"') && value.ends_with('"'))
                                        || (value.starts_with('\'') && value.ends_with('\''))
                                    {
                                        value[1..value.len() - 1].to_string()
                                    } else {
                                        value
                                    };

                                    if !arg.is_empty() {
                                        match key.as_str() {
                                            "required_if_eq" => {
                                                required_if_eq.insert(arg, value);
                                            }
                                            "required_if_eq_all" => {
                                                required_if_eq_all.insert(arg, value);
                                            }
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                            } else {
                                error_handler
                                    .with_context("parameter", arg_name)
                                    .with_context("key", key)
                                    .with_context("value", value)
                                    .error(ConfigErrorKind::MetadataHeaderParameterInvalidKeyValue);
                            }
                        }
                        _ => {
                            error_handler
                                .with_context("parameter", arg_name)
                                .with_context("config_key", key)
                                .error(ConfigErrorKind::MetadataHeaderParameterUnknownConfigKey);
                        }
                    }

                    // We have a key-value pair, so we can continue to the next part
                    continue;
                }
            }

            description = part.to_string();
            description.push_str(value_parts.remainder());
        }

        description = description.trim().to_string();
        let desc = if description.is_empty() {
            error_handler
                .with_context("parameter", arg_name)
                .error(ConfigErrorKind::MetadataHeaderParameterMissingDescription);

            None
        } else {
            let description = handle_color_codes(description);
            Some(description)
        };

        let arg_type = SyntaxOptArgType::from_str(&arg_type, &error_handler.with_key("arg_type"))
            .unwrap_or(SyntaxOptArgType::String);

        Some(SyntaxOptArg {
            names,
            dest,
            desc,
            required,
            placeholders,
            default,
            default_missing_value,
            arg_type,
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

    fn from_source_file_header<R: BufRead>(
        reader: &mut R,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let mut autocompletion = CommandAutocompletion::Null;
        let mut sync_update = false;
        let mut argparser = false;
        let mut category: Option<Vec<String>> = None;
        let mut help_lines: Vec<String> = Vec::new();
        let mut tags: BTreeMap<String, String> = BTreeMap::new();

        let mut current_key: Option<(String, Option<String>)> = None;
        let mut current_obj: Option<(String, String, String)> = None;
        let mut parameters_data: Vec<(String, String, String)> = vec![];
        let mut group_data: Vec<(String, String)> = vec![];

        let mut key_tracker = MetadataKeyTracker::new();

        // We want to parse lines in the format:
        // # key: value
        // And support continuation:
        // # key: this is a multiline
        // # + value for the key

        for (idx, line) in reader
            .lines()
            .map_while(Result::ok)
            .take_while(|line| line.starts_with('#'))
            .filter_map(|line| line.strip_prefix('#').map(|s| s.to_string()))
            .map(|line| line.trim().to_string())
            .enumerate()
        {
            let lineno = idx + 1;
            let (key, subkey, value) = {
                let mut parts = line.splitn(2, ':');
                let key = match parts.next() {
                    Some(key) => key.trim().to_lowercase(),
                    None => continue,
                };
                let value = match parts.next() {
                    Some(value) => value.trim().to_string(),
                    None => continue,
                };

                let (subkey, value) = match key.as_str() {
                    "opt" | "arg" | "arggroup" | "tag" => {
                        let mut subparts = value.splitn(2, ':');
                        let subkey = match subparts.next().map(|s| s.trim()) {
                            Some(subkey) if !subkey.is_empty() => subkey.to_string(),
                            _ => {
                                error_handler
                                    .with_lineno(lineno)
                                    .with_context("key", key)
                                    .error(ConfigErrorKind::MetadataHeaderMissingSubkey);

                                continue;
                            }
                        };
                        let value = subparts.next().unwrap_or("").trim().to_string();
                        (Some(subkey), value)
                    }
                    _ => (None, value),
                };
                (key, subkey, value)
            };

            let (key, subkey) = match key.as_str() {
                "+" => match current_key {
                    Some((ref key, ref subkey)) => (key.clone(), subkey.clone()),
                    None => {
                        error_handler
                            .with_lineno(lineno)
                            .error(ConfigErrorKind::MetadataHeaderContinueWithoutKey);

                        continue;
                    }
                },
                _ => {
                    current_key = Some((key.clone(), subkey.clone()));
                    (key, subkey)
                }
            };

            match (key.as_str(), subkey, value) {
                ("category", None, value) => {
                    key_tracker.handle_seen_key(&key, lineno, true, error_handler);

                    let handled_value = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<String>>();
                    match category {
                        Some(ref mut cat) => cat.extend(handled_value),
                        None => category = Some(handled_value),
                    }
                }
                ("autocompletion", None, value) => {
                    key_tracker.handle_seen_key(&key, lineno, false, error_handler);
                    autocompletion = match str_to_bool(&value) {
                        Some(b) => CommandAutocompletion::from(b),
                        None if value.to_lowercase() == "partial" => CommandAutocompletion::Partial,
                        None => {
                            error_handler
                                .with_lineno(lineno)
                                .with_context("key", key)
                                .with_context("value", value)
                                .with_expected("boolean")
                                .error(ConfigErrorKind::MetadataHeaderInvalidValueType);

                            CommandAutocompletion::Null
                        }
                    };
                }
                ("sync_update", None, value) => {
                    key_tracker.handle_seen_key(&key, lineno, false, error_handler);
                    sync_update = match str_to_bool(&value) {
                        Some(b) => b,
                        None => {
                            error_handler
                                .with_lineno(lineno)
                                .with_context("key", key)
                                .with_context("value", value)
                                .with_expected("boolean")
                                .error(ConfigErrorKind::MetadataHeaderInvalidValueType);

                            false
                        }
                    };
                }
                ("argparser", None, value) => {
                    key_tracker.handle_seen_key(&key, lineno, false, error_handler);
                    argparser = match str_to_bool(&value) {
                        Some(b) => b,
                        None => {
                            error_handler
                                .with_lineno(lineno)
                                .with_context("key", key)
                                .with_context("value", value)
                                .with_expected("boolean")
                                .error(ConfigErrorKind::MetadataHeaderInvalidValueType);

                            false
                        }
                    };
                }
                ("help", None, value) => {
                    key_tracker.handle_seen_key(&key, lineno, true, error_handler);

                    help_lines.push(value);
                }
                ("arg", Some(subkey), value)
                | ("opt", Some(subkey), value)
                | ("arggroup", Some(subkey), value) => {
                    key_tracker.handle_seen_key(
                        &format!("{}:{}", key, subkey),
                        lineno,
                        true,
                        error_handler,
                    );

                    match current_obj {
                        Some((cur_key, cur_subkey, cur_value))
                            if cur_key == key
                                && cur_subkey.to_lowercase() == subkey.to_lowercase() =>
                        {
                            current_obj =
                                Some((cur_key, cur_subkey, format!("{}\n{}", cur_value, value)));
                        }
                        Some((cur_key, cur_subkey, cur_value)) => {
                            match cur_key.as_str() {
                                "arg" | "opt" => {
                                    parameters_data.push((cur_key, cur_subkey, cur_value));
                                }
                                "arggroup" => {
                                    group_data.push((cur_subkey, cur_value));
                                }
                                _ => unreachable!(),
                            }
                            current_obj = Some((key, subkey, value));
                        }
                        None => {
                            current_obj = Some((key, subkey, value));
                        }
                    }
                }
                ("tag", Some(subkey), value) => {
                    key_tracker.handle_seen_key(
                        &format!("{}:{}", key, subkey),
                        lineno,
                        false,
                        error_handler,
                    );
                    tags.insert(subkey.to_string(), value);
                }
                _ if !key_tracker.is_empty() => {
                    error_handler
                        .with_lineno(lineno)
                        .with_context("key", key)
                        .error(ConfigErrorKind::MetadataHeaderUnknownKey);
                }
                _ => {}
            }
        }

        // Make sure any current object is added to the corresponding list
        if let Some((key, subkey, value)) = current_obj {
            match key.as_str() {
                "arg" | "opt" => {
                    parameters_data.push((key, subkey, value));
                }
                "arggroup" => {
                    group_data.push((subkey, value));
                }
                _ => unreachable!(),
            }
        }

        let parameters = parameters_data
            .iter()
            .flat_map(|(key, arg_name, value)| {
                let is_required = key == "arg";
                Self::parse_header_arg(is_required, arg_name, value, error_handler)
            })
            .map(|mut param| {
                if let Some(desc) = &param.desc {
                    param.desc = Some(handle_color_codes(desc.clone()));
                }
                param
            })
            .collect::<Vec<SyntaxOptArg>>();

        let groups = group_data
            .iter()
            .flat_map(|(grp_name, value)| Self::parse_header_group(grp_name, value, error_handler))
            .collect::<Vec<SyntaxGroup>>();

        let syntax = if parameters.is_empty() && groups.is_empty() {
            error_handler.error(ConfigErrorKind::MetadataHeaderMissingSyntax);

            None
        } else {
            let mut syntax = CommandSyntax::new();
            syntax.parameters = parameters;
            syntax.groups = groups;

            Some(syntax)
        };

        let help = if help_lines.is_empty() {
            error_handler.error(ConfigErrorKind::MetadataHeaderMissingHelp);

            None
        } else {
            let help = handle_color_codes(help_lines.join("\n"));
            Some(help)
        };

        Some(PathCommandFileDetails {
            category,
            help,
            autocompletion,
            argparser,
            syntax,
            tags,
            sync_update,
        })
    }

    pub fn from_source_file(path: &str, error_handler: &ConfigErrorHandler) -> Option<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(_) => {
                error_handler
                    .with_file(path)
                    .error(ConfigErrorKind::OmniPathFileFailedToLoadMetadata);

                return None;
            }
        };

        let mut reader = BufReader::new(file);

        Self::from_source_file_header(&mut reader, &error_handler.with_file(path))
    }
}

struct MetadataKeyTracker {
    seen_keys: HashMap<String, usize>,
    last_key: String,
}

impl MetadataKeyTracker {
    fn new() -> Self {
        Self {
            seen_keys: HashMap::new(),
            last_key: String::new(),
        }
    }

    fn handle_seen_key(
        &mut self,
        key: &str,
        lineno: usize,
        allow_repeat: bool,
        error_handler: &ConfigErrorHandler,
    ) {
        if let Some(prev_lineno) = self.seen_keys.get(key) {
            if !allow_repeat || key != self.last_key {
                error_handler
                    .with_lineno(lineno)
                    .with_context("key", key)
                    .with_context("prev_lineno", *prev_lineno)
                    .error(ConfigErrorKind::MetadataHeaderDuplicateKey);
            }
        } else {
            self.seen_keys.insert(key.to_string(), lineno);
        }
        self.last_key = key.to_string();
    }

    fn is_empty(&self) -> bool {
        self.seen_keys.is_empty()
    }
}

fn handle_color_codes<T: ToString>(string: T) -> String {
    string
        .to_string()
        .replace("\\033[", "\x1B[")
        .replace("\\e[", "\x1B[")
        .replace("\\x1b[", "\x1B[")
        .replace("\\x1B[", "\x1B[")
        .replace("\\u{1B}[", "\x1B[")
        .replace("\\u{1b}[", "\x1B[")
}

#[cfg(test)]
mod tests {
    use super::*;

    mod from_source_file_header {
        use super::*;

        #[test]
        fn default() {
            let mut reader = BufReader::new("".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());

            let details = details.unwrap();
            assert_eq!(details.category, None);
            assert_eq!(details.help, None);
            assert!(matches!(
                details.autocompletion,
                CommandAutocompletion::Null
            ));
            assert_eq!(details.syntax, None);
            assert!(!details.sync_update);
        }

        #[test]
        fn simple() {
            let mut reader = BufReader::new("# category: test cat\n# help: test help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.category, Some(vec!["test cat".to_string()]));
            assert_eq!(details.help, Some("test help".to_string()));
        }

        #[test]
        fn help() {
            let mut reader = BufReader::new("# help: test help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.help, Some("test help".to_string()));
        }

        #[test]
        fn help_multiline_using_repeat() {
            let mut reader =
                BufReader::new("# help: test help\n# help: continued help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.help, Some("test help\ncontinued help".to_string()));
        }

        #[test]
        fn help_multiline_using_plus() {
            let mut reader = BufReader::new("# help: test help\n# +: continued help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.help, Some("test help\ncontinued help".to_string()));
        }

        #[test]
        fn category() {
            let mut reader = BufReader::new("# category: test cat\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.category, Some(vec!["test cat".to_string()]));
        }

        #[test]
        fn category_splits_commas() {
            let mut reader = BufReader::new("# category: test cat, continued cat\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(
                details.category,
                Some(vec!["test cat".to_string(), "continued cat".to_string()])
            );
        }

        #[test]
        fn category_multiline_appends_to_existing() {
            let mut reader =
                BufReader::new("# category: test cat\n# category: continued cat\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(
                details.category,
                Some(vec!["test cat".to_string(), "continued cat".to_string()])
            );
        }

        #[test]
        fn category_multiline_splits_commas() {
            let mut reader = BufReader::new(
                "# category: test cat, other cat\n# category: continued cat, more cat\n".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(
                details.category,
                Some(vec![
                    "test cat".to_string(),
                    "other cat".to_string(),
                    "continued cat".to_string(),
                    "more cat".to_string()
                ])
            );
        }

        #[test]
        fn autocompletion() {
            let mut reader = BufReader::new("# autocompletion: true\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(matches!(
                details.autocompletion,
                CommandAutocompletion::Full
            ));
        }

        #[test]
        fn autocompletion_partial() {
            let mut reader = BufReader::new("# autocompletion: partial\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(matches!(
                details.autocompletion,
                CommandAutocompletion::Partial
            ));
        }

        #[test]
        fn autocompletion_false() {
            let mut reader = BufReader::new("# autocompletion: false\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(matches!(
                details.autocompletion,
                CommandAutocompletion::Null
            ));
        }

        #[test]
        fn argparser() {
            let mut reader = BufReader::new("# argparser: true\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(details.argparser);
        }

        #[test]
        fn argparser_false() {
            let mut reader = BufReader::new("# argparser: false\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(!details.argparser);
        }

        #[test]
        fn sync_update() {
            let mut reader = BufReader::new("# sync_update: true\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(details.sync_update);
        }

        #[test]
        fn sync_update_false() {
            let mut reader = BufReader::new("# sync_update: false\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(!details.sync_update);
        }

        #[test]
        fn arg_simple_short() {
            let mut reader = BufReader::new("# arg: -a: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_simple_long() {
            let mut reader = BufReader::new("# arg: --arg: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["--arg".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_simple_positional() {
            let mut reader = BufReader::new("# arg: arg: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["arg".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_without_description() {
            let mut reader = BufReader::new("# arg: -a\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    required: true,
                    ..Default::default()
                }
            );
        }

        fn param_with_type(required: bool, type_str: &str, type_enum: SyntaxOptArgType) {
            let value = format!(
                "# {}: -a: type={}: test desc\n",
                if required { "arg" } else { "opt" },
                type_str
            );
            let mut reader = BufReader::new(value.as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required,
                    arg_type: type_enum,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_type_string() {
            param_with_type(true, "string", SyntaxOptArgType::String);
        }

        #[test]
        fn arg_with_type_int() {
            param_with_type(true, "int", SyntaxOptArgType::Integer);
        }

        #[test]
        fn arg_with_type_integer() {
            param_with_type(true, "integer", SyntaxOptArgType::Integer);
        }

        #[test]
        fn arg_with_type_float() {
            param_with_type(true, "float", SyntaxOptArgType::Float);
        }

        #[test]
        fn arg_with_type_bool() {
            param_with_type(true, "bool", SyntaxOptArgType::Boolean);
        }

        #[test]
        fn arg_with_type_boolean() {
            param_with_type(true, "boolean", SyntaxOptArgType::Boolean);
        }

        #[test]
        fn arg_with_type_flag() {
            param_with_type(true, "flag", SyntaxOptArgType::Flag);
        }

        #[test]
        fn arg_with_type_array_string() {
            param_with_type(
                true,
                "array/string",
                SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
            );
        }

        #[test]
        fn arg_with_type_array_int() {
            param_with_type(
                true,
                "array/int",
                SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Integer)),
            );
        }

        #[test]
        fn arg_with_type_array_integer() {
            param_with_type(
                true,
                "array/integer",
                SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Integer)),
            );
        }

        #[test]
        fn arg_with_type_array_float() {
            param_with_type(
                true,
                "array/float",
                SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Float)),
            );
        }

        #[test]
        fn arg_with_type_array_bool() {
            param_with_type(
                true,
                "array/bool",
                SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Boolean)),
            );
        }

        #[test]
        fn arg_with_type_array_boolean() {
            param_with_type(
                true,
                "array/boolean",
                SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Boolean)),
            );
        }

        #[test]
        fn arg_with_delimiter() {
            let mut reader = BufReader::new("# arg: -a: delimiter=,: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    value_delimiter: Some(','),
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_last() {
            let mut reader = BufReader::new("# arg: -a: last=true: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    last_arg_double_hyphen: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_leftovers_dots() {
            let mut reader = BufReader::new("# arg: a...: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    leftovers: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_leftovers_no_dots() {
            let mut reader = BufReader::new("# arg: -a: leftovers=true: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    leftovers: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_allow_hyphen_values() {
            let mut reader = BufReader::new("# arg: -a: allow_hyphen=true: test desc\n# arg: -b: allow_hyphen_values=true: test desc2".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 2);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    allow_hyphen_values: true,
                    ..Default::default()
                }
            );

            let arg = &syntax.parameters[1];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-b".to_string()],
                    desc: Some("test desc2".to_string()),
                    required: true,
                    allow_hyphen_values: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_allow_negative_numbers() {
            let mut reader = BufReader::new("# arg: -a: allow_negative_numbers=true: test desc\n# arg: -b: negative_numbers=true: test desc2".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 2);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    allow_negative_numbers: true,
                    ..Default::default()
                }
            );

            let arg = &syntax.parameters[1];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-b".to_string()],
                    desc: Some("test desc2".to_string()),
                    required: true,
                    allow_negative_numbers: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_requires_single() {
            let mut reader = BufReader::new("# arg: -a: requires=b: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    requires: vec!["b".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_requires_multiple() {
            let mut reader = BufReader::new("# arg: -a: requires=b c: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    requires: vec!["b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_requires_multiple_repeat() {
            let mut reader =
                BufReader::new("# arg: -a: requires=b: requires=c: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    requires: vec!["b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_conflicts_with() {
            let mut reader = BufReader::new("# arg: -a: conflicts_with=b: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    conflicts_with: vec!["b".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_conflits_with_multiple() {
            let mut reader =
                BufReader::new("# arg: -a: conflicts_with=b c: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    conflicts_with: vec!["b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_conflits_with_multiple_repeat() {
            let mut reader = BufReader::new(
                "# arg: -a: conflicts_with=b: conflicts_with=c: test desc\n".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    conflicts_with: vec!["b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_required_without() {
            let mut reader =
                BufReader::new("# arg: -a: required_without=b: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    required_without: vec!["b".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_required_without_multiple() {
            let mut reader =
                BufReader::new("# arg: -a: required_without=b c: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    required_without: vec!["b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_required_without_multiple_repeat() {
            let mut reader = BufReader::new(
                "# arg: -a: required_without=b: required_without=c: test desc\n".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    required_without: vec!["b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_required_without_all() {
            let mut reader =
                BufReader::new("# arg: -a: required_without_all=b c: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    required_without_all: vec!["b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_required_if_eq() {
            let mut reader =
                BufReader::new("# arg: -a: required_if_eq=b c=5: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    required_if_eq: HashMap::from_iter(vec![
                        ("b".to_string(), "".to_string()),
                        ("c".to_string(), "5".to_string())
                    ]),
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_required_if_eq_all() {
            let mut reader =
                BufReader::new("# arg: -a: required_if_eq_all=b c=5 d=10: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    required_if_eq_all: HashMap::from_iter(vec![
                        ("b".to_string(), "".to_string()),
                        ("c".to_string(), "5".to_string()),
                        ("d".to_string(), "10".to_string())
                    ]),
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_with_default() {
            let mut reader = BufReader::new("# arg: -a: default=5: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: true,
                    default: Some("5".to_string()),
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_multiline_between_options_using_repeat() {
            let mut reader = BufReader::new(
                "# arg: -a: type=int\n# arg: -a: delimiter=,\n# arg: -a: test desc\n".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    arg_type: SyntaxOptArgType::Integer,
                    value_delimiter: Some(','),
                    required: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_multiline_between_options_using_plus() {
            let mut reader = BufReader::new(
                "# arg: -a: type=int\n# +: delimiter=,\n# +: test desc\n".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    arg_type: SyntaxOptArgType::Integer,
                    value_delimiter: Some(','),
                    required: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_multiline_description_using_repeat() {
            let mut reader =
                BufReader::new("# arg: -a: test desc\n# arg: -a: continued desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc\ncontinued desc".to_string()),
                    required: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arg_multiline_description_using_plus() {
            let mut reader =
                BufReader::new("# arg: -a: test desc\n# +: continued desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc\ncontinued desc".to_string()),
                    required: true,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn opt_simple_short() {
            let mut reader = BufReader::new("# opt: -a: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 1);

            let arg = &syntax.parameters[0];
            assert_eq!(
                arg,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    required: false,
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_simple() {
            let mut reader = BufReader::new("# arggroup: a_group: a\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    parameters: vec!["a".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_multiple() {
            let mut reader =
                BufReader::new("# arggroup: a_group: multiple=true: a b c\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    multiple: true,
                    parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_required() {
            let mut reader =
                BufReader::new("# arggroup: a_group: required=true: a b c\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    required: true,
                    parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_conflicts_with() {
            let mut reader =
                BufReader::new("# arggroup: a_group: conflicts_with=b_group: a b c\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    conflicts_with: vec!["b_group".to_string()],
                    parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_requires() {
            let mut reader =
                BufReader::new("# arggroup: a_group: requires=b_group: a b c\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    requires: vec!["b_group".to_string()],
                    parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_repeat() {
            let mut reader = BufReader::new(
                "# arggroup: a_group: a b c\n# arggroup: a_group: d e f\n".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();

            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    parameters: vec![
                        "a".to_string(),
                        "b".to_string(),
                        "c".to_string(),
                        "d".to_string(),
                        "e".to_string(),
                        "f".to_string()
                    ],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_repeat_plus() {
            let mut reader = BufReader::new("# arggroup: a_group: a b c\n# +: d e f\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();

            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    parameters: vec![
                        "a".to_string(),
                        "b".to_string(),
                        "c".to_string(),
                        "d".to_string(),
                        "e".to_string(),
                        "f".to_string()
                    ],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn arggroup_repeat_with_required() {
            let mut reader = BufReader::new(
                "# arggroup: a_group: required=true\n# arggroup: a_group: a b c\n".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some(), "Details are not present");
            let details = details.unwrap();

            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();

            assert_eq!(syntax.groups.len(), 1);

            let group = &syntax.groups[0];
            assert_eq!(
                group,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    required: true,
                    parameters: vec!["a".to_string(), "b".to_string(), "c".to_string(),],
                    ..Default::default()
                }
            );
        }

        #[test]
        fn complex_multiline_everywhere() {
            let mut reader = BufReader::new(
                "# category: test cat\n# +: more cat\n# autocompletion: true\n# argparser: true\n# sync_update: false\n# help: test help\n# +: more help\n# arg: -a: type=int\n# +: delimiter=,\n# +: test desc\n# opt: -b: type=string\n# +: delimiter=|\n# +: test desc\n# arggroup: a_group: multiple=true: a".as_bytes(),
            );
            let details = PathCommandFileDetails::from_source_file_header(
                &mut reader,
                &ConfigErrorHandler::noop(),
            );

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(
                details.category,
                Some(vec!["test cat".to_string(), "more cat".to_string()])
            );
            assert_eq!(details.help, Some("test help\nmore help".to_string()));
            assert!(matches!(
                details.autocompletion,
                CommandAutocompletion::Full
            ));
            assert!(details.argparser);
            assert!(!details.sync_update);
            assert!(details.syntax.is_some(), "Syntax is not present");

            let syntax = details.syntax.unwrap();
            assert_eq!(syntax.parameters.len(), 2);

            let arg_a = &syntax.parameters[0];
            assert_eq!(
                arg_a,
                &SyntaxOptArg {
                    names: vec!["-a".to_string()],
                    desc: Some("test desc".to_string()),
                    arg_type: SyntaxOptArgType::Integer,
                    value_delimiter: Some(','),
                    required: true,
                    ..Default::default()
                }
            );

            let arg_b = &syntax.parameters[1];
            assert_eq!(
                arg_b,
                &SyntaxOptArg {
                    names: vec!["-b".to_string()],
                    desc: Some("test desc".to_string()),
                    arg_type: SyntaxOptArgType::String,
                    value_delimiter: Some('|'),
                    required: false,
                    ..Default::default()
                }
            );

            assert_eq!(syntax.groups.len(), 1);

            let group_a = &syntax.groups[0];
            assert_eq!(
                group_a,
                &SyntaxGroup {
                    name: "a_group".to_string(),
                    multiple: true,
                    parameters: vec!["a".to_string()],
                    ..Default::default()
                }
            );
        }

        mod error_handling {
            use super::*;

            #[test]
            fn test_invalid_value_type_boolean() {
                let mut reader = BufReader::new("# autocompletion: not_a_bool\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(err.kind(), ConfigErrorKind::MetadataHeaderInvalidValueType)
                            && err.lineno() == 1
                            && err.context_str("key") == "autocompletion"
                            && err.context_str("value") == "not_a_bool"
                            && err.context_str("expected") == "boolean"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_missing_help() {
                let mut reader = BufReader::new("# category: test\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderMissingHelp
                    )),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_missing_syntax() {
                let mut reader = BufReader::new("# help: test help\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderMissingSyntax
                    )),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_duplicate_key() {
                let mut reader =
                    BufReader::new("# autocompletion: true\n# autocompletion: false\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(err.kind(), ConfigErrorKind::MetadataHeaderDuplicateKey)
                            && err.lineno() == 2
                            && err.context_str("key") == "autocompletion"
                            && err.context_usize("prev_lineno") == 1
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_unknown_key() {
                let mut reader =
                    BufReader::new("# category: test\n# unknown_key: value\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(err.kind(), ConfigErrorKind::MetadataHeaderUnknownKey)
                            && err.lineno() == 2
                            && err.context_str("key") == "unknown_key"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_group_empty_part() {
                let mut reader = BufReader::new("# arggroup: test_group: :\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(err.kind(), ConfigErrorKind::MetadataHeaderGroupEmptyPart)
                            && err.context_str("group") == "test_group"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_group_unknown_config_key() {
                let mut reader =
                    BufReader::new("# arggroup: test_group: unknown_key=value\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(
                            err.kind(),
                            ConfigErrorKind::MetadataHeaderGroupUnknownConfigKey
                        ) && err.context_str("group") == "test_group"
                            && err.context_str("config_key") == "unknown_key"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_group_missing_parameters() {
                let mut reader = BufReader::new("# arggroup: test_group:\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(
                            err.kind(),
                            ConfigErrorKind::MetadataHeaderGroupMissingParameters
                        ) && err.context_str("group") == "test_group"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_parameter_empty_part() {
                let mut reader = BufReader::new("# arg: test_param: :\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(
                            err.kind(),
                            ConfigErrorKind::MetadataHeaderParameterEmptyPart
                        ) && err.context_str("parameter") == "test_param"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_parameter_invalid_key_value() {
                let mut reader =
                    BufReader::new("# arg: test_param: delimiter=invalid\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();

                assert!(
                    errors.iter().any(|err| {
                        matches!(
                            err.kind(),
                            ConfigErrorKind::MetadataHeaderParameterInvalidKeyValue
                        ) && err.context_str("parameter") == "test_param"
                            && err.context_str("key") == "delimiter"
                            && err.context_str("value") == "invalid"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_parameter_unknown_config_key() {
                let mut reader =
                    BufReader::new("# arg: test_param: unknown_key=value\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();
                assert!(
                    errors.iter().any(|err| {
                        matches!(
                            err.kind(),
                            ConfigErrorKind::MetadataHeaderParameterUnknownConfigKey
                        ) && err.context_str("parameter") == "test_param"
                            && err.context_str("config_key") == "unknown_key"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_parameter_missing_description() {
                let mut reader = BufReader::new("# arg: test_param:\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();
                assert!(
                    errors.iter().any(|err| {
                        matches!(
                            err.kind(),
                            ConfigErrorKind::MetadataHeaderParameterMissingDescription
                        ) && err.context_str("parameter") == "test_param"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_continue_without_key() {
                let mut reader = BufReader::new("# +: continued value\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();
                assert!(
                    errors.iter().any(|err| {
                        matches!(
                            err.kind(),
                            ConfigErrorKind::MetadataHeaderContinueWithoutKey
                        ) && err.lineno() == 1
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }

            #[test]
            fn test_metadata_header_missing_subkey() {
                let mut reader = BufReader::new("# arg:\n".as_bytes());

                let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
                let _ =
                    PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
                let errors = error_handler.errors();
                assert!(
                    errors.iter().any(|err| {
                        matches!(err.kind(), ConfigErrorKind::MetadataHeaderMissingSubkey)
                            && err.lineno() == 1
                            && err.context_str("key") == "arg"
                    }),
                    "Did not find expected error, found: {:?}",
                    errors
                );
            }
        }
    }
}
