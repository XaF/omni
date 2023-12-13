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

#[derive(Debug, Serialize, Deserialize, Clone)]
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
