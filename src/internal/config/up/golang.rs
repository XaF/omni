use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

use normalize_path::NormalizePath;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils as cache_utils;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::AsdfToolUpVersion;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::env::current_dir;
use crate::internal::workdir;
use crate::internal::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UpConfigGolangSerialized {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_file: Option<String>,
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    upgrade: bool,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    dirs: BTreeSet<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfigGolang {
    pub version: Option<String>,
    pub version_file: Option<String>,
    pub upgrade: bool,
    pub dirs: BTreeSet<String>,
    #[serde(skip)]
    pub asdf_base: OnceCell<UpConfigAsdfBase>,
}

impl Serialize for UpConfigGolang {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        let mut serialized = UpConfigGolangSerialized {
            version: self.version.clone(),
            version_file: self.version_file.clone(),
            upgrade: self.upgrade,
            dirs: self.dirs.clone(),
        };

        if serialized.version.is_none() && serialized.version_file.is_none() {
            serialized.version = Some("latest".to_string());
        }

        serialized.serialize(serializer)
    }
}

impl UpConfigGolang {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut version = None;
        let mut version_file = None;
        let mut dirs = BTreeSet::new();
        let mut upgrade = false;

        if let Some(config_value) = config_value {
            if let Some(value) = config_value.as_str() {
                version = Some(value.to_string());
            } else if let Some(value) = config_value.as_float() {
                version = Some(value.to_string());
            } else if let Some(value) = config_value.as_integer() {
                version = Some(value.to_string());
            } else {
                if let Some(value) = config_value.get_as_str_forced("version") {
                    version = Some(value.to_string());
                } else if let Some(value) = config_value.get_as_str_forced("version_file") {
                    version_file = Some(value.to_string());
                }

                if let Some(value) = config_value.get_as_str("dir") {
                    dirs.insert(
                        PathBuf::from(value)
                            .normalize()
                            .to_string_lossy()
                            .to_string(),
                    );
                } else if let Some(array) = config_value.get_as_array("dir") {
                    for value in array {
                        if let Some(value) = value.as_str_forced() {
                            dirs.insert(
                                PathBuf::from(value)
                                    .normalize()
                                    .to_string_lossy()
                                    .to_string(),
                            );
                        }
                    }
                }

                if let Some(value) = config_value.get_as_bool_forced("upgrade") {
                    upgrade = value;
                }
            }
        }

        Self {
            asdf_base: OnceCell::new(),
            version,
            version_file,
            upgrade,
            dirs,
        }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        self.asdf_base()?.up(options, environment, progress_handler)
    }

    pub fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        self.asdf_base()?.commit(options, env_version_id)
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        self.asdf_base()?.down(progress_handler)
    }

    pub fn was_upped(&self) -> bool {
        self.asdf_base()
            .map_or(false, |asdf_base| asdf_base.was_upped())
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        self.asdf_base()
            .map_or(vec![], |asdf_base| asdf_base.data_paths())
    }

    pub fn asdf_base(&self) -> Result<&UpConfigAsdfBase, UpError> {
        self.asdf_base.get_or_try_init(|| {
            let version = if let Some(version) = &self.version {
                version.clone()
            } else if let Some(version) = self.extract_version_from_gomod()? {
                version
            } else {
                "latest".to_string()
            };

            let mut asdf_base =
                UpConfigAsdfBase::new("golang", version.as_ref(), self.dirs.clone(), self.upgrade);
            asdf_base.add_detect_version_func(detect_version_from_gomod);
            asdf_base.add_post_install_func(setup_individual_gopath);

            Ok(asdf_base)
        })
    }

    fn extract_version_from_gomod(&self) -> Result<Option<String>, UpError> {
        if self.version_file.is_none() {
            return Ok(None);
        }

        extract_version_from_gomod_file(self.version_file.as_ref().unwrap().clone())
    }
}

fn detect_version_from_gomod(_tool_name: String, path: PathBuf) -> Option<String> {
    let version_file_path = path.join("go.mod");
    if !version_file_path.exists() || version_file_path.is_dir() {
        return None;
    }

    extract_version_from_gomod_file(version_file_path).unwrap_or(None)
}

fn extract_version_from_gomod_file(
    version_file: impl AsRef<Path>,
) -> Result<Option<String>, UpError> {
    // Get the version file abs path
    let version_file = abs_path(version_file);

    // Open the file and read it line by line
    let file = File::open(version_file.clone());
    if let Err(err) = &file {
        return Err(UpError::Exec(format!(
            "failed to open version file ({}): {}",
            version_file.display(),
            err,
        )));
    }

    let file = file.unwrap();
    let reader = BufReader::new(file);

    // Prepare the regex to extract the version
    let goversion = regex::Regex::new(r"(?m)^go (?<version>\d+\.\d+(?:\.\d+)?)$").unwrap();

    for line in reader.lines() {
        if line.is_err() {
            continue;
        }
        let line = line.unwrap();

        // Check if the line contains the version, we use simple string matching first
        // as it is way faster than regex
        if line.starts_with("go ") {
            // Try and match the regex to extract the version
            if let Some(captures) = goversion.captures(&line) {
                // Get the version
                let version = captures.name("version").unwrap().as_str().to_string();

                // Return the version
                return Ok(Some(version));
            }
        }
    }

    // Return None if we didn't find the version
    Err(UpError::Exec(format!(
        "no version found in version file ({})",
        version_file.display(),
    )))
}

fn setup_individual_gopath(
    _options: &UpOptions,
    environment: &mut UpEnvironment,
    _progress_handler: &dyn ProgressHandler,
    _config_value: Option<ConfigValue>,
    tool: String,
    tool_real_name: String,
    _requested_version: String,
    versions: Vec<AsdfToolUpVersion>,
) -> Result<(), UpError> {
    if tool_real_name != "golang" {
        panic!("setup_individual_gopath called with wrong tool: {}", tool);
    }

    // Get the data path for the work directory
    let workdir = workdir(".");

    let data_path = match workdir.data_path() {
        Some(data_path) => data_path,
        None => {
            return Err(UpError::Exec(format!(
                "failed to get data path for {}",
                current_dir().display()
            )));
        }
    };

    // Handle each version individually
    for version in &versions {
        for dir in &version.dirs {
            let gopath_dir = data_path_dir_hash(dir);

            let gopath = data_path
                .join(&tool)
                .join(&version.version)
                .join(&gopath_dir);

            environment.add_version_data_path(
                &tool,
                &version.version,
                dir,
                &gopath.to_string_lossy(),
            );
        }
    }

    Ok(())
}
