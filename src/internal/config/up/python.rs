use std::path::PathBuf;

use normalize_path::NormalizePath;
use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::up::mise::FullyQualifiedToolName;
use crate::internal::config::up::mise::PostInstallFuncArgs;
use crate::internal::config::up::mise_tool_path;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::MiseToolUpVersion;
use crate::internal::config::up::UpConfigMise;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::dynenv::update_dynamic_env_for_command_from_env;
use crate::internal::env::current_dir;
use crate::internal::env::workdir;
use crate::internal::ConfigValue;

const MIN_VERSION_VENV: Version = Version::new(3, 3, 0);
// const MIN_VERSION_VIRTUALENV: Version = Version::new(2, 6, 0);

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpConfigPythonParams {
    #[serde(default, rename = "pip", skip_serializing_if = "Vec::is_empty")]
    pip_files: Vec<String>,
    #[serde(default, skip)]
    pip_auto: bool,
}

impl UpConfigPythonParams {
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        let mut pip_files = Vec::new();
        let mut pip_auto = false;

        if let Some(config_value) = config_value {
            if let Some(config_value) = config_value.get_as_array("pip") {
                for file_path in config_value {
                    if let Some(file_path) = file_path.as_str_forced() {
                        pip_files.push(file_path.to_string());
                    } else {
                        on_error(ConfigErrorKind::InvalidValueType {
                            key: error_key.to_string(),
                            actual: file_path.as_serde_yaml(),
                            expected: "string".to_string(),
                        });
                    }
                }
            } else if let Some(file_path) = config_value.get_as_str_forced("pip") {
                if file_path == "auto" {
                    pip_auto = true;
                } else {
                    pip_files.push(file_path.to_string());
                }
            } else {
                on_error(ConfigErrorKind::InvalidValueType {
                    key: error_key.to_string(),
                    actual: config_value.as_serde_yaml(),
                    expected: "string or array of strings".to_string(),
                });
            }
        }

        Self {
            pip_files,
            pip_auto,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfigPython {
    #[serde(skip)]
    pub backend: UpConfigMise,
    #[serde(skip)]
    pub params: UpConfigPythonParams,
}

impl Serialize for UpConfigPython {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        // Serialize object into serde_json::Value
        let mut backend = serde_json::to_value(&self.backend).unwrap();

        // Serialize the params object
        let mut params = serde_json::to_value(&self.params).unwrap();

        // If params.pip_auto is true, set the pip field to "auto"
        if self.params.pip_auto {
            params["pip"] = serde_json::Value::String("auto".to_string());
        }

        // Merge the params object into the base object
        backend
            .as_object_mut()
            .unwrap()
            .extend(params.as_object().unwrap().clone());

        // Serialize the object
        backend.serialize(serializer)
    }
}

impl UpConfigPython {
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        let mut backend =
            UpConfigMise::from_config_value("python", config_value, error_key, on_error);
        backend.add_post_install_func(setup_python_venv);
        backend.add_post_install_func(setup_python_pip);

        let params = UpConfigPythonParams::from_config_value(config_value, error_key, on_error);

        Self { backend, params }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        self.backend.up(options, environment, progress_handler)
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        self.backend.down(progress_handler)
    }
}

fn setup_python_venv(
    options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &dyn ProgressHandler,
    args: &PostInstallFuncArgs,
) -> Result<(), UpError> {
    if args.fqtn.tool() != "python" {
        panic!(
            "setup_python_venv called with wrong tool: {}",
            args.fqtn.tool()
        );
    }

    // Handle each version individually
    for version in &args.versions {
        setup_python_venv_per_version(
            options,
            environment,
            progress_handler,
            args.fqtn,
            version.clone(),
        )?;
    }

    Ok(())
}

fn setup_python_venv_per_version(
    options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &dyn ProgressHandler,
    fqtn: &FullyQualifiedToolName,
    version: MiseToolUpVersion,
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
        setup_python_venv_per_dir(
            options,
            environment,
            progress_handler,
            fqtn,
            version.version.clone(),
            dir,
        )?;
    }

    Ok(())
}

fn setup_python_venv_per_dir(
    _options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &dyn ProgressHandler,
    fqtn: &FullyQualifiedToolName,
    version: String,
    dir: String,
) -> Result<(), UpError> {
    // Get the data path for the work directory
    let workdir = workdir(".");

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
        .join(fqtn.normalized_plugin_name()?)
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
        let python_version_path = mise_tool_path(&fqtn.normalized_plugin_name()?, &version);
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

        progress_handler.progress(format!(
            "venv created for python {} in {}",
            version,
            if dir.is_empty() { "." } else { &dir }
        ));
    }

    // Update the cache
    environment.add_version_data_path(
        fqtn.fully_qualified_plugin_name(),
        &version,
        &dir,
        &venv_path.to_string_lossy(),
    );

    Ok(())
}

fn setup_python_pip(
    _options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &dyn ProgressHandler,
    args: &PostInstallFuncArgs,
) -> Result<(), UpError> {
    let params =
        UpConfigPythonParams::from_config_value(args.config_value.as_ref(), "", &mut |_| ());
    let mut pip_auto = params.pip_auto;

    // TODO: should we default set pip_auto to true if no pip_files are specified?
    //       if yes, this should come with an option to disable it entirely too
    if params.pip_files.is_empty() && !pip_auto {
        if args.requested_version == "auto" {
            pip_auto = true;
        } else {
            return Ok(());
        }
    }

    let tool_dirs = args
        .versions
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
        update_dynamic_env_for_command_from_env(full_path.to_string_lossy(), environment);

        if pip_auto {
            // If auto, use the requirements.txt file in the directory
            // if it exists
            let req_txt = path.join("requirements.txt");
            if req_txt.exists() {
                setup_python_pip_file(progress_handler, req_txt)?;
            }
        } else {
            // Otherwise, use the specified files
            for pip_file in &params.pip_files {
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
