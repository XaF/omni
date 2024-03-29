use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;

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
use crate::internal::config::up::homebrew::HomebrewInstall;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigNix;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::env::data_home;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;
use crate::omni_warning;

lazy_static! {
    static ref ASDF_PATH: String = format!("{}/asdf", data_home());
    static ref ASDF_BIN: String = format!("{}/bin/asdf", *ASDF_PATH);
}

type DetectVersionFunc = fn(String, PathBuf) -> Option<String>;
type PostInstallFunc = fn(
    &dyn ProgressHandler,
    Option<ConfigValue>,
    String,
    String,
    Vec<AsdfToolUpVersion>,
) -> Result<(), UpError>;

fn asdf_path() -> String {
    (*ASDF_PATH).clone()
}

fn asdf_bin() -> &'static str {
    ASDF_BIN.as_str()
}

fn asdf_async_command() -> TokioCommand {
    let mut asdf = TokioCommand::new(asdf_bin());
    asdf.env("ASDF_DIR", asdf_path());
    asdf.env("ASDF_DATA_DIR", asdf_path());
    asdf.env_remove("INSTALL_PREFIX");
    asdf.env_remove("DESTDIR");
    asdf.stdout(std::process::Stdio::piped());
    asdf.stderr(std::process::Stdio::piped());
    asdf
}

fn asdf_sync_command() -> std::process::Command {
    let mut asdf = std::process::Command::new(asdf_bin());
    asdf.env("ASDF_DIR", asdf_path());
    asdf.env("ASDF_DATA_DIR", asdf_path());
    asdf.env_remove("INSTALL_PREFIX");
    asdf.env_remove("DESTDIR");
    asdf.stdout(std::process::Stdio::piped());
    asdf.stderr(std::process::Stdio::piped());
    asdf
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

    let mut asdf_update = asdf_async_command();
    asdf_update.arg("update");

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
    let mut asdf_list = asdf_sync_command();
    asdf_list.arg("list");
    asdf_list.arg(tool);
    asdf_list.arg(version);
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
    #[serde(skip)]
    pub tool: String,

    /// The URL to use to install the tool.
    #[serde(skip)]
    pub tool_url: Option<String>,

    /// The version of the tool to install, as specified in the config file.
    pub version: String,

    /// A list of directories to install the tool for.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
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

    /// The configuration value that was used to create this object.
    #[serde(skip)]
    config_value: Option<ConfigValue>,

    /// Whether the up operation succeeded. If unset, the operation has not
    /// been attempted yet.
    #[serde(skip)]
    up_succeeded: OnceCell<bool>,

    /// The tool object representing the dependencies for this asdf tool.
    #[serde(skip)]
    deps: OnceCell<Box<UpConfigTool>>,
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
            config_value: None,
            up_succeeded: OnceCell::new(),
            deps: OnceCell::new(),
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

            up_succeeded: OnceCell::new(),
            deps: OnceCell::new(),

            // We can ignore all those fields, as they won't be used,
            // since the version passed to that call is a specific version
            // that we got from running the detection functions from a
            // main instance called with "auto" as the version.
            detect_version_funcs: vec![],
            post_install_funcs: vec![],
            actual_version: OnceCell::new(),
            actual_versions: OnceCell::new(),
            config_value: None,
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
            config_value: config_value.cloned(),
            up_succeeded: OnceCell::new(),
            deps: OnceCell::new(),
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
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        if self.up_succeeded.get().is_some() {
            return Err(UpError::Exec("up operation already attempted".to_string()));
        }

        let result = self.run_up(options, progress_handler);
        if let Err(err) = self.up_succeeded.set(result.is_ok()) {
            omni_warning!(format!("failed to record status of up operation: {}", err));
        }

        result
    }

    pub fn was_upped(&self) -> bool {
        matches!(self.up_succeeded.get(), Some(true))
    }

    fn run_up(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.init(format!("{} ({}):", self.tool, self.version).light_blue());

        // Make sure that dependencies are installed
        let subhandler = progress_handler.subhandler(&"deps: ".light_black());
        self.deps().up(options, &subhandler)?;
        update_dynamic_env_for_command(".");

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
                        self.config_value.clone(),
                        self.tool.clone(),
                        self.version.clone(),
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
                                self.config_value.clone(),
                                self.tool.clone(),
                                self.version.clone(),
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

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        self.deps().down(progress_handler)
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
                let mut asdf_list_all = asdf_sync_command();
                asdf_list_all.arg("list");
                asdf_list_all.arg("all");
                asdf_list_all.arg(self.tool.clone());

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
        let mut asdf_plugin_list = asdf_sync_command();
        asdf_plugin_list.arg("plugin");
        asdf_plugin_list.arg("list");
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

        let mut asdf_plugin_add = asdf_async_command();
        asdf_plugin_add.arg("plugin");
        asdf_plugin_add.arg("add");
        asdf_plugin_add.arg(self.tool.clone());
        if let Some(tool_url) = &self.tool_url {
            asdf_plugin_add.arg(tool_url.clone());
        }

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

        let mut asdf_plugin_update = asdf_async_command();
        asdf_plugin_update.arg("plugin");
        asdf_plugin_update.arg("update");
        asdf_plugin_update.arg(self.tool.clone());

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

        let mut asdf_install = asdf_async_command();
        asdf_install.arg("install");
        asdf_install.arg(self.tool.clone());
        asdf_install.arg(version);

        run_progress(
            &mut asdf_install,
            Some(progress_handler),
            RunConfig::default(),
        )?;

        Ok(true)
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        let workdir = workdir(".");

        let wd_data_path = match workdir.data_path() {
            Some(wd_data_path) => wd_data_path,
            None => return vec![],
        };

        let version = match self.version(None) {
            Ok(version) => version,
            Err(_) => return vec![],
        };

        let tool_data_path = wd_data_path.join(&self.tool).join(version);

        let mut dirs = self.dirs.clone();
        if dirs.is_empty() {
            dirs.insert("".to_string());
        }

        let mut data_paths = BTreeSet::new();
        for dir in dirs {
            let hashed_dir = data_path_dir_hash(&dir);
            data_paths.insert(tool_data_path.join(&hashed_dir));
        }

        // Add also all data paths from dependencies
        data_paths.extend(self.deps().data_paths());

        data_paths.into_iter().collect()
    }

    pub fn cleanup(progress_handler: &dyn ProgressHandler) -> Result<Option<String>, UpError> {
        let workdir = workdir(".");
        let workdir_id = match workdir.id() {
            Some(workdir_id) => workdir_id,
            None => return Err(UpError::Exec("failed to get workdir id".to_string())),
        };

        // Get the expected installed versions of the tool from
        // the up environment cache
        let mut env_tools = Vec::new();
        if let Some(env) = UpEnvironmentsCache::get().get_env(&workdir_id) {
            env_tools.extend(env.versions.iter().cloned());
        }

        let expected_tools = env_tools
            .iter()
            .map(|tool| (tool.tool.clone(), tool.version.clone()))
            .collect::<HashSet<_>>();

        let mut uninstalled = Vec::new();
        if let Err(err) = AsdfOperationCache::exclusive(|asdf_cache| {
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
                return updated;
            }

            for (idx, to_remove) in to_remove.iter().rev() {
                if is_asdf_tool_version_installed(&to_remove.tool, &to_remove.version) {
                    progress_handler.progress(format!(
                        "uninstalling {} {}",
                        to_remove.tool, to_remove.version,
                    ));

                    let mut asdf_uninstall = asdf_async_command();
                    asdf_uninstall.arg("uninstall");
                    asdf_uninstall.arg(to_remove.tool.clone());
                    asdf_uninstall.arg(to_remove.version.clone());

                    if let Err(_err) = run_progress(
                        &mut asdf_uninstall,
                        Some(progress_handler),
                        RunConfig::default(),
                    ) {
                        progress_handler.error_with_message(format!(
                            "failed to uninstall {} {}",
                            to_remove.tool, to_remove.version,
                        ));
                        return updated;
                    }

                    uninstalled.push(format!("{}:{}", to_remove.tool, to_remove.version));
                }

                asdf_cache.installed.remove(*idx);
                updated = true;
            }

            updated
        }) {
            progress_handler.progress(format!("failed to update cache: {}", err));
            return Err(UpError::Exec("failed to update cache".to_string()));
        }

        if uninstalled.is_empty() {
            Ok(None)
        } else {
            let uninstalled = uninstalled
                .iter()
                .map(|tool| tool.light_blue().to_string())
                .collect::<Vec<_>>();
            Ok(Some(format!("uninstalled {}", uninstalled.join(", "))))
        }
    }

    fn deps(&self) -> &UpConfigTool {
        self.deps
            .get_or_init(|| {
                Box::new(UpConfigTool::Any(vec![
                    self.deps_using_homebrew(),
                    self.deps_using_nix(),
                ]))
            })
            .as_ref()
    }

    fn deps_using_homebrew(&self) -> UpConfigTool {
        let mut homebrew_install = vec![
            HomebrewInstall::new_formula("autoconf"),
            // HomebrewInstall::new_formula("automake"),
            HomebrewInstall::new_formula("coreutils"),
            HomebrewInstall::new_formula("curl"),
            // HomebrewInstall::new_formula("libtool"),
            HomebrewInstall::new_formula("libyaml"),
            HomebrewInstall::new_formula("openssl@3"),
            HomebrewInstall::new_formula("readline"),
            // HomebrewInstall::new_formula("unixodbc"),
        ];

        match self.tool.as_str() {
            "python" => {
                homebrew_install.extend(vec![
                    HomebrewInstall::new_formula("pkg-config"),
                    // HomebrewInstall::new_formula("sqlite"),
                    // HomebrewInstall::new_formula("xz"),
                ]);
            }
            "rust" => {
                homebrew_install.extend(vec![
                    HomebrewInstall::new_formula("libgit2"),
                    HomebrewInstall::new_formula("libssh2"),
                    HomebrewInstall::new_formula("llvm"),
                    HomebrewInstall::new_formula("pkg-config"),
                ]);
            }
            _ => {}
        }

        UpConfigTool::Homebrew(UpConfigHomebrew {
            install: homebrew_install,
            tap: vec![],
        })
    }

    fn deps_using_nix(&self) -> UpConfigTool {
        let mut nix_packages = vec!["gawk", "gnused", "openssl", "readline"];

        match self.tool.as_str() {
            "python" => {
                nix_packages.extend(vec![
                    "bzip2",
                    "gcc",
                    "gdbm",
                    "gnumake",
                    "libffi",
                    "lzma",
                    "ncurses",
                    "pkg-config",
                    "sqlite",
                    "zlib",
                ]);
            }
            "ruby" => {
                nix_packages.extend(vec!["libyaml"]);
            }
            _ => {}
        }

        UpConfigTool::Nix(UpConfigNix::new_from_packages(
            nix_packages.into_iter().map(|p| p.to_string()).collect(),
        ))
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
