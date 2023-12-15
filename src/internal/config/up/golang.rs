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

use crate::internal::cache::utils::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::AsdfToolUpVersion;
use crate::internal::config::up::ProgressHandler;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::env::current_dir;
use crate::internal::workdir;
use crate::internal::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigGolang {
    pub version: Option<String>,
    pub version_file: Option<String>,
    pub dirs: BTreeSet<String>,
    #[serde(skip)]
    pub asdf_base: OnceCell<UpConfigAsdfBase>,
}

impl UpConfigGolang {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut version = None;
        let mut version_file = None;
        let mut dirs = BTreeSet::new();

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
            }
        }

        Self {
            asdf_base: OnceCell::new(),
            version,
            version_file,
            dirs,
        }
    }

    pub fn up(&self, options: &UpOptions, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        self.asdf_base()?.up(options, progress)
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        self.asdf_base()?.down(progress)
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
                UpConfigAsdfBase::new("golang", version.as_ref(), self.dirs.clone());
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
    progress_handler: &dyn ProgressHandler,
    tool: String,
    versions: Vec<AsdfToolUpVersion>,
) -> Result<(), UpError> {
    if tool != "golang" {
        panic!("setup_individual_gopath called with wrong tool: {}", tool);
    }

    // Get the data path for the work directory
    let workdir = workdir(".");

    let workdir_id = if let Some(workdir_id) = workdir.id() {
        workdir_id
    } else {
        return Err(UpError::Exec(format!(
            "failed to get workdir id for {}",
            current_dir().display()
        )));
    };

    let data_path = if let Some(data_path) = workdir.data_path() {
        data_path
    } else {
        return Err(UpError::Exec(format!(
            "failed to get data path for {}",
            current_dir().display()
        )));
    };

    // Handle each version individually
    for version in &versions {
        if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| {
            let mut any_changed = false;
            for dir in &version.dirs {
                let gopath_dir = data_path_dir_hash(dir);

                let gopath = data_path
                    .join(&tool)
                    .join(&version.version)
                    .join(&gopath_dir);

                any_changed = up_env.add_version_data_path(
                    &workdir_id,
                    &tool,
                    &version.version,
                    dir,
                    &gopath.to_string_lossy(),
                ) || any_changed;
            }
            any_changed
        }) {
            progress_handler.progress(format!("failed to update tool cache: {}", err));
            return Err(UpError::Cache(format!(
                "failed to update tool cache: {}",
                err
            )));
        }
    }

    Ok(())
}
