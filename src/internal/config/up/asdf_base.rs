use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;

use itertools::any;
use lazy_static::lazy_static;
use node_semver::Range as semverRange;
use node_semver::Version as semverVersion;
use normalize_path::NormalizePath;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;
use walkdir::WalkDir;

use crate::internal::cache::AsdfOperationCache;
use crate::internal::cache::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config::up::utils::force_remove_dir_all;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::env::data_home;
use crate::internal::env::shell_is_interactive;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;

lazy_static! {
    static ref ASDF_PATH: String = format!("{}/asdf", data_home());
    static ref ASDF_BIN: String = format!("{}/bin/asdf", *ASDF_PATH);
}

type DetectVersionFunc = fn(String, PathBuf) -> Option<String>;
type PostInstallFunc =
    fn(&dyn ProgressHandler, String, Vec<AsdfToolUpVersion>) -> Result<(), UpError>;

fn asdf_path() -> String {
    (*ASDF_PATH).clone()
}

fn asdf_bin() -> &'static str {
    ASDF_BIN.as_str()
}

pub fn asdf_tool_path(tool: &str, version: &str) -> String {
    format!("{}/installs/{}/{}", asdf_path(), tool, version)
}

fn is_asdf_installed() -> bool {
    let bin_path = std::path::Path::new(asdf_bin());
    bin_path.is_file() && bin_path.metadata().unwrap().permissions().mode() & 0o111 != 0
}

fn install_asdf(progress_handler: &dyn ProgressHandler) -> Result<(), UpError> {
    // Add asdf to PATH if not there yet, as some of the asdf plugins depend on it being
    // in the PATH. We will want it to be at the beginning of the PATH, so that it takes
    // precedence over any other asdf installation.
    let bin_path = PathBuf::from(format!("{}/bin", asdf_path()));
    let path_env = std::env::var("PATH").unwrap();
    let paths: Vec<PathBuf> = std::env::split_paths(&path_env).collect();
    let mut new_paths: Vec<PathBuf> = paths.into_iter().filter(|p| *p != bin_path).collect();
    new_paths.insert(0, bin_path);
    let new_path_env = std::env::join_paths(new_paths).expect("Failed to join paths");
    std::env::set_var("PATH", new_path_env);

    if !is_asdf_installed() {
        progress_handler.progress("installing asdf".to_string());

        let mut git_clone = TokioCommand::new("git");
        git_clone.arg("clone");
        git_clone.arg("https://github.com/asdf-vm/asdf.git");
        git_clone.arg(asdf_path());
        git_clone.arg("--branch");
        // We hardcode the version we initially get, but since we update asdf right after,
        // and then update it on every run, this is not _this_ version that will keep being
        // used, we just need "one version" that works well with updating after.
        git_clone.arg("v0.12.0");
        git_clone.stdout(std::process::Stdio::piped());
        git_clone.stderr(std::process::Stdio::piped());

        run_progress(&mut git_clone, Some(progress_handler), RunConfig::default())?;
    }

    update_asdf(progress_handler)
}

fn update_asdf(progress_handler: &dyn ProgressHandler) -> Result<(), UpError> {
    if !AsdfOperationCache::get().should_update_asdf() {
        return Ok(());
    }

    progress_handler.progress("updating asdf".to_string());

    let mut asdf_update = TokioCommand::new(asdf_bin());
    asdf_update.arg("update");
    asdf_update.env("ASDF_DIR", asdf_path());
    asdf_update.env("ASDF_DATA_DIR", asdf_path());
    asdf_update.stdout(std::process::Stdio::piped());
    asdf_update.stderr(std::process::Stdio::piped());

    run_progress(
        &mut asdf_update,
        Some(progress_handler),
        RunConfig::default(),
    )?;

    if let Err(err) = AsdfOperationCache::exclusive(|asdf_cache| {
        asdf_cache.updated_asdf();
        true
    }) {
        return Err(UpError::Cache(err.to_string()));
    }

    Ok(())
}

fn is_asdf_tool_version_installed(tool: &str, version: &str) -> bool {
    let mut asdf_list = std::process::Command::new(asdf_bin());
    asdf_list.arg("list");
    asdf_list.arg(tool);
    asdf_list.arg(version);
    asdf_list.env("ASDF_DIR", asdf_path());
    asdf_list.env("ASDF_DATA_DIR", asdf_path());
    asdf_list.stdout(std::process::Stdio::null());
    asdf_list.stderr(std::process::Stdio::null());

    if let Ok(output) = asdf_list.output() {
        if output.status.success() {
            return true;
        }
    }

    false
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpConfigAsdfBase {
    /// The name of the tool to install.
    pub tool: String,

    /// The URL to use to install the tool.
    pub tool_url: Option<String>,

    /// The version of the tool to install, as specified in the config file.
    pub version: String,

    /// A list of directories to install the tool for.
    pub dirs: BTreeSet<String>,

    /// A list of functions to run to detect the version of the tool.
    /// The functions will be called with the following parameters:
    /// - tool: the name of the tool
    /// - path: the path currently being searched
    /// The functions should return the version of the tool if found, or None
    /// if not found.
    /// The functions will be called in order, and the first one to return a
    /// version will be used.
    /// If no function returns a version, the version will be considered not
    /// found.
    #[serde(skip)]
    detect_version_funcs: Vec<DetectVersionFunc>,

    /// A list of functions to run after installing a version of the tool.
    /// This is useful for tools that require additional steps after installing
    /// a version, such as installing plugins or running post-install scripts.
    /// The functions will be called with the following parameters:
    /// - progress_handler: a progress handler to use to report progress
    /// - tool: the name of the tool
    /// - versions: AsdfToolUpVersion objects describing the versions that were
    ///             up-ed, with the following fields:
    ///     - version: the version of the tool that was installed
    ///     - installed: whether the tool was installed or already installed
    ///     - paths: the relative paths where the tool version was installed
    #[serde(skip)]
    post_install_funcs: Vec<PostInstallFunc>,

    /// The actual version of the tool that has to be installed.
    #[serde(skip)]
    actual_version: OnceCell<String>,

    /// The actual versions of the tool that have been installed.
    /// This is only used when the version is "auto".
    #[serde(skip)]
    actual_versions: OnceCell<BTreeSet<String>>,
}

impl UpConfigAsdfBase {
    pub fn new(tool: &str, version: &str, dirs: BTreeSet<String>) -> Self {
        UpConfigAsdfBase {
            tool: tool.to_string(),
            tool_url: None,
            version: version.to_string(),
            dirs: dirs.clone(),
            detect_version_funcs: vec![],
            post_install_funcs: vec![],
            actual_version: OnceCell::new(),
            actual_versions: OnceCell::new(),
        }
    }

    pub fn add_detect_version_func(&mut self, func: DetectVersionFunc) {
        self.detect_version_funcs.push(func);
    }

    pub fn add_post_install_func(&mut self, func: PostInstallFunc) {
        self.post_install_funcs.push(func);
    }

    fn new_from_auto(&self, version: &str, dirs: BTreeSet<String>) -> Self {
        UpConfigAsdfBase {
            tool: self.tool.clone(),
            tool_url: self.tool_url.clone(),
            version: version.to_string(),
            dirs: dirs.clone(),

            // We can ignore all those fields, as they won't be used,
            // since the version passed to that call is a specific version
            // that we got from running the detection functions from a
            // main instance called with "auto" as the version.
            detect_version_funcs: vec![],
            post_install_funcs: vec![],
            actual_version: OnceCell::new(),
            actual_versions: OnceCell::new(),
        }
    }

    pub fn from_config_value(tool: &str, config_value: Option<&ConfigValue>) -> Self {
        Self::from_config_value_with_params(tool, None, config_value)
    }

    pub fn from_config_value_with_url(
        tool: &str,
        tool_url: &str,
        config_value: Option<&ConfigValue>,
    ) -> Self {
        Self::from_config_value_with_params(tool, Some(tool_url.to_string()), config_value)
    }

    fn from_config_value_with_params(
        tool: &str,
        tool_url: Option<String>,
        config_value: Option<&ConfigValue>,
    ) -> Self {
        let mut version = "latest".to_string();
        let mut dirs = BTreeSet::new();

        if let Some(config_value) = config_value {
            if let Some(value) = config_value.as_str() {
                version = value.to_string();
            } else if let Some(value) = config_value.as_float() {
                version = value.to_string();
            } else if let Some(value) = config_value.as_integer() {
                version = value.to_string();
            } else {
                if let Some(value) = config_value.get_as_str_forced("version") {
                    version = value.to_string();
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

        UpConfigAsdfBase {
            tool: tool.to_string(),
            tool_url,
            version,
            dirs,
            detect_version_funcs: vec![],
            post_install_funcs: vec![],
            actual_version: OnceCell::new(),
            actual_versions: OnceCell::new(),
        }
    }

    fn update_cache(&self, progress_handler: &dyn ProgressHandler) {
        let workdir = workdir(".");

        let repo_id = if let Some(repo_id) = workdir.id() {
            repo_id
        } else {
            return;
        };

        let version = if let Ok(version) = self.version(None) {
            version.to_string()
        } else {
            return;
        };

        progress_handler.progress("updating cache".to_string());

        if let Err(err) = AsdfOperationCache::exclusive(|asdf_cache| {
            asdf_cache.add_installed(&repo_id, &self.tool, &version)
        }) {
            progress_handler.progress(format!("failed to update tool cache: {}", err));
            return;
        }

        if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| {
            let mut dirs = self.dirs.clone();
            if dirs.is_empty() {
                dirs.insert("".to_string());
            }

            up_env.add_version(&repo_id, &self.tool, &version, dirs.clone())
        }) {
            progress_handler.progress(format!("failed to update tool cache: {}", err));
            return;
        }

        progress_handler.progress("updated cache".to_string());
    }

    pub fn up(
        &self,
        _options: &UpOptions,
        progress: Option<(usize, usize)>,
    ) -> Result<(), UpError> {
        let desc = format!("{} ({}):", self.tool, self.version).light_blue();
        let progress_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
            Box::new(SpinnerProgressHandler::new(desc, progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, progress))
        };
        let progress_handler: &dyn ProgressHandler = progress_handler.as_ref();

        if let Err(err) = install_asdf(progress_handler) {
            progress_handler.error_with_message(format!("error: {}", err));
            return Err(err);
        }

        if let Err(err) = self.install_plugin(progress_handler) {
            progress_handler.error_with_message(format!("error: {}", err));
            return Err(err);
        }

        if self.version == "auto" {
            progress_handler.progress("detecting required versions and paths".to_string());

            let mut detected_versions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

            // Get the current directory
            let current_dir = std::env::current_dir().expect("failed to get current directory");

            let mut search_dirs = self.dirs.clone();
            if search_dirs.is_empty() {
                search_dirs.insert("".to_string());
            }

            let mut detect_version_funcs = self.detect_version_funcs.clone();
            detect_version_funcs.push(detect_version_from_asdf_version_file);
            detect_version_funcs.push(detect_version_from_tool_version_file);

            for search_dir in search_dirs.iter() {
                // For safety, we remove any leading slashes from the search directory,
                // as we only want to search in the workdir
                let mut search_dir = search_dir.clone();
                while search_dir.starts_with('/') {
                    search_dir.remove(0);
                }

                // Append the search directory to the current directory, since we are
                // at the root of the workdir
                let search_path = current_dir.join(search_dir);

                for entry in WalkDir::new(search_path)
                    .follow_links(true)
                    .into_iter()
                    .flatten()
                {
                    if !entry.path().is_dir() {
                        continue;
                    }

                    for detect_version_func in detect_version_funcs.iter() {
                        if let Some(detected_version) =
                            detect_version_func(self.tool.clone(), entry.path().to_path_buf())
                        {
                            let mut dir = entry
                                .path()
                                .strip_prefix(&current_dir)
                                .expect("failed to strip prefix")
                                .to_string_lossy()
                                .to_string();
                            while dir.starts_with('/') {
                                dir.remove(0);
                            }
                            while dir.ends_with('/') {
                                dir.pop();
                            }

                            if let Some(dirs) = detected_versions.get_mut(&detected_version) {
                                dirs.insert(dir);
                            } else {
                                let mut dirs = BTreeSet::new();
                                dirs.insert(dir);
                                detected_versions.insert(detected_version.to_string(), dirs);
                            }

                            break;
                        }
                    }
                }
            }

            if detected_versions.is_empty() {
                progress_handler.success_with_message("no version detected".to_string());
                return Ok(());
            }

            let mut installed_versions = Vec::new();
            let mut already_installed_versions = Vec::new();
            let mut all_versions = BTreeMap::new();

            for (version, dirs) in detected_versions.iter() {
                let asdf_base = self.new_from_auto(version, dirs.clone());
                let installed = asdf_base.install_version(progress_handler);
                if installed.is_err() {
                    let err = installed.err().unwrap();
                    progress_handler.error_with_message(format!("error: {}", err));
                    return Err(err);
                }

                let version = asdf_base.version(None).unwrap();
                all_versions.insert(version.clone(), dirs.clone());
                if installed.unwrap() {
                    installed_versions.push(version.clone());
                } else {
                    already_installed_versions.push(version.clone());
                }

                asdf_base.update_cache(progress_handler);
            }

            self.actual_versions
                .set(all_versions.keys().cloned().collect())
                .expect("failed to set installed versions");

            if !self.post_install_funcs.is_empty() {
                let post_install_versions = all_versions
                    .iter()
                    .map(|(version, dirs)| AsdfToolUpVersion {
                        version: version.clone(),
                        dirs: dirs.clone(),
                        installed: installed_versions.contains(version),
                    })
                    .collect::<Vec<AsdfToolUpVersion>>();

                for func in self.post_install_funcs.iter() {
                    if let Err(err) = func(
                        progress_handler,
                        self.tool.clone(),
                        post_install_versions.clone(),
                    ) {
                        progress_handler.error_with_message(format!("error: {}", err));
                        return Err(err);
                    }
                }
            }

            let mut msgs = Vec::new();

            if !installed_versions.is_empty() {
                msgs.push(
                    format!(
                        "{} {} installed",
                        self.tool,
                        installed_versions
                            .iter()
                            .map(|version| version.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                    .green(),
                );
            }

            if !already_installed_versions.is_empty() {
                msgs.push(
                    format!(
                        "{} {} already installed",
                        self.tool,
                        already_installed_versions
                            .iter()
                            .map(|version| version.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                    .light_black(),
                );
            }

            progress_handler.success_with_message(msgs.join(", "));

            Ok(())
        } else {
            match self.install_version(progress_handler) {
                Ok(installed) => {
                    self.update_cache(progress_handler);

                    let version = self.version(None).unwrap();

                    if !self.post_install_funcs.is_empty() {
                        let post_install_versions = vec![AsdfToolUpVersion {
                            version: version.clone(),
                            dirs: if self.dirs.is_empty() {
                                vec!["".to_string()].into_iter().collect()
                            } else {
                                self.dirs.clone()
                            },
                            installed,
                        }];

                        for func in self.post_install_funcs.iter() {
                            if let Err(err) = func(
                                progress_handler,
                                self.tool.clone(),
                                post_install_versions.clone(),
                            ) {
                                progress_handler.error_with_message(format!("error: {}", err));
                                return Err(err);
                            }
                        }
                    }

                    let msg = if installed {
                        format!("{} {} installed", self.tool, version).green()
                    } else {
                        format!("{} {} already installed", self.tool, version).light_black()
                    };
                    progress_handler.success_with_message(msg);

                    Ok(())
                }
                Err(err) => {
                    progress_handler.error_with_message(format!("error: {}", err));
                    Err(err)
                }
            }
        }
    }

    pub fn down(&self, _progress: Option<(usize, usize)>) -> Result<(), UpError> {
        Ok(())
    }

    fn version(&self, progress_handler: Option<&dyn ProgressHandler>) -> Result<&String, UpError> {
        let version = self.actual_version.get_or_init(|| {
            if self.update_plugin(progress_handler).is_err() {
                return "".to_string();
            }

            if let Some(handler) = progress_handler {
                handler.progress("checking available versions".to_string());
            }

            let available_versions = if let Some(versions) =
                AsdfOperationCache::get().get_asdf_plugin_versions(&self.tool)
            {
                versions
            } else {
                let mut asdf_list_all = std::process::Command::new(asdf_bin());
                asdf_list_all.arg("list");
                asdf_list_all.arg("all");
                asdf_list_all.arg(self.tool.clone());
                asdf_list_all.env("ASDF_DIR", asdf_path());
                asdf_list_all.env("ASDF_DATA_DIR", asdf_path());
                asdf_list_all.stdout(std::process::Stdio::piped());
                asdf_list_all.stderr(std::process::Stdio::piped());

                if let Ok(output) = asdf_list_all.output() {
                    if output.status.success() {
                        let stdout = String::from_utf8(output.stdout).unwrap();
                        let versions = stdout
                            .lines()
                            .map(|line| line.trim().to_string())
                            .filter(|line| !line.is_empty())
                            .collect::<Vec<String>>();

                        if let Err(err) = AsdfOperationCache::exclusive(|cache| {
                            cache.set_asdf_plugin_versions(&self.tool, versions.clone());
                            true
                        }) {
                            omni_error!(format!("failed to update cache: {}", err));
                            return "".to_string();
                        }

                        versions
                    } else {
                        omni_error!(format!(
                            "failed to list versions for {}; exited with status {}",
                            self.tool, output.status
                        ));
                        return "".to_string();
                    }
                } else {
                    omni_error!(format!("failed to list versions for {}", self.tool));
                    return "".to_string();
                }
            };

            let mut version = "".to_string();
            for available_version in available_versions {
                if version_match(&self.version, available_version.as_str()) {
                    version = available_version;
                }
            }

            version
        });

        if version.is_empty() {
            return Err(UpError::Exec(format!(
                "No {} version found matching {}",
                self.tool, self.version,
            )));
        }

        Ok(version)
    }

    fn is_plugin_installed(&self) -> bool {
        let mut asdf_plugin_list = std::process::Command::new(asdf_bin());
        asdf_plugin_list.arg("plugin");
        asdf_plugin_list.arg("list");
        asdf_plugin_list.env("ASDF_DIR", asdf_path());
        asdf_plugin_list.env("ASDF_DATA_DIR", asdf_path());
        asdf_plugin_list.stdout(std::process::Stdio::piped());
        asdf_plugin_list.stderr(std::process::Stdio::null());

        if let Ok(output) = asdf_plugin_list.output() {
            if output.status.success() {
                let stdout = String::from_utf8(output.stdout).unwrap();
                return stdout.lines().any(|line| line.trim() == self.tool);
            }
        }

        false
    }

    fn install_plugin(&self, progress_handler: &dyn ProgressHandler) -> Result<(), UpError> {
        if self.is_plugin_installed() {
            return Ok(());
        }

        progress_handler.progress(format!("installing {} plugin", self.tool));

        let mut asdf_plugin_add = TokioCommand::new(asdf_bin());
        asdf_plugin_add.arg("plugin");
        asdf_plugin_add.arg("add");
        asdf_plugin_add.arg(self.tool.clone());
        if let Some(tool_url) = &self.tool_url {
            asdf_plugin_add.arg(tool_url.clone());
        }
        asdf_plugin_add.env("ASDF_DIR", asdf_path());
        asdf_plugin_add.env("ASDF_DATA_DIR", asdf_path());
        asdf_plugin_add.stdout(std::process::Stdio::piped());
        asdf_plugin_add.stderr(std::process::Stdio::piped());

        run_progress(
            &mut asdf_plugin_add,
            Some(progress_handler),
            RunConfig::default(),
        )
    }

    fn update_plugin(&self, progress_handler: Option<&dyn ProgressHandler>) -> Result<(), UpError> {
        if !AsdfOperationCache::get().should_update_asdf_plugin(&self.tool) {
            return Ok(());
        }

        if let Some(ph) = progress_handler {
            ph.progress(format!("updating {} plugin", self.tool));
        }

        let mut asdf_plugin_update = TokioCommand::new(asdf_bin());
        asdf_plugin_update.arg("plugin");
        asdf_plugin_update.arg("update");
        asdf_plugin_update.arg(self.tool.clone());
        asdf_plugin_update.env("ASDF_DIR", asdf_path());
        asdf_plugin_update.env("ASDF_DATA_DIR", asdf_path());
        asdf_plugin_update.stdout(std::process::Stdio::piped());
        asdf_plugin_update.stderr(std::process::Stdio::piped());

        run_progress(
            &mut asdf_plugin_update,
            progress_handler,
            RunConfig::default(),
        )?;

        // Update the cache
        if let Err(err) = AsdfOperationCache::exclusive(|cache| {
            cache.updated_asdf_plugin(&self.tool);
            true
        }) {
            return Err(UpError::Cache(err.to_string()));
        }

        Ok(())
    }

    fn is_version_installed(&self) -> bool {
        let version = self.version(None);
        if version.is_err() {
            return false;
        }
        let version = version.unwrap();

        is_asdf_tool_version_installed(&self.tool, version)
    }

    fn install_version(&self, progress_handler: &dyn ProgressHandler) -> Result<bool, UpError> {
        let version = self.version(Some(progress_handler))?;

        if self.is_version_installed() {
            return Ok(false);
        }

        progress_handler.progress(format!("installing {} {}", self.tool, version));

        let mut asdf_install = tokio::process::Command::new(asdf_bin());
        asdf_install.arg("install");
        asdf_install.arg(self.tool.clone());
        asdf_install.arg(version);
        asdf_install.env("ASDF_DIR", asdf_path());
        asdf_install.env("ASDF_DATA_DIR", asdf_path());
        asdf_install.stdout(std::process::Stdio::piped());
        asdf_install.stderr(std::process::Stdio::piped());

        run_progress(
            &mut asdf_install,
            Some(progress_handler),
            RunConfig::default(),
        )?;

        Ok(true)
    }

    pub fn cleanup_unused(progress: Option<(usize, usize)>) -> Result<(), UpError> {
        let desc = "resources cleanup:".light_blue();
        let progress_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
            Box::new(SpinnerProgressHandler::new(desc, progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, progress))
        };
        let progress_handler: Option<&dyn ProgressHandler> = Some(progress_handler.as_ref());

        let workdir = workdir(".");
        let workdir_id = if let Some(workdir_id) = workdir.id() {
            workdir_id
        } else {
            return Err(UpError::Exec("failed to get workdir id".to_string()));
        };

        // Get the expected installed versions of the tool from
        // the up environment cache
        let mut env_tools = Vec::new();
        if let Some(env) = UpEnvironmentsCache::get().get_env(&workdir_id) {
            env_tools.extend(env.versions.iter().cloned());
        }

        if let Some(wd_data_path) = workdir.data_path() {
            if wd_data_path.exists() {
                if let Some(ph) = progress_handler {
                    ph.progress("removing unused data paths".to_string());
                }

                // If any data path in the versions
                if !any(&env_tools, |tool| tool.data_path.is_some()) {
                    force_remove_dir_all(wd_data_path).map_err(|err| {
                        UpError::Exec(format!(
                            "failed to remove workdir data path {}: {}",
                            wd_data_path.display(),
                            err
                        ))
                    })?;
                } else {
                    let expected_tools = env_tools
                        .iter()
                        .map(|tool| tool.tool.clone())
                        .collect::<BTreeSet<_>>();

                    let tool_dirs = std::fs::read_dir(wd_data_path.clone()).map_err(|err| {
                        UpError::Exec(format!(
                            "failed to read data path directory for {}: {}",
                            workdir_id, err,
                        ))
                    })?;
                    for tool_dir in tool_dirs {
                        let tool_dir = tool_dir.map_err(|err| {
                            UpError::Exec(format!(
                                "failed to read data path directory for {}: {}",
                                workdir_id, err,
                            ))
                        })?;

                        let tool_dir_name = tool_dir.file_name().to_string_lossy().to_string();

                        // Remove the tool directory if the tool is not expected
                        if !expected_tools.contains(&tool_dir_name) {
                            force_remove_dir_all(tool_dir.path()).map_err(|err| {
                                UpError::Exec(format!(
                                    "failed to remove workdir data path for tool {}: {}",
                                    tool_dir_name, err
                                ))
                            })?;
                            continue;
                        }

                        // Check the versions for that tool
                        let expected_versions = env_tools
                            .iter()
                            .filter(|tool| tool.tool == tool_dir_name)
                            .map(|tool| tool.version.clone())
                            .collect::<BTreeSet<_>>();

                        let version_dirs = std::fs::read_dir(tool_dir.path()).map_err(|err| {
                            UpError::Exec(format!(
                                "failed to read data path directory for workdir {} and tool {}: {}",
                                workdir_id, tool_dir_name, err,
                            ))
                        })?;

                        for version_dir in version_dirs {
                            let version_dir = version_dir.map_err(|err| {
                                UpError::Exec(format!(
                                    "failed to read data path directory for workdir {} and tool {}: {}",
                                    workdir_id, tool_dir_name, err,
                                ))
                            })?;

                            let version_dir_name =
                                version_dir.file_name().to_string_lossy().to_string();

                            // Remove the version directory if the version is not expected
                            if !expected_versions.contains(&version_dir_name) {
                                force_remove_dir_all(version_dir.path()).map_err(|err| {
                                    UpError::Exec(format!(
                                        "failed to remove workdir data path for workdir {}, tool {} and version {}: {}",
                                        workdir_id, tool_dir_name, version_dir_name, err
                                    ))
                                })?;
                                continue;
                            }

                            // Check the paths for that version
                            let expected_paths = env_tools
                                .iter()
                                .filter(|tool| tool.tool == tool_dir_name)
                                .filter(|tool| tool.version == version_dir_name)
                                .filter_map(|tool| tool.data_path.clone())
                                .filter_map(|path| {
                                    PathBuf::from(&path)
                                        .strip_prefix(&version_dir.path())
                                        .map(|path| path.to_string_lossy().to_string())
                                        .ok()
                                })
                                .collect::<BTreeSet<_>>();

                            let path_dirs = std::fs::read_dir(version_dir.path()).map_err(|err| {
                                UpError::Exec(format!(
                                    "failed to read data path directory for workdir {}, tool {} and version {}: {}",
                                    workdir_id, tool_dir_name, version_dir_name, err,
                                ))
                            })?;

                            for path_dir in path_dirs {
                                let path_dir = path_dir.map_err(|err| {
                                    UpError::Exec(format!(
                                        "failed to read data path directory for workdir {}, tool {} and version {}: {}",
                                        workdir_id, tool_dir_name, version_dir_name, err,
                                    ))
                                })?;

                                let path_dir_name =
                                    path_dir.file_name().to_string_lossy().to_string();

                                // Remove the path directory if the path is not expected
                                if !expected_paths.contains(&path_dir_name) {
                                    force_remove_dir_all(path_dir.path()).map_err(|err| {
                                        UpError::Exec(format!(
                                            "failed to remove workdir data path for workdir {}, tool {}, version {} and path {}: {}",
                                            workdir_id, tool_dir_name, version_dir_name, path_dir_name, err
                                        ))
                                    })?;
                                }
                            }
                        }
                    }
                }
            }
        }

        let expected_tools = env_tools
            .iter()
            .map(|tool| (tool.tool.clone(), tool.version.clone()))
            .collect::<HashSet<_>>();

        let mut uninstalled = Vec::new();
        let write_cache = AsdfOperationCache::exclusive(|asdf_cache| {
            // Update the asdf versions cache
            let mut updated = false;
            let mut to_remove = Vec::new();

            for (idx, exists) in asdf_cache.installed.iter_mut().enumerate() {
                if exists.required_by.contains(&workdir_id)
                    && !expected_tools.contains(&(exists.tool.clone(), exists.version.clone()))
                {
                    exists.required_by.retain(|id| id != &workdir_id);
                    updated = true;
                }
                if exists.required_by.is_empty() {
                    to_remove.push((idx, exists.clone()));
                }
            }

            if to_remove.is_empty() {
                if let Some(handler) = progress_handler {
                    handler.success_with_message("nothing to do".to_string().light_black());
                }
                return updated;
            }

            for (idx, to_remove) in to_remove.iter().rev() {
                if is_asdf_tool_version_installed(&to_remove.tool, &to_remove.version) {
                    if let Some(handler) = progress_handler {
                        handler.progress(format!(
                            "uninstalling {} {}",
                            to_remove.tool, to_remove.version,
                        ));
                    }

                    let mut asdf_uninstall = tokio::process::Command::new(asdf_bin());
                    asdf_uninstall.arg("uninstall");
                    asdf_uninstall.arg(to_remove.tool.clone());
                    asdf_uninstall.arg(to_remove.version.clone());
                    asdf_uninstall.env("ASDF_DIR", asdf_path());
                    asdf_uninstall.env("ASDF_DATA_DIR", asdf_path());
                    asdf_uninstall.stdout(std::process::Stdio::piped());
                    asdf_uninstall.stderr(std::process::Stdio::piped());

                    if let Err(_err) =
                        run_progress(&mut asdf_uninstall, progress_handler, RunConfig::default())
                    {
                        if let Some(handler) = progress_handler {
                            handler.error_with_message(format!(
                                "failed to uninstall {} {}",
                                to_remove.tool, to_remove.version,
                            ));
                        }
                        return updated;
                    }

                    uninstalled.push(format!("{}:{}", to_remove.tool, to_remove.version));
                }

                asdf_cache.installed.remove(*idx);
                updated = true;
            }

            updated
        });

        if let Err(err) = write_cache {
            if let Some(handler) = progress_handler {
                handler.error_with_message(format!("failed to update cache: {}", err));
            }
            return Err(UpError::Exec("failed to update cache".to_string()));
        }

        if let Some(handler) = progress_handler {
            if !uninstalled.is_empty() {
                let uninstalled = uninstalled
                    .iter()
                    .map(|tool| tool.light_blue().to_string())
                    .collect::<Vec<_>>();
                handler.success_with_message(format!("uninstalled {}", uninstalled.join(", ")));
            } else {
                handler.success_with_message("nothing to do".light_black());
            }
        }

        Ok(())
    }
}

fn version_match(expect: &str, version: &str) -> bool {
    if expect == "latest" {
        let mut prev = '.';
        for c in version.chars() {
            if !c.is_ascii_digit() {
                if c == '.' {
                    if prev == '.' {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            prev = c;
        }
        return true;
    }

    if let Ok(requirements) = semverRange::from_str(expect) {
        if let Ok(version) = semverVersion::from_str(version) {
            // By not directly returning, we allow to keep the prefix
            // check in case the version is not a semver version
            if version.satisfies(&requirements) {
                return true;
            }
        }
    }

    let expect_prefix = format!("{}.", expect);
    if !version.starts_with(&expect_prefix) {
        return false;
    }

    let rest_of_line = version.strip_prefix(&expect_prefix).unwrap();
    rest_of_line.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn detect_version_from_asdf_version_file(tool_name: String, path: PathBuf) -> Option<String> {
    let version_file_path = path.join(".tool-versions");
    if !version_file_path.exists() || version_file_path.is_dir() {
        return None;
    }

    // Read the contents of the file
    match std::fs::read_to_string(&version_file_path) {
        Ok(contents) => {
            let tool_name = tool_name.to_lowercase();

            // Read line by line
            for line in contents.lines() {
                // Trim all leading and trailing whitespaces
                let line = line.trim();

                // Go to next line if the line does not start by the tool name
                if !line.starts_with(&tool_name) {
                    continue;
                }

                // Split the line by whitespace
                let mut parts = line.split_whitespace();

                // Remove first entry
                parts.next();

                // Find the first part that contains only digits and dots, starting with a digit;
                // any other version format is not supported by omni
                for part in parts {
                    if part.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && part.starts_with(|c: char| c.is_ascii_digit())
                    {
                        return Some(part.to_string());
                    }
                }
            }
        }
        Err(_err) => {}
    };

    None
}

fn detect_version_from_tool_version_file(tool_name: String, path: PathBuf) -> Option<String> {
    let tool_name = tool_name.to_lowercase();
    let version_file_prefixes = match tool_name.as_str() {
        "golang" => vec!["go", "golang"],
        "node" => vec!["node", "nodejs"],
        _ => vec![tool_name.as_str()],
    };

    for version_file_prefix in version_file_prefixes {
        let version_file_path = path.join(format!(".{}-version", version_file_prefix));
        if !version_file_path.exists() || version_file_path.is_dir() {
            continue;
        }

        // Read the contents of the file
        match std::fs::read_to_string(&version_file_path) {
            Ok(contents) => {
                // Strip contents of all leading or trailing whitespaces
                let version = contents.trim();
                if !version.is_empty() {
                    return Some(version.to_string());
                }
            }
            Err(_err) => {}
        };
    }

    None
}

#[derive(Debug, Clone)]
pub struct AsdfToolUpVersion {
    pub version: String,
    pub dirs: BTreeSet<String>,
    pub installed: bool,
}
