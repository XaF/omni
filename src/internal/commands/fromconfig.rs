use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;
use std::process::Command as ProcessCommand;

use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::commands::utils::abs_path;
use crate::internal::commands::utils::split_name;
use crate::internal::config::config;
use crate::internal::config::CommandDefinition;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigSource;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;

#[derive(Debug, Clone)]
pub struct ConfigCommand {
    name: Vec<String>,
    details: CommandDefinition,
}

impl ConfigCommand {
    pub fn all() -> Vec<Self> {
        Self::all_commands(config(".").commands.clone(), vec![])
    }

    fn all_commands(
        command_definitions: HashMap<String, CommandDefinition>,
        parent_aliases: Vec<String>,
    ) -> Vec<Self> {
        let mut all_commands = Vec::new();

        for (command_name, command_details) in command_definitions {
            let name = if parent_aliases.is_empty() {
                command_name.clone()
            } else {
                format!("{} {}", parent_aliases[0], command_name)
            };

            let mut aliases = Vec::new();
            if parent_aliases.is_empty() {
                aliases = command_details.aliases.clone();
            } else {
                for parent_alias in parent_aliases[1..].iter() {
                    aliases.push(format!("{} {}", parent_alias, command_name));
                }
                for command_alias in command_details.aliases.iter() {
                    for parent_alias in parent_aliases.iter() {
                        aliases.push(format!("{} {}", parent_alias, command_alias));
                    }
                }
            }

            let mut command_details = command_details.clone();
            command_details.aliases = aliases.clone();

            all_commands.push(Self::new(name.clone(), command_details.clone()));

            if let Some(subcommands) = command_details.subcommands {
                let mut parent_aliases = vec![name];
                parent_aliases.extend(aliases.clone());

                all_commands.extend(Self::all_commands(subcommands, parent_aliases));
            }
        }

        all_commands
    }

    pub fn new(name: String, details: CommandDefinition) -> Self {
        let mut name = vec![name];

        name = name.into_iter().flat_map(|n| split_name(&n, " ")).collect();

        if config(".").config_commands.split_on_dash {
            name = name.into_iter().flat_map(|n| split_name(&n, "-")).collect();
        }
        if config(".").config_commands.split_on_slash {
            name = name.into_iter().flat_map(|n| split_name(&n, "/")).collect();
        }

        ConfigCommand { name, details }
    }

    pub fn name(&self) -> Vec<String> {
        self.name.clone()
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        let mut aliases: Vec<Vec<String>> = Vec::new();

        for alias in self.details.aliases.iter() {
            let mut alias = vec![alias.to_string()];

            alias = alias
                .into_iter()
                .flat_map(|n| split_name(&n, " "))
                .collect();

            if config(".").config_commands.split_on_dash {
                alias = alias
                    .into_iter()
                    .flat_map(|n| split_name(&n, "-"))
                    .collect();
            }
            if config(".").config_commands.split_on_slash {
                alias = alias
                    .into_iter()
                    .flat_map(|n| split_name(&n, "/"))
                    .collect();
            }

            aliases.push(alias);
        }

        aliases
    }

    pub fn source(&self) -> String {
        match self.details.source {
            ConfigSource::Default => "/default".to_string(),
            ConfigSource::File(ref path) => path.clone(),
            ConfigSource::Package(ref path_entry_config) => path_entry_config.full_path.clone(),
            ConfigSource::Null => "/null".to_string(),
        }
    }

    pub fn help(&self) -> Option<String> {
        self.details.desc.clone()
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        self.details.syntax.clone()
    }

    pub fn category(&self) -> Option<Vec<String>> {
        let mut category = vec!["Configuration".to_string()];

        if let Some(cat) = &self.details.category {
            category.extend(cat.clone());
        }

        let source = abs_or_rel_path(&self.source());
        category.insert(0, source);

        Some(category)
    }

    pub fn exec_dir(&self) -> Result<PathBuf, String> {
        let config_file = self.source();
        let config_dir = abs_path(
            Path::new(&config_file)
                .parent()
                .expect("Failed to get config directory"),
        );

        let exec_dir = if let Some(dir) = self.details.dir.clone() {
            abs_path(config_dir.join(dir))
        } else {
            config_dir.to_path_buf()
        };

        // Raise error if the resulting directory is not in the config directory
        if !exec_dir.starts_with(config_dir.clone()) {
            return Err(format!(
                "directory {} is not a subpath of {}",
                exec_dir.display(),
                config_dir.display()
            ));
        }

        Ok(exec_dir)
    }

    pub fn exec(&self, argv: Vec<String>) {
        // Get the current directory so we can store it in a variable
        let current_dir = std::env::current_dir().expect("Failed to get current directory");
        std::env::set_var("OMNI_CWD", current_dir.display().to_string());

        // Raise error if the resulting directory is not in the config directory
        match self.exec_dir() {
            Ok(exec_dir) => {
                if std::env::set_current_dir(exec_dir.clone()).is_err() {
                    omni_error!(format!(
                        "failed to change directory to {}",
                        exec_dir.display()
                    ));
                    exit(1);
                }
            }
            Err(err) => {
                omni_error!(err.to_string());
                exit(1);
            }
        }

        ProcessCommand::new("bash")
            .arg("-c")
            .arg(self.details.run.clone())
            .arg(self.source())
            .args(argv)
            .exec();

        panic!("Something went wrong");
    }
}
