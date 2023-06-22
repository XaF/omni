use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpConfigBundler;
use crate::internal::config::up::UpConfigCustom;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpError;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum UpConfigTool {
    Homebrew(UpConfigHomebrew),
    // TODO: Apt(UpConfigApt),
    // TODO: Dnf(UpConfigDnf),
    // TODO: Pacman(UpConfigPacman),
    Bundler(UpConfigBundler),
    Ruby(UpConfigAsdfBase),
    Rust(UpConfigAsdfBase),
    Go(UpConfigAsdfBase),
    Nodejs(UpConfigAsdfBase),
    Python(UpConfigAsdfBase),
    // TODO: Java(UpConfigAsdfBase), // JAVA_HOME
    // TODO: Kotlin(UpConfigAsdfBase), // KOTLIN_HOME
    Custom(UpConfigCustom),
}

impl UpConfigTool {
    pub fn from_config_value(up_name: &str, config_value: Option<&ConfigValue>) -> Option<Self> {
        match up_name {
            "homebrew" | "brew" => Some(UpConfigTool::Homebrew(
                UpConfigHomebrew::from_config_value(config_value),
            )),
            "bundler" | "bundle" => Some(UpConfigTool::Bundler(
                UpConfigBundler::from_config_value(config_value),
            )),
            "ruby" => Some(UpConfigTool::Ruby(UpConfigAsdfBase::from_config_value(
                "ruby",
                config_value,
            ))),
            "nodejs" | "node" | "npm" => Some(UpConfigTool::Nodejs(
                UpConfigAsdfBase::from_config_value("nodejs", config_value),
            )),
            "rust" => Some(UpConfigTool::Rust(UpConfigAsdfBase::from_config_value(
                "rust",
                config_value,
            ))),
            "go" | "golang" => Some(UpConfigTool::Go(UpConfigAsdfBase::from_config_value(
                "golang",
                config_value,
            ))),
            "python" => Some(UpConfigTool::Python(UpConfigAsdfBase::from_config_value(
                "python",
                config_value,
            ))),
            "custom" => Some(UpConfigTool::Custom(UpConfigCustom::from_config_value(
                config_value,
            ))),
            _ => None,
        }
    }

    pub fn up(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        match self {
            UpConfigTool::Homebrew(config) => config.up(progress),
            UpConfigTool::Bundler(config) => config.up(progress),
            UpConfigTool::Ruby(config) => config.up(progress),
            UpConfigTool::Nodejs(config) => config.up(progress),
            UpConfigTool::Rust(config) => config.up(progress),
            UpConfigTool::Go(config) => config.up(progress),
            UpConfigTool::Python(config) => config.up(progress),
            UpConfigTool::Custom(config) => config.up(progress),
        }
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        match self {
            UpConfigTool::Homebrew(config) => config.down(progress),
            UpConfigTool::Bundler(config) => config.down(progress),
            UpConfigTool::Ruby(config) => config.down(progress),
            UpConfigTool::Nodejs(config) => config.down(progress),
            UpConfigTool::Rust(config) => config.down(progress),
            UpConfigTool::Go(config) => config.down(progress),
            UpConfigTool::Python(config) => config.down(progress),
            UpConfigTool::Custom(config) => config.down(progress),
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            UpConfigTool::Homebrew(config) => config.is_available(),
            _ => true,
        }
    }

    pub fn asdf_tool(&self) -> Option<&UpConfigAsdfBase> {
        match self {
            UpConfigTool::Ruby(config) => Some(config),
            UpConfigTool::Nodejs(config) => Some(config),
            UpConfigTool::Rust(config) => Some(config),
            UpConfigTool::Go(config) => Some(config),
            UpConfigTool::Python(config) => Some(config),
            _ => None,
        }
    }
}
