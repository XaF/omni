use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command as ProcessCommand;

use regex::Regex;

use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::commands::utils::split_name;
use crate::internal::config::config;
use crate::internal::config::CommandSyntax;
use crate::internal::env::git_env;

#[derive(Debug, Clone)]
pub struct MakefileCommand {
    name: Vec<String>,
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
        let git_env = git_env(path.to_str().unwrap());

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

            if git_env.in_repo() && git_env.root().unwrap() == path.to_str().unwrap() {
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

            if line.starts_with("##@") {
                category = Some(line[3..].trim().to_string());
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

        MakefileCommand {
            name: name,
            category: category,
            desc: desc,
            target: target,
            source: source,
            lineno: lineno,
        }
    }

    pub fn name(&self) -> Vec<String> {
        self.name.clone()
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
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
        let makefile_dir = Path::new(&self.source).parent().unwrap();
        if std::env::set_current_dir(makefile_dir).is_err() {
            println!("Failed to change directory to {}", makefile_dir.display());
        }

        ProcessCommand::new("make")
            .arg("-f")
            .arg(self.source())
            .arg(self.target.clone())
            .args(argv)
            .exec();

        panic!("Something went wrong");
    }
}
