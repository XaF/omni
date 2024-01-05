use std::path::PathBuf;

use normalize_path::NormalizePath;
use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::utils::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::up::asdf_tool_path;
use crate::internal::config::up::run_progress;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::AsdfToolUpVersion;
use crate::internal::config::up::ProgressHandler;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::env::current_dir;
use crate::internal::env::workdir;
use crate::internal::ConfigValue;

const MIN_VERSION_VENV: Version = Version::new(3, 3, 0);
// const MIN_VERSION_VIRTUALENV: Version = Version::new(2, 6, 0);

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfigPython {
    pub asdf_base: UpConfigAsdfBase,
}

impl Serialize for UpConfigPython {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        self.asdf_base.serialize(serializer)
        // TODO: add pip option serialization
    }
}

impl UpConfigPython {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut asdf_base = UpConfigAsdfBase::from_config_value("python", config_value);
        asdf_base.add_post_install_func(setup_python_venv);
        asdf_base.add_post_install_func(setup_python_pip);

        Self { asdf_base }
    }

    pub fn up(&self, options: &UpOptions, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        self.asdf_base.up(options, progress)
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        self.asdf_base.down(progress)
    }
}

fn setup_python_venv(
    progress_handler: &dyn ProgressHandler,
    _config_value: Option<ConfigValue>,
    tool: String,
    _requested_version: String,
    versions: Vec<AsdfToolUpVersion>,
) -> Result<(), UpError> {
    if tool != "python" {
        panic!("setup_python_venv called with wrong tool: {}", tool);
    }

    // Handle each version individually
    for version in &versions {
        setup_python_venv_per_version(progress_handler, version.clone())?;
    }

    Ok(())
}

fn setup_python_venv_per_version(
    progress_handler: &dyn ProgressHandler,
    version: AsdfToolUpVersion,
) -> Result<(), UpError> {
    // Check if we care about that version
    match Version::parse(&version.version) {
        Ok(version) => {
            if version < MIN_VERSION_VENV {
                progress_handler.progress(format!(
                    "skipping venv setup for python {} < {}",
                    version, MIN_VERSION_VENV
                ));
                return Ok(());
            }
        }
        Err(_) => {
            progress_handler.progress(format!(
                "skipping venv setup for python {} (unsupported version)",
                version.version
            ));
            return Ok(());
        }
    }

    for dir in version.dirs {
        setup_python_venv_per_dir(progress_handler, version.version.clone(), dir)?;
    }

    Ok(())
}

fn setup_python_venv_per_dir(
    progress_handler: &dyn ProgressHandler,
    version: String,
    dir: String,
) -> Result<(), UpError> {
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

    // Get the hash of the relative path
    let venv_dir = data_path_dir_hash(&dir);

    let venv_path = data_path
        .join("python")
        .join(version.clone())
        .join(venv_dir.clone());

    // Check if we need to install, or if the virtual env is already there
    let already_setup = if venv_path.exists() {
        if venv_path.join("pyvenv.cfg").exists() {
            progress_handler.progress(format!("venv already exists for python {}", version));
            true
        } else {
            // Remove the directory since it exists but is not a venv,
            // so we clean it up and replace it by a clean venv
            std::fs::remove_dir_all(&venv_path).map_err(|_| {
                UpError::Exec(format!(
                    "failed to remove existing venv directory {}",
                    venv_path.display()
                ))
            })?;
            false
        }
    } else {
        false
    };

    // Only create the new venv if it doesn't exist
    if !already_setup {
        let python_version_path = asdf_tool_path("python", &version);
        let python_bin = PathBuf::from(python_version_path)
            .join("bin")
            .join("python");

        std::fs::create_dir_all(&venv_path).map_err(|_| {
            UpError::Exec(format!(
                "failed to create venv directory {}",
                venv_path.display()
            ))
        })?;

        let mut venv_create = TokioCommand::new(python_bin);
        venv_create.arg("-m");
        venv_create.arg("venv");
        venv_create.arg(venv_path.to_string_lossy().to_string());
        venv_create.stdout(std::process::Stdio::piped());
        venv_create.stderr(std::process::Stdio::piped());

        run_progress(
            &mut venv_create,
            Some(progress_handler),
            RunConfig::default(),
        )?;

        progress_handler.progress(format!("venv created for python {} in {}", version, dir,));
    }

    // Update the cache
    if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| {
        up_env.add_version_data_path(
            &workdir_id,
            "python",
            &version,
            &dir,
            &venv_path.to_string_lossy(),
        )
    }) {
        progress_handler.progress(format!("failed to update tool cache: {}", err));
        return Err(UpError::Cache(format!(
            "failed to update tool cache: {}",
            err
        )));
    }

    Ok(())
}

fn setup_python_pip(
    progress_handler: &dyn ProgressHandler,
    config_value: Option<ConfigValue>,
    _tool: String,
    requested_version: String,
    _versions: Vec<AsdfToolUpVersion>,
) -> Result<(), UpError> {
    let config_value = if let Some(config_value) = config_value {
        config_value
    } else {
        return Ok(());
    };

    let mut pip_auto = false;
    let mut pip_files = Vec::new();
    if let Some(config_value) = config_value.get_as_array("pip") {
        for file_path in config_value {
            if let Some(file_path) = file_path.as_str_forced() {
                pip_files.push(file_path.to_string());
            }
        }
    } else if let Some(file_path) = config_value.get_as_str_forced("pip") {
        if file_path == "auto" {
            pip_auto = true;
        } else {
            pip_files.push(file_path.to_string());
        }
    }

    if pip_files.is_empty() && !pip_auto {
        if requested_version == "auto" {
            pip_auto = true;
        } else {
            return Ok(());
        }
    }

    let tool_dirs = _versions
        .iter()
        .flat_map(|version| version.dirs.clone())
        .collect::<Vec<String>>();

    for dir in &tool_dirs {
        let path = PathBuf::from(dir).normalize();

        // Check if path is in current dir
        let full_path = abs_path(dir);
        if !full_path.starts_with(current_dir()) {
            return Err(UpError::Exec(format!(
                "directory {} is not in work directory",
                path.display(),
            )));
        }

        // Load the environment for that directory
        update_dynamic_env_for_command(full_path.to_string_lossy());

        if pip_auto {
            // If auto, use the requirements.txt file in the directory
            // if it exists
            let req_txt = path.join("requirements.txt");
            if req_txt.exists() {
                setup_python_pip_file(progress_handler, req_txt)?;
            }
        } else {
            // Otherwise, use the specified files
            for pip_file in &pip_files {
                setup_python_pip_file(progress_handler, PathBuf::from(pip_file))?
            }
        }
    }

    Ok(())
}

fn setup_python_pip_file(
    progress_handler: &dyn ProgressHandler,
    pip_file: PathBuf,
) -> Result<(), UpError> {
    if !pip_file.exists() {
        return Err(UpError::Exec(format!(
            "file {} does not exist",
            pip_file.display()
        )));
    }

    progress_handler.progress(format!(
        "installing dependencies from {}",
        pip_file.display()
    ));

    let mut pip_install = TokioCommand::new("pip");
    pip_install.arg("install");
    pip_install.arg("-r");
    pip_install.arg(pip_file.to_string_lossy().to_string());
    pip_install.stdout(std::process::Stdio::piped());
    pip_install.stderr(std::process::Stdio::piped());

    run_progress(
        &mut pip_install,
        Some(progress_handler),
        RunConfig::default(),
    )?;

    progress_handler.progress(format!(
        "dependencies from {} installed",
        pip_file.display()
    ));

    Ok(())
}
