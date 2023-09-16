use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;

use lazy_static::lazy_static;
use node_semver::Range as semverRange;
use node_semver::Version as semverVersion;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;
use tokio::process::Command as TokioCommand;
use walkdir::WalkDir;

use crate::internal::cache::AsdfInstalled;
use crate::internal::cache::Cache;
use crate::internal::cache::UpEnvironment;
use crate::internal::cache::UpEnvironments;
use crate::internal::cache::UpVersion;
use crate::internal::config::up::tool::UpConfigTool;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::ConfigValue;
use crate::internal::get_cache;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::internal::ENV;
use crate::omni_error;

lazy_static! {
    pub static ref ASDF_PATH: String = {
        let omni_data_home = ENV.data_home.clone();
        let asdf_path = format!("{}/asdf", omni_data_home);

        asdf_path
    };
    pub static ref ASDF_BIN: String = format!("{}/bin/asdf", *ASDF_PATH);
}

fn is_asdf_installed() -> bool {
    let bin_path = std::path::Path::new(&*ASDF_BIN);
    bin_path.is_file() && bin_path.metadata().unwrap().permissions().mode() & 0o111 != 0
}

fn install_asdf(progress_handler: Option<Box<&dyn ProgressHandler>>) -> Result<(), UpError> {
    // Add asdf to PATH if not there yet, as some of the asdf plugins depend on it being
    // in the PATH. We will want it to be at the beginning of the PATH, so that it takes
    // precedence over any other asdf installation.
    let bin_path = PathBuf::from(format!("{}/bin", *ASDF_PATH));
    let path_env = std::env::var("PATH").unwrap();
    let paths: Vec<PathBuf> = std::env::split_paths(&path_env).collect();
    let mut new_paths: Vec<PathBuf> = paths.into_iter().filter(|p| *p != bin_path).collect();
    new_paths.insert(0, bin_path);
    let new_path_env = std::env::join_paths(new_paths).expect("Failed to join paths");
    std::env::set_var("PATH", new_path_env);

    if !is_asdf_installed() {
        if progress_handler.is_some() {
            progress_handler
                .clone()
                .unwrap()
                .progress("installing asdf".to_string());
        }

        let mut git_clone = TokioCommand::new("git");
        git_clone.arg("clone");
        git_clone.arg("https://github.com/asdf-vm/asdf.git");
        git_clone.arg(&*ASDF_PATH);
        git_clone.arg("--branch");
        // We hardcode the version we initially get, but since we update asdf right after,
        // and then update it on every run, this is not _this_ version that will keep being
        // used, we just need "one version" that works well with updating after.
        git_clone.arg("v0.12.0");
        git_clone.stdout(std::process::Stdio::piped());
        git_clone.stderr(std::process::Stdio::piped());

        run_progress(
            &mut git_clone,
            progress_handler.clone(),
            RunConfig::default(),
        )?;
    }

    update_asdf(progress_handler)
}

fn update_asdf(progress_handler: Option<Box<&dyn ProgressHandler>>) -> Result<(), UpError> {
    if !get_cache().should_update_asdf() {
        return Ok(());
    }

    if progress_handler.is_some() {
        progress_handler
            .clone()
            .unwrap()
            .progress("updating asdf".to_string());
    }

    let mut asdf_update = TokioCommand::new(format!("{}", *ASDF_BIN));
    asdf_update.arg("update");
    asdf_update.env("ASDF_DIR", &*ASDF_PATH);
    asdf_update.env("ASDF_DATA_DIR", &*ASDF_PATH);
    asdf_update.stdout(std::process::Stdio::piped());
    asdf_update.stderr(std::process::Stdio::piped());

    if let Err(err) = run_progress(
        &mut asdf_update,
        progress_handler.clone(),
        RunConfig::default(),
    ) {
        return Err(err);
    }

    if let Err(err) = Cache::exclusive(|cache| {
        cache.updated_asdf();
        true
    }) {
        return Err(UpError::Cache(err.to_string()));
    }

    Ok(())
}

fn is_asdf_tool_version_installed(tool: &str, version: &str) -> bool {
    let mut asdf_list = std::process::Command::new(format!("{}", *ASDF_BIN));
    asdf_list.arg("list");
    asdf_list.arg(tool);
    asdf_list.arg(version);
    asdf_list.env("ASDF_DIR", &*ASDF_PATH);
    asdf_list.env("ASDF_DATA_DIR", &*ASDF_PATH);
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
    pub tool: String,
    pub tool_url: Option<String>,
    pub version: String,
    pub dirs: BTreeSet<String>,
    #[serde(skip)]
    detect_version_funcs: Vec<fn(String, PathBuf) -> Option<String>>,
    #[serde(skip)]
    actual_version: OnceCell<String>,
    #[serde(skip)]
    actual_versions: OnceCell<BTreeSet<String>>,
}

impl UpConfigAsdfBase {
    pub fn new(tool: &str, version: &str) -> Self {
        UpConfigAsdfBase {
            tool: tool.to_string(),
            tool_url: None,
            version: version.to_string(),
            dirs: BTreeSet::new(),
            detect_version_funcs: vec![],
            actual_version: OnceCell::new(),
            actual_versions: OnceCell::new(),
        }
    }

    pub fn add_detect_version_func(&mut self, func: fn(String, PathBuf) -> Option<String>) {
        self.detect_version_funcs.push(func);
    }

    fn new_from_auto(&self, version: &str, dirs: BTreeSet<String>) -> Self {
        UpConfigAsdfBase {
            tool: self.tool.clone(),
            tool_url: self.tool_url.clone(),
            version: version.to_string(),
            dirs: dirs.clone(),
            detect_version_funcs: vec![],
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
                if let Some(value) = config_value.get("version") {
                    version = value.as_str().unwrap().to_string();
                }

                if let Some(value) = config_value.get_as_str("dir") {
                    dirs.insert(value.to_string());
                } else if let Some(array) = config_value.get_as_array("dir") {
                    for value in array {
                        dirs.insert(value.as_str().unwrap().to_string());
                    }
                }
            }
        }

        UpConfigAsdfBase {
            tool: tool.to_string(),
            tool_url: tool_url,
            version: version,
            dirs: dirs,
            detect_version_funcs: vec![],
            actual_version: OnceCell::new(),
            actual_versions: OnceCell::new(),
        }
    }

    fn update_cache(&self, progress_handler: Option<Box<&dyn ProgressHandler>>) {
        progress_handler
            .clone()
            .map(|progress_handler| progress_handler.progress("updating cache".to_string()));

        let result = Cache::exclusive(|cache| {
            let mut updated = false;

            let workdir = workdir(".");
            let repo_id = workdir.id();
            if repo_id.is_none() {
                return false;
            }
            let repo_id = repo_id.unwrap();

            let version = self.version(None);
            if version.is_err() {
                return false;
            }
            let version = version.unwrap().to_string();

            let mut dirs = self.dirs.clone();
            if dirs.is_empty() {
                dirs.insert("".to_string());
            }

            // Update the asdf versions cache
            let mut installed = Vec::new();
            if let Some(asdf_cache) = &cache.asdf_operation {
                installed.extend(asdf_cache.installed.clone());
            }

            let mut found = false;
            for exists in installed.iter_mut() {
                if exists.tool == self.tool && exists.version == version {
                    if !exists.required_by.contains(&repo_id) {
                        exists.required_by.push(repo_id.clone());
                        updated = true;
                    }
                    found = true;
                    break;
                }
            }

            if !found {
                installed.push(AsdfInstalled {
                    tool: self.tool.clone(),
                    version: version.clone(),
                    required_by: vec![repo_id.clone()],
                });
                updated = true;
            }

            cache.set_asdf_operation_installed(installed);

            // Update the repository up cache
            let mut up_env = HashMap::new();
            if let Some(up_cache) = &cache.up_environments {
                up_env = up_cache.env.clone();
            }

            if !up_env.contains_key(&repo_id) {
                up_env.insert(repo_id.clone(), UpEnvironment::new());
            }
            let repo_up_env = up_env.get_mut(&repo_id).unwrap();

            for exists in repo_up_env.versions.iter_mut() {
                if exists.tool == self.tool && exists.version == version {
                    dirs.remove(&exists.dir);
                    if dirs.is_empty() {
                        break;
                    }
                }
            }

            if !dirs.is_empty() {
                for dir in dirs {
                    repo_up_env.versions.push(UpVersion {
                        tool: self.tool.clone(),
                        version: version.clone(),
                        dir: dir.clone(),
                    });
                }

                cache.up_environments = Some(UpEnvironments {
                    env: up_env.clone(),
                    updated_at: OffsetDateTime::now_utc(),
                });

                updated = true;
            }

            updated
        });

        if let Err(err) = result {
            progress_handler.clone().map(|progress_handler| {
                progress_handler.progress(format!("failed to update cache: {}", err))
            });
        } else {
            progress_handler
                .clone()
                .map(|progress_handler| progress_handler.progress("updated cache".to_string()));
        }
    }

    pub fn up(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        let desc = format!("{} ({}):", self.tool, self.version).light_blue();
        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, progress))
        };
        let progress_handler: Option<Box<&dyn ProgressHandler>> =
            Some(Box::new(progress_handler.as_ref()));

        if let Err(err) = install_asdf(progress_handler.clone()) {
            if progress_handler.is_some() {
                progress_handler
                    .clone()
                    .unwrap()
                    .error_with_message(format!("error: {}", err));
            }
            return Err(err);
        }

        if let Err(err) = self.install_plugin(progress_handler.clone()) {
            if progress_handler.is_some() {
                progress_handler
                    .clone()
                    .unwrap()
                    .error_with_message(format!("error: {}", err));
            }
            return Err(err);
        }

        if self.version == "auto" {
            let mut detected_versions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

            // Get the current directory
            let current_dir = std::env::current_dir().expect("failed to get current directory");

            let mut search_dirs = self.dirs.clone();
            if search_dirs.is_empty() {
                search_dirs.insert("".to_string());
            }

            let mut detect_version_funcs = self.detect_version_funcs.clone();
            detect_version_funcs.push(detect_version_from_asdf_version_file);
            detect_version_funcs.push(detect_version_from_version_file);

            for search_dir in search_dirs.iter() {
                // For safety, we remove any leading slashes from the search directory,
                // as we only want to search in the workdir
                let mut search_dir = search_dir.clone();
                while search_dir.starts_with("/") {
                    search_dir.remove(0);
                }

                // Append the search directory to the current directory, since we are
                // at the root of the workdir
                let search_path = current_dir.join(search_dir);

                for entry in WalkDir::new(search_path).follow_links(true) {
                    if let Ok(entry) = entry {
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
                                while dir.starts_with("/") {
                                    dir.remove(0);
                                }
                                while dir.ends_with("/") {
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
            }

            if detected_versions.is_empty() {
                progress_handler.clone().map(|progress_handler| {
                    progress_handler.success_with_message("no version detected".to_string())
                });
                return Ok(());
            }

            let mut installed_versions = Vec::new();
            let mut already_installed_versions = Vec::new();
            let mut all_versions = BTreeSet::new();

            for (version, dirs) in detected_versions.iter() {
                let asdf_base = self.new_from_auto(version, dirs.clone());
                let installed = asdf_base.install_version(progress_handler.clone());
                if installed.is_err() {
                    let err = installed.err().unwrap();
                    if progress_handler.is_some() {
                        progress_handler
                            .clone()
                            .unwrap()
                            .error_with_message(format!("error: {}", err));
                    }
                    return Err(err);
                }

                let version = asdf_base.version(None).unwrap();
                all_versions.insert(version.clone());
                if installed.unwrap() {
                    installed_versions.push(version.clone());
                } else {
                    already_installed_versions.push(version.clone());
                }

                asdf_base.update_cache(progress_handler.clone());
            }

            self.actual_versions
                .set(all_versions)
                .expect("failed to set installed versions");

            if progress_handler.is_some() {
                let mut msgs = Vec::new();

                if !installed_versions.is_empty() {
                    msgs.push(
                        format!(
                            "{} {} installed",
                            self.tool,
                            installed_versions
                                .iter()
                                .map(|version| format!("{}", version))
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
                                .map(|version| format!("{}", version))
                                .collect::<Vec<String>>()
                                .join(", ")
                        )
                        .light_black(),
                    );
                }

                progress_handler
                    .clone()
                    .unwrap()
                    .success_with_message(msgs.join(", "));
            }
        } else {
            let install_version = self.install_version(progress_handler.clone());
            if install_version.is_err() {
                let err = install_version.err().unwrap();
                if progress_handler.is_some() {
                    progress_handler
                        .clone()
                        .unwrap()
                        .error_with_message(format!("error: {}", err));
                }
                return Err(err);
            }

            self.update_cache(progress_handler.clone());

            if progress_handler.is_some() {
                let msg = if install_version.unwrap() {
                    format!("{} {} installed", self.tool, self.version(None).unwrap()).green()
                } else {
                    format!(
                        "{} {} already installed",
                        self.tool,
                        self.version(None).unwrap(),
                    )
                    .light_black()
                };
                progress_handler.clone().unwrap().success_with_message(msg);
            }
        }

        Ok(())
    }

    pub fn down(&self, _progress: Option<(usize, usize)>) -> Result<(), UpError> {
        Ok(())
    }

    fn versions(&self) -> BTreeSet<String> {
        if self.version != "auto" {
            let mut versions = BTreeSet::new();
            if let Ok(version) = self.version(None) {
                versions.insert(version.clone());
            }
            return versions;
        }
        self.actual_versions.get_or_init(|| BTreeSet::new()).clone()
    }

    fn version(
        &self,
        progress_handler: Option<Box<&dyn ProgressHandler>>,
    ) -> Result<&String, UpError> {
        let version = self.actual_version.get_or_init(|| {
            if let Err(_) = self.update_plugin(progress_handler.clone()) {
                return "".to_string();
            }

            if progress_handler.is_some() {
                progress_handler
                    .clone()
                    .unwrap()
                    .progress(format!("checking available versions"));
            }

            let available_versions =
                if let Some(versions) = get_cache().get_asdf_plugin_versions(&self.tool) {
                    versions
                } else {
                    let mut asdf_list_all = std::process::Command::new(format!("{}", *ASDF_BIN));
                    asdf_list_all.arg("list");
                    asdf_list_all.arg("all");
                    asdf_list_all.arg(self.tool.clone());
                    asdf_list_all.env("ASDF_DIR", &*ASDF_PATH);
                    asdf_list_all.env("ASDF_DATA_DIR", &*ASDF_PATH);
                    asdf_list_all.stdout(std::process::Stdio::piped());
                    asdf_list_all.stderr(std::process::Stdio::piped());

                    if let Ok(output) = asdf_list_all.output() {
                        if output.status.success() {
                            let stdout = String::from_utf8(output.stdout).unwrap();
                            let mut versions = Vec::new();
                            let mut lines = stdout.lines();
                            while let Some(line) = lines.next() {
                                let line = line.trim();

                                if line.is_empty() {
                                    continue;
                                }

                                versions.push(line.to_string());
                            }

                            if let Err(err) = Cache::exclusive(|cache| {
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
        let mut asdf_plugin_list = std::process::Command::new(format!("{}", *ASDF_BIN));
        asdf_plugin_list.arg("plugin");
        asdf_plugin_list.arg("list");
        asdf_plugin_list.env("ASDF_DIR", &*ASDF_PATH);
        asdf_plugin_list.env("ASDF_DATA_DIR", &*ASDF_PATH);
        asdf_plugin_list.stdout(std::process::Stdio::piped());
        asdf_plugin_list.stderr(std::process::Stdio::null());

        match asdf_plugin_list.output() {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8(output.stdout).unwrap();
                    let mut lines = stdout.lines();
                    while let Some(line) = lines.next() {
                        if line.trim() == self.tool {
                            return true;
                        }
                    }
                }
            }
            Err(_) => {}
        }

        false
    }

    fn install_plugin(
        &self,
        progress_handler: Option<Box<&dyn ProgressHandler>>,
    ) -> Result<(), UpError> {
        if self.is_plugin_installed() {
            return Ok(());
        }

        if progress_handler.is_some() {
            progress_handler
                .clone()
                .unwrap()
                .progress(format!("installing {} plugin", self.tool));
        }

        let mut asdf_plugin_add = TokioCommand::new(format!("{}", *ASDF_BIN));
        asdf_plugin_add.arg("plugin");
        asdf_plugin_add.arg("add");
        asdf_plugin_add.arg(self.tool.clone());
        if let Some(tool_url) = &self.tool_url {
            asdf_plugin_add.arg(tool_url.clone());
        }
        asdf_plugin_add.env("ASDF_DIR", &*ASDF_PATH);
        asdf_plugin_add.env("ASDF_DATA_DIR", &*ASDF_PATH);
        asdf_plugin_add.stdout(std::process::Stdio::piped());
        asdf_plugin_add.stderr(std::process::Stdio::piped());

        run_progress(
            &mut asdf_plugin_add,
            progress_handler.clone(),
            RunConfig::default(),
        )
    }

    fn update_plugin(
        &self,
        progress_handler: Option<Box<&dyn ProgressHandler>>,
    ) -> Result<(), UpError> {
        if !get_cache().should_update_asdf_plugin(&self.tool) {
            return Ok(());
        }

        if progress_handler.is_some() {
            progress_handler
                .clone()
                .unwrap()
                .progress(format!("updating {} plugin", self.tool));
        }

        let mut asdf_plugin_update = TokioCommand::new(format!("{}", *ASDF_BIN));
        asdf_plugin_update.arg("plugin");
        asdf_plugin_update.arg("update");
        asdf_plugin_update.arg(self.tool.clone());
        asdf_plugin_update.env("ASDF_DIR", &*ASDF_PATH);
        asdf_plugin_update.env("ASDF_DATA_DIR", &*ASDF_PATH);
        asdf_plugin_update.stdout(std::process::Stdio::piped());
        asdf_plugin_update.stderr(std::process::Stdio::piped());

        if let Err(err) = run_progress(
            &mut asdf_plugin_update,
            progress_handler.clone(),
            RunConfig::default(),
        ) {
            return Err(err);
        }

        // Update the cache
        if let Err(err) = Cache::exclusive(|cache| {
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

        is_asdf_tool_version_installed(&self.tool, &version)
    }

    fn install_version(
        &self,
        progress_handler: Option<Box<&dyn ProgressHandler>>,
    ) -> Result<bool, UpError> {
        let version = self.version(progress_handler.clone())?;

        if self.is_version_installed() {
            return Ok(false);
        }

        if progress_handler.is_some() {
            progress_handler
                .clone()
                .unwrap()
                .progress(format!("installing {} {}", self.tool, version));
        }

        let mut asdf_install = tokio::process::Command::new(format!("{}", *ASDF_BIN));
        asdf_install.arg("install");
        asdf_install.arg(self.tool.clone());
        asdf_install.arg(version);
        asdf_install.env("ASDF_DIR", &*ASDF_PATH);
        asdf_install.env("ASDF_DATA_DIR", &*ASDF_PATH);
        asdf_install.stdout(std::process::Stdio::piped());
        asdf_install.stderr(std::process::Stdio::piped());

        if let Err(err) = run_progress(
            &mut asdf_install,
            progress_handler.clone(),
            RunConfig::default(),
        ) {
            return Err(err);
        }

        Ok(true)
    }

    pub fn cleanup_unused(
        steps: Vec<UpConfigTool>,
        progress: Option<(usize, usize)>,
    ) -> Result<(), UpError> {
        let desc = format!("resources cleanup:").light_blue();
        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, progress))
        };
        let progress_handler: Option<Box<&dyn ProgressHandler>> =
            Some(Box::new(progress_handler.as_ref()));

        let mut expected_tools = HashSet::new();
        let all_tool_versions = steps
            .iter()
            .map(|step| step.asdf_tool())
            .filter(|tool| tool.is_some())
            .map(|tool| tool.unwrap())
            .map(|tool| (tool.tool.clone(), tool.versions()))
            .filter(|(_, version)| !version.is_empty());
        for (tool, versions) in all_tool_versions {
            for version in versions {
                expected_tools.insert((tool.clone(), version));
            }
        }

        let mut uninstalled = Vec::new();
        let write_cache = Cache::exclusive(|cache| {
            let workdir = workdir(".");
            let repo_id = workdir.id();
            if repo_id.is_none() {
                return false;
            }
            let repo_id = repo_id.unwrap();

            // Update the asdf versions cache
            let mut updated = false;
            if let Some(asdf_cache) = &mut cache.asdf_operation {
                let mut to_remove = Vec::new();

                for (idx, exists) in asdf_cache.installed.iter_mut().enumerate() {
                    if exists.required_by.contains(&repo_id)
                        && !expected_tools.contains(&(exists.tool.clone(), exists.version.clone()))
                    {
                        exists.required_by.retain(|id| id != &repo_id);
                        updated = true;
                    }
                    if exists.required_by.is_empty() {
                        to_remove.push((idx, exists.clone()));
                    }
                }

                if to_remove.len() == 0 {
                    progress_handler.clone().map(|handler| {
                        handler.success_with_message(format!("nothing to do").light_black());
                    });
                    return updated;
                }

                for (idx, to_remove) in to_remove.iter().rev() {
                    if is_asdf_tool_version_installed(&to_remove.tool, &to_remove.version) {
                        progress_handler.clone().map(|handler| {
                            handler.progress(format!(
                                "uninstalling {} {}",
                                to_remove.tool, to_remove.version,
                            ));
                        });

                        let mut asdf_uninstall =
                            tokio::process::Command::new(format!("{}", *ASDF_BIN));
                        asdf_uninstall.arg("uninstall");
                        asdf_uninstall.arg(to_remove.tool.clone());
                        asdf_uninstall.arg(to_remove.version.clone());
                        asdf_uninstall.env("ASDF_DIR", &*ASDF_PATH);
                        asdf_uninstall.env("ASDF_DATA_DIR", &*ASDF_PATH);
                        asdf_uninstall.stdout(std::process::Stdio::piped());
                        asdf_uninstall.stderr(std::process::Stdio::piped());

                        if let Err(_err) = run_progress(
                            &mut asdf_uninstall,
                            progress_handler.clone(),
                            RunConfig::default(),
                        ) {
                            progress_handler.clone().map(|handler| {
                                handler.error_with_message(format!(
                                    "failed to uninstall {} {}",
                                    to_remove.tool, to_remove.version,
                                ));
                            });
                            return updated;
                        }

                        uninstalled.push(format!("{}:{}", to_remove.tool, to_remove.version));
                    }

                    asdf_cache.installed.remove(*idx);
                    updated = true;
                }
            }

            updated
        });

        if let Err(err) = write_cache {
            progress_handler.clone().map(|handler| {
                handler.error_with_message(format!("failed to update cache: {}", err));
            });
            return Err(UpError::Exec("failed to update cache".to_string()));
        }

        progress_handler.clone().map(|handler| {
            if uninstalled.len() > 0 {
                let uninstalled = uninstalled
                    .iter()
                    .map(|tool| tool.light_blue().to_string())
                    .collect::<Vec<_>>();
                handler.success_with_message(format!("uninstalled {}", uninstalled.join(", ")));
            } else {
                handler.success_with_message(format!("nothing to do").light_black());
            }
        });

        Ok(())
    }
}

fn version_match(expect: &str, version: &str) -> bool {
    if expect == "latest" {
        let mut prev = '.';
        for c in version.chars() {
            if !c.is_digit(10) {
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
    rest_of_line.chars().all(|c| c.is_digit(10) || c == '.')
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
                    if part.chars().all(|c| c.is_digit(10) || c == '.')
                        && part.starts_with(|c: char| c.is_digit(10))
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

fn detect_version_from_version_file(tool_name: String, path: PathBuf) -> Option<String> {
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
