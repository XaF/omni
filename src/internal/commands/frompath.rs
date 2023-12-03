use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::Command as ProcessCommand;

use once_cell::sync::OnceCell;
use walkdir::WalkDir;

use crate::internal::commands::path::omnipath;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;

#[derive(Debug, Clone)]
pub struct PathCommand {
    name: Vec<String>,
    source: String,
    aliases: BTreeMap<Vec<String>, String>,
    file_details: OnceCell<Option<PathCommandFileDetails>>,
}

impl PathCommand {
    pub fn all() -> Vec<Self> {
        let mut all_commands: Vec<PathCommand> = Vec::new();
        let mut known_sources: HashMap<String, usize> = HashMap::new();

        for path in &omnipath() {
            // Aggregate all the files first, since WalkDir does not sort the list
            let mut files_to_process = Vec::new();
            for entry in WalkDir::new(path).follow_links(true).into_iter().flatten() {
                let filetype = entry.file_type();
                let filepath = entry.path();

                if !filetype.is_file() || !Self::is_executable(filepath) {
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

    fn is_executable(path: &std::path::Path) -> bool {
        fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
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
        self.file_details()
            .and_then(|details| details.help.clone())
            .map(|lines| lines.join("\n"))
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        self.file_details()
            .and_then(|details| details.syntax.clone())
    }

    pub fn category(&self) -> Option<Vec<String>> {
        self.file_details()
            .and_then(|details| details.category.clone())
    }

    pub fn exec(&self, argv: Vec<String>, called_as: Option<Vec<String>>) {
        // Get the source of the command as called
        let source = called_as.map_or(self.source.clone(), |called_as| {
            self.aliases
                .get(&called_as)
                .map(|source| source.clone())
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

    pub fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) {
        let mut command = ProcessCommand::new(self.source.clone());
        command.arg("--complete");
        command.args(argv);
        command.env("COMP_CWORD", comp_cword.to_string());
        command.exec();

        panic!("Something went wrong");
    }

    fn file_details(&self) -> Option<&PathCommandFileDetails> {
        self.file_details
            .get_or_init(|| PathCommandFileDetails::from_file(&self.source))
            .as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct PathCommandFileDetails {
    category: Option<Vec<String>>,
    help: Option<Vec<String>>,
    autocompletion: bool,
    syntax: Option<CommandSyntax>,
}

impl PathCommandFileDetails {
    pub fn from_file(path: &str) -> Option<Self> {
        let mut autocompletion = false;
        let mut category = None;
        let mut help_lines = Vec::new();

        let mut parameters: Vec<SyntaxOptArg> = vec![];

        // let mut arguments_order = Vec::new();
        // let mut arguments = HashMap::new();

        // let mut options_order = Vec::new();
        // let mut options = HashMap::new();

        let mut reading_help = false;

        let file = File::open(path);
        if file.is_err() {
            return None;
        }
        let file = file.unwrap();

        let reader = BufReader::new(file);
        for line in reader.lines() {
            if line.is_err() {
                // If the file is not readable, skip trying to read the headers
                return None;
            }
            let line = line.unwrap();

            // Early exit condition to stop reading when we don't need to anymore
            if !line.starts_with('#') || (reading_help && !line.starts_with("# help:")) {
                break;
            }

            if line.starts_with("# category:") {
                let cat: Vec<String> = line
                    .strip_prefix("# category:")
                    .unwrap()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                category = Some(cat);
            } else if line.starts_with("# autocompletion:") {
                let completion = line
                    .strip_prefix("# autocompletion:")
                    .unwrap()
                    .trim()
                    .to_lowercase();
                autocompletion = completion == "true";
            } else if line.starts_with("# help:") {
                reading_help = true;
                let help_line =
                    handle_color_codes(line.strip_prefix("# help:").unwrap().trim().to_string());
                help_lines.push(help_line);
            } else if line.starts_with("# arg:") || line.starts_with("# opt:") {
                let param_required = line.starts_with("# arg:");
                let param = line
                    .strip_prefix("# arg:")
                    .or_else(|| line.strip_prefix("# opt:"))
                    .unwrap()
                    .splitn(2, ':')
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<String>>();
                if param.len() != 2 {
                    continue;
                }

                let param_name = param[0].clone();
                let param_desc = param[1].clone();

                if let Some(cur_param_desc) = parameters
                    .iter_mut()
                    .find(|p| p.name == param_name && p.required == param_required)
                {
                    cur_param_desc.desc = Some(format!(
                        "{}\n{}",
                        cur_param_desc.desc.clone().unwrap_or(String::new()),
                        param_desc
                    ));
                } else {
                    parameters.push(SyntaxOptArg::new(
                        param_name,
                        Some(param_desc),
                        param_required,
                    ))
                }
            }
        }

        let mut syntax = match parameters.len() {
            0 => None,
            _ => Some(CommandSyntax::new()),
        };

        if !parameters.is_empty() {
            for parameter in &mut parameters {
                if let Some(desc) = &parameter.desc {
                    parameter.desc = Some(handle_color_codes(desc.clone()));
                }
            }
            syntax.as_mut().unwrap().parameters = parameters;
        }

        // // Return the file details
        Some(PathCommandFileDetails {
            category,
            help: Some(help_lines),
            autocompletion,
            syntax,
        })
    }
}

fn handle_color_codes(string: String) -> String {
    string
        .replace("\\033[", "\x1B[")
        .replace("\\e[", "\x1B[")
        .replace("\\x1B[", "\x1B[")
}
