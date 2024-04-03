use std::collections::HashMap;
use std::path::PathBuf;

use itertools::any;
use itertools::Itertools;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::global_config;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfig;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpConfigBundler;
use crate::internal::config::up::UpConfigCustom;
use crate::internal::config::up::UpConfigGithubRelease;
use crate::internal::config::up::UpConfigGolang;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigNix;
use crate::internal::config::up::UpConfigNodejs;
use crate::internal::config::up::UpConfigPython;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::dynenv::update_dynamic_env_for_command;

/// UpConfigTool represents a tool that can be upped or downed.
/// It can be a single tool or a combination of tools.
#[derive(Debug, Deserialize, Clone)]
pub enum UpConfigTool {
    /// And represents a combination of tools that must all be upped.
    And(Vec<UpConfigTool>),

    /// Any represents a combination of tools where at least one must
    /// be upped. It will try to follow user preferences for ordering
    /// of which tool to handle. If none are available, it will try
    /// the others in the order they are defined. If the selected tool
    /// fails to up, it will try the next one until one is successful
    /// or all have been tried.
    Any(Vec<UpConfigTool>),

    // TODO: Apt(UpConfigApt),
    /// Bash represents the bash tool.
    Bash(UpConfigAsdfBase),

    /// Bundler represents the bundler tool.
    Bundler(UpConfigBundler),

    /// Custom represents a custom tool, where the user can define
    /// a custom command to run to up/down the tool.
    Custom(UpConfigCustom),

    // TODO: Dnf(UpConfigDnf),
    /// GithubRelease represents a tool that can be installed from
    /// a github release.
    GithubRelease(UpConfigGithubRelease),

    /// Go represents the golang tool.
    Go(UpConfigGolang),

    /// Homebrew represents the homebrew tool.
    Homebrew(UpConfigHomebrew),

    // TODO: Java(UpConfigAsdfBase), // JAVA_HOME
    // TODO: Kotlin(UpConfigAsdfBase), // KOTLIN_HOME
    /// Nix represents the nix tool, which can be used to install
    /// packages from the nix package manager.
    Nix(UpConfigNix),

    /// Nodejs represents the nodejs tool.
    Nodejs(UpConfigNodejs),

    /// Or represents a combination of tools where at least one must
    /// be upped. It will up the first tool that is available, and
    /// only try the others if the first one fails.
    Or(Vec<UpConfigTool>),

    // TODO: Pacman(UpConfigPacman),
    /// Python represents the python tool.
    Python(UpConfigPython),

    /// Ruby represents the ruby tool.
    Ruby(UpConfigAsdfBase),

    /// Rust represents the rust tool.
    Rust(UpConfigAsdfBase),

    /// Terraform represents the terraform tool.
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
            UpConfigTool::Any(configs) => create_hashmap("any", configs).serialize(serializer),
            UpConfigTool::Bash(config) => create_hashmap("bash", config).serialize(serializer),
            UpConfigTool::Bundler(config) => {
                create_hashmap("bundler", config).serialize(serializer)
            }
            UpConfigTool::Custom(config) => create_hashmap("custom", config).serialize(serializer),
            UpConfigTool::GithubRelease(config) => {
                create_hashmap("github-release", config).serialize(serializer)
            }
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
            "and" | "any" | "or" => {
                let upconfig = match UpConfig::from_config_value(config_value.cloned()) {
                    Some(upconfig) => upconfig,
                    None => return None,
                };

                if upconfig.steps.is_empty() {
                    None
                } else {
                    match up_name {
                        "and" => Some(UpConfigTool::And(upconfig.steps)),
                        "any" => Some(UpConfigTool::Any(upconfig.steps)),
                        "or" => Some(UpConfigTool::Or(upconfig.steps)),
                        _ => None,
                    }
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
            "github-release" | "github_release" | "githubrelease" | "ghrelease" => Some(
                UpConfigTool::GithubRelease(UpConfigGithubRelease::from_config_value(config_value)),
            ),
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
            UpConfigTool::And(_) | UpConfigTool::Any(_) | UpConfigTool::Or(_) => {}
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
            UpConfigTool::Any(configs) => {
                let mut result = Ok(());
                for config in ordered_configs(configs) {
                    if config.is_available() {
                        result = config.up(options, progress_handler);
                        if result.is_ok() {
                            break;
                        }
                    }
                }
                result
            }
            UpConfigTool::Bash(config) => config.up(options, progress_handler),
            UpConfigTool::Bundler(config) => config.up(progress_handler),
            UpConfigTool::Custom(config) => config.up(progress_handler),
            UpConfigTool::GithubRelease(config) => config.up(options, progress_handler),
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
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                for config in configs {
                    config.down(progress_handler)?;
                }
                Ok(())
            }
            UpConfigTool::Bash(config) => config.down(progress_handler),
            UpConfigTool::Bundler(config) => config.down(progress_handler),
            UpConfigTool::Custom(config) => config.down(progress_handler),
            UpConfigTool::GithubRelease(config) => config.down(progress_handler),
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
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
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
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                any(configs, |config| config.was_upped())
            }
            UpConfigTool::Bash(config) => config.was_upped(),
            // UpConfigTool::Bundler(config) => config.was_upped(),
            // UpConfigTool::Custom(config) => config.was_upped(),
            // UpConfigTool::GithubRelease(config) => config.was_upped(),
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
            UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                match configs
                    .iter()
                    .find(|config| config.is_available() && config.was_upped())
                {
                    Some(config) => config.data_paths(),
                    None => vec![],
                }
            }
            UpConfigTool::Bash(config) => config.data_paths(),
            // UpConfigTool::Bundler(config) => config.data_paths(),
            // UpConfigTool::Custom(config) => config.data_paths(),
            // UpConfigTool::GithubRelease(config) => config.data_paths(),
            UpConfigTool::Go(config) => config.data_paths(),
            // UpConfigTool::Homebrew(config) => config.data_paths(),
            UpConfigTool::Nix(config) => config.data_paths(),
            UpConfigTool::Nodejs(config) => config.asdf_base.data_paths(),
            UpConfigTool::Python(config) => config.asdf_base.data_paths(),
            UpConfigTool::Ruby(config) => config.data_paths(),
            UpConfigTool::Rust(config) => config.data_paths(),
            UpConfigTool::Terraform(config) => config.data_paths(),
            _ => vec![],
        }
    }

    pub fn to_name(&self) -> &str {
        match self {
            UpConfigTool::And(_) => "and",
            UpConfigTool::Any(_) => "any",
            UpConfigTool::Or(_) => "or",
            UpConfigTool::Bash(_) => "bash",
            UpConfigTool::Bundler(_) => "bundler",
            UpConfigTool::Custom(_) => "custom",
            UpConfigTool::GithubRelease(_) => "github-release",
            UpConfigTool::Go(_) => "go",
            UpConfigTool::Homebrew(_) => "homebrew",
            UpConfigTool::Nix(_) => "nix",
            UpConfigTool::Nodejs(_) => "nodejs",
            UpConfigTool::Python(_) => "python",
            UpConfigTool::Ruby(_) => "ruby",
            UpConfigTool::Rust(_) => "rust",
            UpConfigTool::Terraform(_) => "terraform",
        }
    }

    pub fn sort_value(&self) -> i32 {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                // return the minimum sort value of all the configs
                // in the list
                configs
                    .iter()
                    .map(|config| config.sort_value())
                    .min()
                    .unwrap_or(i32::MAX)
            }
            _ => {
                let config = global_config();
                let preferred_tools = &config.up_command.preferred_tools;

                // check what is the position for self.to_name() in
                // preferred_tools and if it is not there, return the
                // max value possible; the lowest value means the
                // highest priority
                let position = preferred_tools
                    .iter()
                    .position(|x| x.to_lowercase() == self.to_name());

                match position {
                    Some(position) => position as i32,
                    None => i32::MAX,
                }
            }
        }
    }
}

fn ordered_configs(configs: &[UpConfigTool]) -> Vec<&UpConfigTool> {
    configs
        .iter()
        .sorted_by(|a, b| a.sort_value().cmp(&b.sort_value()))
        .collect()
}
