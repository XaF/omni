use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::process::exit;
use std::process::Command as StdCommand;

use regex::Regex;

use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::commands::utils::split_name;
use crate::internal::config::config;
use crate::internal::config::CommandSyntax;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::workdir;
use crate::omni_error;

#[derive(Debug, Clone)]
pub struct MakefileCommand {
    name: Vec<String>,
    orig_name: Option<String>,
    category: Option<String>,
    desc: Option<String>,
    target: String,
    source: String,
    lineno: usize,
}

impl MakefileCommand {
    pub fn all_from_path(path: &str) -> Vec<Self> {
        // Canonicalize the path
        let abs_path = fs::canonicalize(path);
        if abs_path.is_err() {
            return vec![];
        }
        let abs_path = abs_path.unwrap();

        // Convert to path object
        let mut path = Path::new(abs_path.to_str().unwrap());

        // Get the git environment
        let wd = workdir(path.to_str().unwrap());

        let mut commands = vec![];
        while let Some(parent) = path.parent() {
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let filepath = entry.path();

                if !filepath.is_file() {
                    continue;
                }

                let filename = filepath
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_lowercase();

                if filename == "makefile"
                    || filename == "gnumakefile"
                    || filename.starts_with("makefile.")
                    || filename.starts_with("gnumakefile.")
                {
                    commands.extend(Self::all_from_file(filepath.to_str().unwrap()));
                }
            }

            if wd.in_workdir() && wd.root().unwrap() == path.to_str().unwrap() {
                break;
            }

            path = parent;
        }

        commands
    }

    pub fn all_from_file(filepath: &str) -> Vec<Self> {
        let mut commands = vec![];

        // Open the file and read it line by line
        let file = File::open(filepath);
        if file.is_err() {
            return commands;
        }

        let file = file.unwrap();
        let reader = BufReader::new(file);

        // Prepare the target regex
        let target = Regex::new(r"^(?<target>[a-zA-Z_0-9\-\/\/]+):(.*?##\s*(?<desc>.*))?$")
            .expect("Invalid regex pattern?!");

        let mut category = None;
        for (lineno, line) in reader.lines().enumerate() {
            if line.is_err() {
                break;
            }
            let line = line.unwrap();

            if let Some(cat) = line.strip_prefix("##@") {
                category = Some(cat.trim().to_string());
                continue;
            }

            match target.captures(&line) {
                Some(captures) => {
                    let target = captures.name("target").unwrap().as_str().to_string();

                    let desc = captures.name("desc").map(|m| m.as_str().trim().to_string());

                    commands.push(MakefileCommand::new(
                        target,
                        category.clone(),
                        desc,
                        filepath.to_string(),
                        lineno + 1,
                    ));
                }
                None => continue,
            };
        }

        commands
    }

    pub fn new(
        target: String,
        category: Option<String>,
        desc: Option<String>,
        source: String,
        lineno: usize,
    ) -> Self {
        let mut name = vec![target.clone()];
        if config(".").makefile_commands.split_on_dash {
            name = name.into_iter().flat_map(|n| split_name(&n, "-")).collect();
        }
        if config(".").makefile_commands.split_on_slash {
            name = name.into_iter().flat_map(|n| split_name(&n, "/")).collect();
        }

        let orig_name = if name.len() > 1 || name[0] != target {
            Some(target.clone())
        } else {
            None
        };

        MakefileCommand {
            name,
            orig_name,
            category,
            desc,
            target,
            source,
            lineno,
        }
    }

    pub fn name(&self) -> Vec<String> {
        self.name.clone()
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn orig_name(&self) -> Option<String> {
        self.orig_name.clone()
    }

    pub fn source(&self) -> String {
        self.source.clone()
    }

    pub fn lineno(&self) -> usize {
        self.lineno
    }

    pub fn help(&self) -> Option<String> {
        self.desc.clone()
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        None
    }

    pub fn category(&self) -> Option<Vec<String>> {
        let source = abs_or_rel_path(&self.source);
        let mut category = vec![source];

        if let Some(cat) = &self.category {
            category.push(cat.clone());
        }

        Some(category)
    }

    pub fn exec(&self, argv: Vec<String>) {
        // Get the current directory so we can store it in a variable
        let current_dir = std::env::current_dir()
            .expect("Failed to get current directory")
            .to_string_lossy()
            .to_string();

        let makefile_dir = match Path::new(&self.source).parent() {
            Some(p) => p,
            None => {
                omni_error!("failed to get parent directory of {}", self.source);
                exit(1);
            }
        };

        // Execute the command
        match StdCommand::new("make")
            .arg("-f")
            .arg(self.source())
            .arg(self.target.clone())
            .args(argv)
            .env("OMNI_CWD", current_dir)
            .current_dir(makefile_dir)
            .status()
        {
            Ok(status) if status.success() => {
                // TODO: handle metrics about the success
                exit(0);
            }
            Ok(status) => {
                // TODO: handle metrics about the error
                exit(status.code().unwrap_or(1));
            }
            Err(err) => {
                // TODO: handle metrics about the error
                omni_error!("failed to execute make command: {}", err.to_string());
                exit(1);
            }
        }
    }
}
