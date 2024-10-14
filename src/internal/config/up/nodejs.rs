use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use node_semver::Range as semverRange;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::utils as cache_utils;
use crate::internal::cache::utils::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::AsdfToolUpVersion;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::env::current_dir;
use crate::internal::workdir;
use crate::internal::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigNodejsParams {
    #[serde(
        default = "cache_utils::set_true",
        skip_serializing_if = "cache_utils::is_true"
    )]
    install_engines: bool,
    #[serde(
        default = "cache_utils::set_true",
        skip_serializing_if = "cache_utils::is_true"
    )]
    install_packages: bool,
}

impl Default for UpConfigNodejsParams {
    fn default() -> Self {
        Self {
            install_engines: Self::DEFAULT_INSTALL_ENGINES,
            install_packages: Self::DEFAULT_INSTALL_PACKAGES,
        }
    }
}

impl UpConfigNodejsParams {
    const DEFAULT_INSTALL_ENGINES: bool = true;
    const DEFAULT_INSTALL_PACKAGES: bool = true;

    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut params = Self::default();

        if let Some(config_value) = &config_value {
            if let Some(value) = config_value.get_as_bool_forced("install_engines") {
                params.install_engines = value;
            }

            if let Some(value) = config_value.get_as_bool_forced("install_packages") {
                params.install_packages = value;
            }
        }

        params
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfigNodejs {
    #[serde(skip)]
    pub asdf_base: UpConfigAsdfBase,
    #[serde(skip)]
    pub params: UpConfigNodejsParams,
}

impl Serialize for UpConfigNodejs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        // Serialize object into serde_json::Value
        let mut nodejs_base = serde_json::to_value(&self.asdf_base).unwrap();

        // Serialize the params object
        let nodejs_params = serde_json::to_value(&self.params).unwrap();

        // Merge the params object into the base object
        nodejs_base
            .as_object_mut()
            .unwrap()
            .extend(nodejs_params.as_object().unwrap().clone());

        // Serialize the object
        nodejs_base.serialize(serializer)
    }
}

impl UpConfigNodejs {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut asdf_base = UpConfigAsdfBase::from_config_value("nodejs", config_value);
        asdf_base.add_detect_version_func(detect_version_from_package_json);
        asdf_base.add_detect_version_func(detect_version_from_nvmrc);
        asdf_base.add_post_install_func(setup_individual_npm_prefix);

        let params = UpConfigNodejsParams::from_config_value(config_value);

        Self { asdf_base, params }
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

    let package_json_path = path.join("package.json");
    if !package_json_path.exists() || package_json_path.is_dir() {
        return None;
    }

    let package_json_str = match std::fs::read_to_string(&package_json_path) {
        Ok(package_json_str) => package_json_str,
        Err(_) => return None,
    };

    let pkgfile: PackageJson = match serde_json::from_str(&package_json_str) {
        Ok(pkgfile) => pkgfile,
        Err(_) => return None,
    };

    if let Some(node_version) = pkgfile.engines.get("node") {
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

fn setup_individual_npm_prefix(
    progress_handler: &dyn ProgressHandler,
    config_value: Option<ConfigValue>,
    tool: String,
    tool_real_name: String,
    _requested_version: String,
    versions: Vec<AsdfToolUpVersion>,
) -> Result<(), UpError> {
    if tool_real_name != "nodejs" {
        panic!(
            "setup_individual_npm_prefix called with wrong tool: {}",
            tool
        );
    }

    // Get the data path for the work directory
    let workdir = workdir(".");

    let workdir_id = match workdir.id() {
        Some(workdir_id) => workdir_id,
        None => {
            return Err(UpError::Exec(format!(
                "failed to get workdir id for {}",
                current_dir().display()
            )));
        }
    };

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
    let per_version_per_dir_data_path = |version: &AsdfToolUpVersion, dir: &String| {
        let npm_prefix_dir = data_path_dir_hash(dir);

        let npm_prefix = data_path
            .join(&tool)
            .join(&version.version)
            .join(npm_prefix_dir);

        npm_prefix.to_string_lossy().to_string()
    };

    for version in &versions {
        if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| {
            let mut any_changed = false;
            for dir in &version.dirs {
                let npm_prefix = per_version_per_dir_data_path(version, dir);

                any_changed = up_env.add_version_data_path(
                    &workdir_id,
                    &tool,
                    &version.version,
                    dir,
                    &npm_prefix,
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

    let workdir_root = match workdir.root() {
        Some(workdir_root) => workdir_root,
        None => {
            return Err(UpError::Exec(format!(
                "failed to get workdir root for {}",
                current_dir().display()
            )));
        }
    };

    let params = UpConfigNodejsParams::from_config_value(config_value.as_ref());
    if !params.install_engines && !params.install_packages {
        // Exit early if we don't need to install engines or packages
        return Ok(());
    }

    // Handle auto-installing the right engines in the right versions, and the packages
    for version in &versions {
        for dir in &version.dirs {
            let actual_dir = PathBuf::from(workdir_root).join(dir);

            // Check if the package.json exists
            let package_json_path = actual_dir.join("package.json");
            if !package_json_path.exists() || package_json_path.is_dir() {
                continue;
            }

            let package_json_str = match std::fs::read_to_string(&package_json_path) {
                Ok(package_json_str) => package_json_str,
                Err(err) => {
                    progress_handler.progress(format!("failed to read package.json: {}", err));
                    return Err(UpError::Exec(format!(
                        "failed to read package.json: {}",
                        err
                    )));
                }
            };

            let pkgfile: PackageJson = match serde_json::from_str(&package_json_str) {
                Ok(pkgfile) => pkgfile,
                Err(err) => {
                    progress_handler.progress(format!("failed to parse package.json: {}", err));
                    return Err(UpError::Exec(format!(
                        "failed to parse package.json: {}",
                        err
                    )));
                }
            };

            // Load the environment for that directory
            update_dynamic_env_for_command(actual_dir.to_str().unwrap());

            if params.install_engines {
                // Install the engines
                for (engine, version_range) in pkgfile.engines.iter() {
                    if engine == "node" || engine == "iojs" {
                        continue;
                    }

                    progress_handler
                        .progress(format!("installing {} version {}", engine, version_range));

                    // Install the engine using directly the provided version range
                    let mut npm_install = TokioCommand::new("npm");
                    npm_install.arg("install");
                    npm_install.arg("-g");
                    npm_install.arg(format!("{}@{}", engine, version_range));
                    npm_install.stdout(std::process::Stdio::piped());
                    npm_install.stderr(std::process::Stdio::piped());

                    let result = run_progress(
                        &mut npm_install,
                        Some(progress_handler),
                        RunConfig::default(),
                    );

                    if let Err(e) = result {
                        let msg = format!(
                            "failed to install engine {} version {}: {}",
                            engine, version_range, e
                        );
                        progress_handler.error_with_message(msg.clone());
                        return Err(UpError::Exec(msg));
                    }
                }
            }

            if params.install_packages {
                // Install the packages
                let engines_slice: Vec<String> = pkgfile.engines.keys().cloned().collect();
                let install_engines = PackageInstallEngine::all_sorted(&actual_dir, &engines_slice);
                let install_engine = install_engines.first().unwrap();

                if which::which(install_engine.name()).is_err() {
                    progress_handler.progress(format!(
                        "skipping package installation: {} not found",
                        install_engine.name(),
                    ));
                    continue;
                }

                progress_handler.progress(format!(
                    "installing packages with {}",
                    install_engine.name(),
                ));

                let mut pkg_install = install_engine.install_command();
                pkg_install.current_dir(&actual_dir);

                let result = run_progress(
                    &mut pkg_install,
                    Some(progress_handler),
                    RunConfig::default(),
                );

                if let Err(e) = result {
                    let msg = format!("failed to install packages: {}", e);
                    progress_handler.error_with_message(msg.clone());
                    return Err(UpError::Exec(msg));
                }
            }
        }
    }

    // Load the environment for the current directory
    update_dynamic_env_for_command(".");

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct PackageJson {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    engines: HashMap<String, String>,
}

#[derive(Debug)]
enum PackageInstallEngine {
    Pnpm,
    Yarn,
    Npm,
}

impl PackageInstallEngine {
    fn all() -> Vec<Self> {
        vec![Self::Pnpm, Self::Yarn, Self::Npm]
    }

    fn all_sorted(path: &Path, engines: &[String]) -> Vec<Self> {
        let mut sorted = Self::all();
        sorted.sort_by_key(|a| a.weight(path, engines));
        sorted.reverse();
        sorted
    }

    fn name(&self) -> String {
        match self {
            Self::Npm => "npm".to_string(),
            Self::Yarn => "yarn".to_string(),
            Self::Pnpm => "pnpm".to_string(),
        }
    }

    fn lock_file(&self) -> String {
        match self {
            Self::Npm => "package-lock.json".to_string(),
            Self::Yarn => "yarn.lock".to_string(),
            Self::Pnpm => "pnpm-lock.yaml".to_string(),
        }
    }

    fn weight(&self, path: &Path, engines: &[String]) -> u8 {
        let mut weight = 0;

        if engines.contains(&self.name()) {
            weight += 1;
        }

        let lock_path = path.join(self.lock_file());
        if lock_path.exists() && !lock_path.is_dir() {
            weight += 2;
        }

        weight
    }

    fn install_command(&self) -> TokioCommand {
        let mut cmd = TokioCommand::new(self.name());
        cmd.arg("install");
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        cmd
    }
}
