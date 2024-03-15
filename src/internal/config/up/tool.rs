use std::collections::HashMap;
use std::path::PathBuf;

use itertools::any;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfig;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpConfigBundler;
use crate::internal::config::up::UpConfigCustom;
use crate::internal::config::up::UpConfigGolang;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigNix;
use crate::internal::config::up::UpConfigNodejs;
use crate::internal::config::up::UpConfigPython;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::dynenv::update_dynamic_env_for_command;

#[derive(Debug, Deserialize, Clone)]
pub enum UpConfigTool {
    And(Vec<UpConfigTool>),
    // TODO: Apt(UpConfigApt),
    Bash(UpConfigAsdfBase),
    Bundler(UpConfigBundler),
    Custom(UpConfigCustom),
    // TODO: Dnf(UpConfigDnf),
    Go(UpConfigGolang),
    Homebrew(UpConfigHomebrew),
    // TODO: Java(UpConfigAsdfBase), // JAVA_HOME
    // TODO: Kotlin(UpConfigAsdfBase), // KOTLIN_HOME
    Nix(UpConfigNix),
    Nodejs(UpConfigNodejs),
    Or(Vec<UpConfigTool>),
    // TODO: Pacman(UpConfigPacman),
    Python(UpConfigPython),
    Ruby(UpConfigAsdfBase),
    Rust(UpConfigAsdfBase),
    Terraform(UpConfigAsdfBase),
}

// Generic function to create a hashmap with a single key/value pair.
fn create_hashmap<T>(key: &str, value: T) -> HashMap<String, T> {
    let mut new_obj = HashMap::new();
    new_obj.insert(key.to_string(), value);
    new_obj
}

impl Serialize for UpConfigTool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        // When serializing, we want to create a yaml object with the key being the UpConfigTool
        // type and the value being the UpConfigTool struct.
        match self {
            UpConfigTool::And(configs) => create_hashmap("and", configs).serialize(serializer),
            UpConfigTool::Bash(config) => create_hashmap("bash", config).serialize(serializer),
            UpConfigTool::Bundler(config) => {
                create_hashmap("bundler", config).serialize(serializer)
            }
            UpConfigTool::Custom(config) => create_hashmap("custom", config).serialize(serializer),
            UpConfigTool::Go(config) => create_hashmap("go", config).serialize(serializer),
            UpConfigTool::Homebrew(config) => {
                create_hashmap("homebrew", config).serialize(serializer)
            }
            UpConfigTool::Nix(config) => create_hashmap("nix", config).serialize(serializer),
            UpConfigTool::Nodejs(config) => create_hashmap("nodejs", config).serialize(serializer),
            UpConfigTool::Or(configs) => create_hashmap("or", configs).serialize(serializer),
            UpConfigTool::Python(config) => create_hashmap("python", config).serialize(serializer),
            UpConfigTool::Ruby(config) => create_hashmap("ruby", config).serialize(serializer),
            UpConfigTool::Rust(config) => create_hashmap("rust", config).serialize(serializer),
            UpConfigTool::Terraform(config) => {
                create_hashmap("terraform", config).serialize(serializer)
            }
        }
    }
}

impl UpConfigTool {
    pub fn from_config_value(up_name: &str, config_value: Option<&ConfigValue>) -> Option<Self> {
        match up_name {
            "and" | "or" => {
                let upconfig = match UpConfig::from_config_value(config_value.cloned()) {
                    Some(upconfig) => upconfig,
                    None => return None,
                };

                if upconfig.steps.is_empty() {
                    None
                } else if up_name == "and" {
                    Some(UpConfigTool::And(upconfig.steps))
                } else {
                    Some(UpConfigTool::Or(upconfig.steps))
                }
            }
            "bash" => Some(UpConfigTool::Bash(
                UpConfigAsdfBase::from_config_value_with_url(
                    "bash",
                    "https://github.com/XaF/asdf-bash",
                    config_value,
                ),
            )),
            "bundler" | "bundle" => Some(UpConfigTool::Bundler(
                UpConfigBundler::from_config_value(config_value),
            )),
            "custom" => Some(UpConfigTool::Custom(UpConfigCustom::from_config_value(
                config_value,
            ))),
            "go" | "golang" => Some(UpConfigTool::Go(UpConfigGolang::from_config_value(
                config_value,
            ))),
            "homebrew" | "brew" => Some(UpConfigTool::Homebrew(
                UpConfigHomebrew::from_config_value(config_value),
            )),
            "nix" => Some(UpConfigTool::Nix(UpConfigNix::from_config_value(
                config_value,
            ))),
            "nodejs" | "node" => Some(UpConfigTool::Nodejs(UpConfigNodejs::from_config_value(
                config_value,
            ))),
            "python" => Some(UpConfigTool::Python(UpConfigPython::from_config_value(
                config_value,
            ))),
            "ruby" => Some(UpConfigTool::Ruby(UpConfigAsdfBase::from_config_value(
                "ruby",
                config_value,
            ))),
            "rust" => Some(UpConfigTool::Rust(UpConfigAsdfBase::from_config_value(
                "rust",
                config_value,
            ))),
            "terraform" => Some(UpConfigTool::Terraform(
                UpConfigAsdfBase::from_config_value("terraform", config_value),
            )),
            _ => None,
        }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        match self {
            UpConfigTool::And(_) | UpConfigTool::Or(_) => {}
            _ => {
                // Update the dynamic environment so that if anything has changed
                // the command can consider it right away
                update_dynamic_env_for_command(".");
            }
        }

        match self {
            UpConfigTool::And(configs) => {
                for config in configs {
                    config.up(options, progress_handler)?;
                }
                Ok(())
            }
            UpConfigTool::Bash(config) => config.up(options, progress_handler),
            UpConfigTool::Bundler(config) => config.up(progress_handler),
            UpConfigTool::Custom(config) => config.up(progress_handler),
            UpConfigTool::Go(config) => config.up(options, progress_handler),
            UpConfigTool::Homebrew(config) => config.up(options, progress_handler),
            UpConfigTool::Nix(config) => config.up(options, progress_handler),
            UpConfigTool::Nodejs(config) => config.up(options, progress_handler),
            UpConfigTool::Or(configs) => {
                // We stop at the first successful up, we only return
                // an error if all the configs failed.
                let mut result = Ok(());
                for config in configs {
                    if config.is_available() {
                        result = config.up(options, progress_handler);
                        if result.is_ok() {
                            break;
                        }
                    }
                }
                result
            }
            UpConfigTool::Python(config) => config.up(options, progress_handler),
            UpConfigTool::Ruby(config) => config.up(options, progress_handler),
            UpConfigTool::Rust(config) => config.up(options, progress_handler),
            UpConfigTool::Terraform(config) => config.up(options, progress_handler),
        }
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Or(configs) => {
                for config in configs {
                    config.down(progress_handler)?;
                }
                Ok(())
            }
            UpConfigTool::Bash(config) => config.down(progress_handler),
            UpConfigTool::Bundler(config) => config.down(progress_handler),
            UpConfigTool::Custom(config) => config.down(progress_handler),
            UpConfigTool::Go(config) => config.down(progress_handler),
            UpConfigTool::Homebrew(config) => config.down(progress_handler),
            UpConfigTool::Nix(config) => config.down(progress_handler),
            UpConfigTool::Nodejs(config) => config.down(progress_handler),
            UpConfigTool::Python(config) => config.down(progress_handler),
            UpConfigTool::Ruby(config) => config.down(progress_handler),
            UpConfigTool::Rust(config) => config.down(progress_handler),
            UpConfigTool::Terraform(config) => config.down(progress_handler),
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Or(configs) => {
                any(configs, |config| config.is_available())
            }
            UpConfigTool::Homebrew(config) => config.is_available(),
            UpConfigTool::Nix(config) => config.is_available(),
            _ => true,
        }
    }

    pub fn dir(&self) -> Option<String> {
        match self {
            UpConfigTool::Custom(config) => config.dir(),
            _ => None,
        }
    }

    pub fn was_upped(&self) -> bool {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Or(configs) => {
                any(configs, |config| config.was_upped())
            }
            UpConfigTool::Bash(config) => config.was_upped(),
            // UpConfigTool::Bundler(config) => config.was_upped(),
            // UpConfigTool::Custom(config) => config.was_upped(),
            UpConfigTool::Go(config) => config.was_upped(),
            // UpConfigTool::Homebrew(config) => config.was_upped(),
            UpConfigTool::Nix(config) => config.was_upped(),
            UpConfigTool::Nodejs(config) => config.asdf_base.was_upped(),
            UpConfigTool::Python(config) => config.asdf_base.was_upped(),
            UpConfigTool::Ruby(config) => config.was_upped(),
            UpConfigTool::Rust(config) => config.was_upped(),
            UpConfigTool::Terraform(config) => config.was_upped(),
            _ => false,
        }
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        match self {
            UpConfigTool::And(configs) => configs
                .iter()
                .flat_map(|config| config.data_paths())
                .collect(),
            UpConfigTool::Bash(config) => config.data_paths(),
            // UpConfigTool::Bundler(config) => config.data_paths(),
            UpConfigTool::Go(config) => config.data_paths(),
            // UpConfigTool::Homebrew(config) => config.data_paths(),
            UpConfigTool::Nix(config) => config.data_paths(),
            UpConfigTool::Nodejs(config) => config.asdf_base.data_paths(),
            UpConfigTool::Or(configs) => {
                match configs
                    .iter()
                    .find(|config| config.is_available() && config.was_upped())
                {
                    Some(config) => config.data_paths(),
                    None => vec![],
                }
            }
            UpConfigTool::Python(config) => config.asdf_base.data_paths(),
            UpConfigTool::Ruby(config) => config.data_paths(),
            UpConfigTool::Rust(config) => config.data_paths(),
            UpConfigTool::Terraform(config) => config.data_paths(),
            _ => vec![],
        }
    }
}
