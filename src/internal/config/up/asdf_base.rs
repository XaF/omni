use std::collections::HashMap;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::AsdfInstalled;
use crate::internal::cache::AsdfOperation;
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
use crate::internal::env::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::ENV;

lazy_static! {
    pub static ref ASDF_PATH: String = {
        if let Ok(asdf_data_dir) = std::env::var("ASDF_DATA_DIR") {
            if !asdf_data_dir.is_empty() {
                return asdf_data_dir;
            }
        }

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
    if progress_handler.is_some() {
        progress_handler
            .clone()
            .unwrap()
            .progress("updating asdf".to_string());
    }

    let mut asdf_update = TokioCommand::new(format!("{}", *ASDF_BIN));
    asdf_update.arg("update");
    asdf_update.env("ASDF_DATA_DIR", &*ASDF_PATH);
    asdf_update.stdout(std::process::Stdio::piped());
    asdf_update.stderr(std::process::Stdio::piped());

    run_progress(
        &mut asdf_update,
        progress_handler.clone(),
        RunConfig::default(),
    )
}

fn is_asdf_tool_version_installed(tool: &str, version: &str) -> bool {
    let mut asdf_list = std::process::Command::new(format!("{}", *ASDF_BIN));
    asdf_list.arg("list");
    asdf_list.arg(tool);
    asdf_list.arg(version);
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigAsdfBase {
    pub tool: String,
    pub version: String,
    #[serde(skip)]
    actual_version: OnceCell<String>,
}

impl UpConfigAsdfBase {
    pub fn from_config_value(tool: &str, config_value: Option<&ConfigValue>) -> Self {
        let mut version = "latest".to_string();
        if let Some(config_value) = config_value {
            if let Some(value) = config_value.as_str() {
                version = value.to_string();
            } else if let Some(value) = config_value.get("version") {
                version = value.as_str().unwrap().to_string();
            }
        }

        UpConfigAsdfBase {
            tool: tool.to_string(),
            version: version,
            actual_version: OnceCell::new(),
        }
    }

    fn update_cache(&self, progress_handler: Option<Box<&dyn ProgressHandler>>) {
        progress_handler
            .clone()
            .map(|progress_handler| progress_handler.progress("updating cache".to_string()));

        let result = Cache::exclusive(|cache| {
            let git_env = git_env(".");
            let repo_id = git_env.id();
            if repo_id.is_none() {
                return false;
            }
            let repo_id = repo_id.unwrap();

            let version = self.version(None);
            if version.is_err() {
                return false;
            }
            let version = version.unwrap().to_string();

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
            }

            cache.asdf_operation = Some(AsdfOperation {
                installed: installed.clone(),
                updated_at: OffsetDateTime::now_utc(),
            });

            // Update the repository up cache
            let mut up_env = HashMap::new();
            if let Some(up_cache) = &cache.up_environments {
                up_env = up_cache.env.clone();
            }

            if !up_env.contains_key(&repo_id) {
                up_env.insert(repo_id.clone(), UpEnvironment::new());
            }
            let repo_up_env = up_env.get_mut(&repo_id).unwrap();

            repo_up_env.versions.push(UpVersion {
                tool: self.tool.clone(),
                version: version.clone(),
            });

            cache.up_environments = Some(UpEnvironments {
                env: up_env.clone(),
                updated_at: OffsetDateTime::now_utc(),
            });

            true
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

        if progress_handler.is_some() {
            self.update_cache(progress_handler.clone());
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
        Ok(())
    }

    pub fn down(&self, _progress: Option<(usize, usize)>) -> Result<(), UpError> {
        Ok(())
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

            let mut asdf_list_all = std::process::Command::new(format!("{}", *ASDF_BIN));
            asdf_list_all.arg("list");
            asdf_list_all.arg("all");
            asdf_list_all.arg(self.tool.clone());
            asdf_list_all.env("ASDF_DATA_DIR", &*ASDF_PATH);
            asdf_list_all.stdout(std::process::Stdio::piped());
            asdf_list_all.stderr(std::process::Stdio::piped());

            if let Ok(output) = asdf_list_all.output() {
                if output.status.success() {
                    let stdout = String::from_utf8(output.stdout).unwrap();
                    let mut lines = stdout.lines();
                    let mut version = "".to_string();
                    while let Some(line) = lines.next() {
                        let line = line.trim();

                        if line.is_empty() {
                            continue;
                        }

                        if version_match(&self.version, line) {
                            version = line.to_string();
                        }
                    }
                    return version;
                }
            }

            "".to_string()
        });

        if version.is_empty() {
            return Err(UpError::Exec(format!(
                "No {} version found matching {}",
                self.tool, self.version,
            )));
        }

        Ok(version)
    }

    fn version_major(&self) -> Result<String, UpError> {
        let version = self.version(None)?;
        let version_major = version.split('.').next().unwrap();
        Ok(version_major.to_string())
    }

    fn version_minor(&self) -> Result<String, UpError> {
        let version = self.version(None)?;
        let version_minor = version.split('.').take(2).collect::<Vec<_>>().join(".");
        Ok(version_minor.to_string())
    }

    fn is_plugin_installed(&self) -> bool {
        let mut asdf_plugin_list = std::process::Command::new(format!("{}", *ASDF_BIN));
        asdf_plugin_list.arg("plugin");
        asdf_plugin_list.arg("list");
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
        asdf_plugin_update.env("ASDF_DATA_DIR", &*ASDF_PATH);
        asdf_plugin_update.stdout(std::process::Stdio::piped());
        asdf_plugin_update.stderr(std::process::Stdio::piped());

        run_progress(
            &mut asdf_plugin_update,
            progress_handler.clone(),
            RunConfig::default(),
        )
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

        let expected_tools = steps
            .iter()
            .map(|step| step.asdf_tool())
            .filter(|tool| tool.is_some())
            .map(|tool| tool.unwrap())
            .map(|tool| (tool.tool.clone(), tool.version(None)))
            .filter(|(_, version)| version.is_ok())
            .map(|(tool, version)| (tool, version.unwrap().clone()))
            .collect::<HashSet<_>>();

        let mut uninstalled = Vec::new();
        let write_cache = Cache::exclusive(|cache| {
            let git_env = git_env(".");
            let repo_id = git_env.id();
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
    let rest_of_line = if expect == "latest" {
        version
    } else if version.starts_with(&expect) {
        version.strip_prefix(&expect).unwrap()
    } else {
        return false;
    };

    rest_of_line.chars().all(|c| c.is_digit(10) || c == '.')
}
