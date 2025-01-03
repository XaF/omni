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
use walkdir::WalkDir;

use crate::internal::commands::path::omnipath;
use crate::internal::commands::utils::str_to_bool;
use crate::internal::commands::utils::SplitOnSeparators;
use crate::internal::config;
use crate::internal::config::config_loader;
use crate::internal::config::parser::parse_arg_name;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::utils::is_executable;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigExtendOptions;
use crate::internal::config::OmniConfig;
use crate::internal::config::SyntaxGroup;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgNumValues;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::git::package_path_from_handle;
use crate::internal::workdir;

#[derive(Debug, Clone)]
pub struct PathCommand {
    name: Vec<String>,
    source: String,
    aliases: BTreeMap<Vec<String>, String>,
    file_details: OnceCell<Option<PathCommandFileDetails>>,
}

impl PathCommand {
    pub fn all() -> Vec<Self> {
        Self::aggregate_commands_from_path(&omnipath())
    }

    pub fn local() -> Vec<Self> {
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
        let local_config = if suggest_config_value.is_null() {
            cfg
        } else {
            let mut local_config = config_loader(".").raw_config.clone();
            local_config.extend(
                suggest_config_value.clone(),
                ConfigExtendOptions::new(),
                vec![],
            );
            OmniConfig::from_config_value(&local_config)
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

        Self::aggregate_commands_from_path(&local_paths)
    }

    fn aggregate_commands_from_path(paths: &Vec<String>) -> Vec<Self> {
        let mut all_commands: Vec<PathCommand> = Vec::new();
        let mut known_sources: HashMap<String, usize> = HashMap::new();

        for path in paths {
            // Aggregate all the files first, since WalkDir does not sort the list
            let mut files_to_process = Vec::new();
            for entry in WalkDir::new(path).follow_links(true).into_iter().flatten() {
                let filetype = entry.file_type();
                let filepath = entry.path();

                if !filetype.is_file() || !is_executable(filepath) {
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
                    cmd.add_alias(new_command.name(), Some(new_command.source()));
                } else {
                    // Add the new command
                    all_commands.push(new_command.clone());
                    known_sources.insert(new_command.real_source(), all_commands.len() - 1);
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

    pub fn exec(&self, argv: Vec<String>, called_as: Option<Vec<String>>) {
        // Get the source of the command as called
        let source = called_as.map_or(self.source.clone(), |called_as| {
            self.aliases
                .get(&called_as)
                .cloned()
                .unwrap_or(self.source.clone())
        });

        // Execute the command
        let mut command = ProcessCommand::new(source);
        command.args(argv);
        command.exec();

        panic!("Something went wrong");
    }

    pub fn autocompletion(&self) -> bool {
        self.file_details()
            .map(|details| details.autocompletion)
            .unwrap_or(false)
    }

    pub fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) -> Result<(), ()> {
        let mut command = ProcessCommand::new(self.source.clone());
        command.arg("--complete");
        command.args(argv);
        command.env("COMP_CWORD", comp_cword.to_string());

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
            .get_or_init(|| PathCommandFileDetails::from_file(&self.source))
            .as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PathCommandFileDetails {
    category: Option<Vec<String>>,
    help: Option<String>,
    autocompletion: bool,
    syntax: Option<CommandSyntax>,
    sync_update: bool,
    argparser: bool,
    #[serde(skip)]
    errors: Vec<ConfigErrorKind>,
}

impl<'de> Deserialize<'de> for PathCommandFileDetails {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_yaml::Value::deserialize(deserializer)?;

        if let serde_yaml::Value::Mapping(ref mut map) = value {
            let mut errors = vec![];

            // Deserialize the booleans
            let autocompletion = map
                .remove(&serde_yaml::Value::String("autocompletion".to_string()))
                .map_or(false, |v| match bool::deserialize(v.clone()) {
                    Ok(b) => b,
                    Err(_err) => {
                        errors.push(ConfigErrorKind::ValueType {
                            key: "autocompletion".to_string(),
                            expected: "boolean".to_string(),
                            found: v.to_owned(),
                        });
                        false
                    }
                });
            let sync_update = map
                .remove(&serde_yaml::Value::String("sync_update".to_string()))
                .map_or(false, |v| match bool::deserialize(v.clone()) {
                    Ok(b) => b,
                    Err(_err) => {
                        errors.push(ConfigErrorKind::ValueType {
                            key: "sync_update".to_string(),
                            expected: "boolean".to_string(),
                            found: v.to_owned(),
                        });
                        false
                    }
                });
            let argparser = map
                .remove(&serde_yaml::Value::String("argparser".to_string()))
                .map_or(false, |v| match bool::deserialize(v.clone()) {
                    Ok(b) => b,
                    Err(_err) => {
                        errors.push(ConfigErrorKind::ValueType {
                            key: "argparser".to_string(),
                            expected: "boolean".to_string(),
                            found: v.to_owned(),
                        });
                        false
                    }
                });

            // Deserialize the help message
            let help = map
                .remove(&serde_yaml::Value::String("help".to_string()))
                .map_or(None, |v| match String::deserialize(v.clone()) {
                    Ok(s) => Some(s),
                    Err(_err) => {
                        errors.push(ConfigErrorKind::ValueType {
                            key: "help".to_string(),
                            expected: "string".to_string(),
                            found: v.to_owned(),
                        });
                        None
                    }
                });

            // Deserialize the category
            let category = map
                .remove(&serde_yaml::Value::String("category".to_string()))
                .map_or(None, |v| match serde_yaml::Value::deserialize(v.clone()) {
                    Ok(value) => match value {
                        serde_yaml::Value::String(s) => Some(
                            s.split(',')
                                .map(|s| s.trim().to_string())
                                .collect::<Vec<String>>(),
                        ),
                        serde_yaml::Value::Sequence(s) => Some(
                            s.iter()
                                .enumerate()
                                .filter_map(|(idx, entry)| match entry {
                                    serde_yaml::Value::String(s) => Some(s.trim().to_string()),
                                    serde_yaml::Value::Number(n) => Some(n.to_string()),
                                    serde_yaml::Value::Bool(b) => Some(b.to_string()),
                                    _ => {
                                        errors.push(ConfigErrorKind::ValueType {
                                            key: format!("category[{}]", idx),
                                            expected: "string".to_string(),
                                            found: entry.to_owned(),
                                        });
                                        None
                                    }
                                })
                                .collect::<Vec<String>>(),
                        ),
                        _ => {
                            errors.push(ConfigErrorKind::ValueType {
                                key: "category".to_string(),
                                expected: "string or sequence".to_string(),
                                found: value.to_owned(),
                            });
                            None
                        }
                    },
                    Err(_err) => {
                        errors.push(ConfigErrorKind::ValueType {
                            key: "category".to_string(),
                            expected: "string or sequence".to_string(),
                            found: v.to_owned(),
                        });
                        None
                    }
                });

            // Deserialize the syntax
            let syntax = map
                .remove(&serde_yaml::Value::String("syntax".to_string()))
                .map_or(None, |v| {
                    match CommandSyntax::deserialize(v.clone(), "syntax", &mut errors) {
                        Ok(s) => Some(s),
                        Err(_err) => {
                            errors.push(ConfigErrorKind::ValueType {
                                key: "syntax".to_string(),
                                expected: "map".to_string(),
                                found: v.to_owned(),
                            });
                            None
                        }
                    }
                });

            Ok(Self {
                autocompletion,
                sync_update,
                argparser,
                help,
                category,
                syntax,
                errors,
            })
        } else {
            Ok(Self {
                errors: vec![ConfigErrorKind::ValueType {
                    key: "".to_string(),
                    expected: "map".to_string(),
                    found: value,
                }],
                ..Self::default()
            })
        }
    }
}

impl PathCommandFileDetails {
    pub fn from_file(path: &str) -> Option<Self> {
        if let Some(details) = Self::from_metadata_file(path) {
            return Some(details);
        }

        if let Some(details) = Self::from_source_file(path) {
            return Some(details);
        }

        None
    }

    pub fn from_metadata_file(path: &str) -> Option<Self> {
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
                if let Ok(mut md) = serde_yaml::from_reader::<_, Self>(file) {
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
        errors: &mut Vec<ConfigErrorKind>,
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
                errors.push(ConfigErrorKind::MetadataHeaderGroupOrParamEmptyPart {
                    name: group_name.to_string(),
                });
                continue;
            }

            if part.contains('=') {
                let kv: Vec<&str> = part.splitn(2, '=').collect();
                let key = kv[0].to_lowercase();
                if !key.contains(' ') {
                    if !kv.len() == 2 {
                        errors.push(ConfigErrorKind::MetadataHeaderGroupOrParamInvalidPart {
                            name: group_name.to_string(),
                            part: part.to_string(),
                        });
                        continue;
                    }

                    let value = kv[1].trim();

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
                            errors.push(
                                ConfigErrorKind::MetadataHeaderUnknownGroupOrParamConfigKey {
                                    name: group_name.to_string(),
                                    key: key.clone(),
                                },
                            );
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

            if parameters.is_empty() {
                errors.push(ConfigErrorKind::MetadataHeaderGroupMissingParameters {
                    name: group_name.to_string(),
                });
            }
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
        errors: &mut Vec<ConfigErrorKind>,
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
                errors.push(ConfigErrorKind::MetadataHeaderGroupOrParamEmptyPart {
                    name: arg_name.to_string(),
                });
                continue;
            }

            if part.contains('=') {
                let kv: Vec<&str> = part.splitn(2, '=').collect();
                let key = kv[0].to_lowercase();
                if !key.contains(' ') {
                    if !kv.len() == 2 {
                        errors.push(ConfigErrorKind::MetadataHeaderGroupOrParamInvalidPart {
                            name: arg_name.to_string(),
                            part: part.to_string(),
                        });
                        continue;
                    }

                    let value = kv[1].trim();

                    match key.as_str() {
                        "default" => default = Some(value.to_string()),
                        "default_missing_value" => default_missing_value = Some(value.to_string()),
                        "dest" => dest = Some(value.to_string()),
                        "type" => arg_type = value.to_string(),
                        "num_values" => {
                            if let Some(num) = SyntaxOptArgNumValues::from_str(
                                value,
                                &format!("{}.num_values", arg_name),
                                errors,
                            ) {
                                num_values = Some(num)
                            }
                        }
                        "delimiter" => {
                            if value.len() == 1 {
                                value_delimiter = Some(value.chars().next().unwrap());
                            } else {
                                errors.push(ConfigErrorKind::MetadataHeaderParamInvalidKeyValue {
                                    name: arg_name.to_string(),
                                    key: key.clone(),
                                    value: value.to_string(),
                                });
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
                                errors.push(ConfigErrorKind::MetadataHeaderParamInvalidKeyValue {
                                    name: arg_name.to_string(),
                                    key: key.clone(),
                                    value: value.to_string(),
                                });
                            }
                        }
                        _ => {
                            errors.push(
                                ConfigErrorKind::MetadataHeaderUnknownGroupOrParamConfigKey {
                                    name: arg_name.to_string(),
                                    key: key.clone(),
                                },
                            );
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
            errors.push(ConfigErrorKind::MetadataHeaderParamMissingDescription {
                name: arg_name.to_string(),
            });

            None
        } else {
            let description = handle_color_codes(description);
            Some(description)
        };

        let arg_type =
            SyntaxOptArgType::from_str(&arg_type, &format!("{}.arg_type", arg_name), errors)
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

    fn from_source_file_header<R: BufRead>(reader: &mut R) -> Option<Self> {
        let mut errors = vec![];

        let mut autocompletion = false;
        let mut sync_update = false;
        let mut argparser = false;
        let mut category: Option<Vec<String>> = None;
        let mut help_lines: Vec<String> = Vec::new();

        let mut current_key: Option<(String, Option<String>)> = None;
        let mut current_obj: Option<(String, String, String)> = None;
        let mut parameters_data: Vec<(String, String, String)> = vec![];
        let mut group_data: Vec<(String, String)> = vec![];

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
                    "opt" | "arg" | "arggroup" => {
                        let mut subparts = value.splitn(2, ':');
                        let subkey = match subparts.next() {
                            Some(subkey) => subkey.trim().to_string(),
                            None => {
                                errors.push(ConfigErrorKind::MetadataHeaderMissingSubkey {
                                    key: key.clone(),
                                    lineno,
                                });
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
                        errors.push(ConfigErrorKind::MetadataHeaderContinueWithoutKey { lineno });
                        continue;
                    }
                },
                _ => {
                    current_key = Some((key.clone(), subkey.clone()));
                    (key, subkey)
                }
            };

            match (key.as_str(), value) {
                ("category", value) => {
                    let handled_value = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<String>>();
                    match category {
                        Some(ref mut cat) => cat.extend(handled_value),
                        None => category = Some(handled_value),
                    }
                }
                ("autocompletion", value) => {
                    autocompletion = str_to_bool(&value).unwrap_or(false);
                }
                ("sync_update", value) => {
                    sync_update = str_to_bool(&value).unwrap_or(false);
                }
                ("argparser", value) => {
                    argparser = str_to_bool(&value).unwrap_or(false);
                }
                ("help", value) => {
                    help_lines.push(value);
                }
                ("arg", value) | ("opt", value) | ("arggroup", value) if subkey.is_some() => {
                    let subkey = subkey.unwrap();
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
                _ => {
                    errors.push(ConfigErrorKind::MetadataHeaderUnknownKey { key, lineno });
                }
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
                Self::parse_header_arg(is_required, arg_name, value, &mut errors)
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
            .flat_map(|(grp_name, value)| Self::parse_header_group(grp_name, value, &mut errors))
            .collect::<Vec<SyntaxGroup>>();

        let syntax = if parameters.is_empty() && groups.is_empty() {
            errors.push(ConfigErrorKind::MetadataHeaderMissingSyntax);

            None
        } else {
            let mut syntax = CommandSyntax::new();
            syntax.parameters = parameters;
            syntax.groups = groups;

            Some(syntax)
        };

        let help = if help_lines.is_empty() {
            errors.push(ConfigErrorKind::MetadataHeaderMissingHelp);

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
            sync_update,
            errors,
            ..Self::default()
        })
    }

    pub fn from_source_file(path: &str) -> Option<Self> {
        let file = File::open(path);
        if file.is_err() {
            return None;
        }
        let file = file.unwrap();

        let mut reader = BufReader::new(file);

        Self::from_source_file_header(&mut reader)
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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());

            let details = details.unwrap();
            assert_eq!(details.category, None);
            assert_eq!(details.help, None);
            assert!(!details.autocompletion);
            assert_eq!(details.syntax, None);
            assert!(!details.sync_update);
        }

        #[test]
        fn simple() {
            let mut reader = BufReader::new("# category: test cat\n# help: test help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.category, Some(vec!["test cat".to_string()]));
            assert_eq!(details.help, Some("test help".to_string()));
        }

        #[test]
        fn help() {
            let mut reader = BufReader::new("# help: test help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.help, Some("test help".to_string()));
        }

        #[test]
        fn help_multiline_using_repeat() {
            let mut reader =
                BufReader::new("# help: test help\n# help: continued help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.help, Some("test help\ncontinued help".to_string()));
        }

        #[test]
        fn help_multiline_using_plus() {
            let mut reader = BufReader::new("# help: test help\n# +: continued help\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.help, Some("test help\ncontinued help".to_string()));
        }

        #[test]
        fn category() {
            let mut reader = BufReader::new("# category: test cat\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(details.category, Some(vec!["test cat".to_string()]));
        }

        #[test]
        fn category_splits_commas() {
            let mut reader = BufReader::new("# category: test cat, continued cat\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(details.autocompletion);
        }

        #[test]
        fn autocompletion_false() {
            let mut reader = BufReader::new("# autocompletion: false\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(!details.autocompletion);
        }

        #[test]
        fn argparser() {
            let mut reader = BufReader::new("# argparser: true\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(details.argparser);
        }

        #[test]
        fn argparser_false() {
            let mut reader = BufReader::new("# argparser: false\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(!details.argparser);
        }

        #[test]
        fn sync_update() {
            let mut reader = BufReader::new("# sync_update: true\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(details.sync_update);
        }

        #[test]
        fn sync_update_false() {
            let mut reader = BufReader::new("# sync_update: false\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert!(!details.sync_update);
        }

        #[test]
        fn arg_simple_short() {
            let mut reader = BufReader::new("# arg: -a: test desc\n".as_bytes());
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

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
            let details = PathCommandFileDetails::from_source_file_header(&mut reader);

            assert!(details.is_some());
            let details = details.unwrap();

            assert_eq!(
                details.category,
                Some(vec!["test cat".to_string(), "more cat".to_string()])
            );
            assert_eq!(details.help, Some("test help\nmore help".to_string()));
            assert!(details.autocompletion);
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
    }
}
