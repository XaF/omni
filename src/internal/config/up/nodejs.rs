use std::path::PathBuf;
use std::str::FromStr;

use node_semver::Range as semverRange;
use package_json::PackageJsonManager;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::ConfigValue;

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfigNodejs {
    pub asdf_base: UpConfigAsdfBase,
}

impl Serialize for UpConfigNodejs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        self.asdf_base.serialize(serializer)
    }
}

impl UpConfigNodejs {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut asdf_base = UpConfigAsdfBase::from_config_value("nodejs", config_value);
        asdf_base.add_detect_version_func(detect_version_from_package_json);
        asdf_base.add_detect_version_func(detect_version_from_nvmrc);

        Self { asdf_base }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        self.asdf_base.up(options, progress_handler)
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        self.asdf_base.down(progress_handler)
    }
}

fn detect_version_from_package_json(_tool_name: String, path: PathBuf) -> Option<String> {
    if path
        .to_str()
        .unwrap()
        .to_string()
        .contains("/node_modules/")
    {
        return None;
    }

    let version_file_path = path.join("package.json");
    if !version_file_path.exists() || version_file_path.is_dir() {
        return None;
    }

    let mut manager = PackageJsonManager::with_file_path(version_file_path);

    let pkgfile = manager.read_ref();
    if pkgfile.is_err() {
        return None;
    }
    let pkgfile = pkgfile.unwrap();

    pkgfile.engines.as_ref()?;
    let engines = pkgfile.engines.clone().unwrap();

    if let Some(node_version) = engines.get("node") {
        if let Ok(_requirements) = semverRange::from_str(node_version) {
            return Some(node_version.to_string());
        }
    }

    None
}

fn detect_version_from_nvmrc(_tool_name: String, path: PathBuf) -> Option<String> {
    if path
        .to_str()
        .unwrap()
        .to_string()
        .contains("/node_modules/")
    {
        return None;
    }

    let version_file_path = path.join(".nvmrc");
    if !version_file_path.exists() || version_file_path.is_dir() {
        return None;
    }

    match std::fs::read_to_string(version_file_path) {
        Ok(version) => Some(version.trim().to_string()),
        Err(_) => None,
    }
}
