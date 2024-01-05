use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpConfigBundler;
use crate::internal::config::up::UpConfigCustom;
use crate::internal::config::up::UpConfigGolang;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigNodejs;
use crate::internal::config::up::UpConfigPython;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;

#[derive(Debug, Deserialize, Clone)]
pub enum UpConfigTool {
    // TODO: Apt(UpConfigApt),
    Bash(UpConfigAsdfBase),
    Bundler(UpConfigBundler),
    Custom(UpConfigCustom),
    // TODO: Dnf(UpConfigDnf),
    Go(UpConfigGolang),
    Homebrew(UpConfigHomebrew),
    // TODO: Java(UpConfigAsdfBase), // JAVA_HOME
    // TODO: Kotlin(UpConfigAsdfBase), // KOTLIN_HOME
    Nodejs(UpConfigNodejs),
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
            UpConfigTool::Bash(config) => create_hashmap("bash", config).serialize(serializer),
            UpConfigTool::Bundler(config) => {
                create_hashmap("bundler", config).serialize(serializer)
            }
            UpConfigTool::Custom(config) => create_hashmap("custom", config).serialize(serializer),
            UpConfigTool::Go(config) => create_hashmap("go", config).serialize(serializer),
            UpConfigTool::Homebrew(config) => {
                create_hashmap("homebrew", config).serialize(serializer)
            }
            UpConfigTool::Nodejs(config) => create_hashmap("nodejs", config).serialize(serializer),
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

    pub fn up(&self, options: &UpOptions, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        match self {
            UpConfigTool::Bash(config) => config.up(options, progress),
            UpConfigTool::Bundler(config) => config.up(progress),
            UpConfigTool::Custom(config) => config.up(progress),
            UpConfigTool::Go(config) => config.up(options, progress),
            UpConfigTool::Homebrew(config) => config.up(options, progress),
            UpConfigTool::Nodejs(config) => config.up(options, progress),
            UpConfigTool::Python(config) => config.up(options, progress),
            UpConfigTool::Ruby(config) => config.up(options, progress),
            UpConfigTool::Rust(config) => config.up(options, progress),
            UpConfigTool::Terraform(config) => config.up(options, progress),
        }
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        match self {
            UpConfigTool::Bash(config) => config.down(progress),
            UpConfigTool::Bundler(config) => config.down(progress),
            UpConfigTool::Custom(config) => config.down(progress),
            UpConfigTool::Go(config) => config.down(progress),
            UpConfigTool::Homebrew(config) => config.down(progress),
            UpConfigTool::Nodejs(config) => config.down(progress),
            UpConfigTool::Python(config) => config.down(progress),
            UpConfigTool::Ruby(config) => config.down(progress),
            UpConfigTool::Rust(config) => config.down(progress),
            UpConfigTool::Terraform(config) => config.down(progress),
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            UpConfigTool::Homebrew(config) => config.is_available(),
            _ => true,
        }
    }

    pub fn dir(&self) -> Option<String> {
        match self {
            UpConfigTool::Custom(config) => config.dir(),
            _ => None,
        }
    }
}
