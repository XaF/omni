use std::path::PathBuf;

use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::internal::cache::Cache;
use crate::internal::cache::UpEnvironments;
use crate::internal::cache::UpVersion;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpError;
use crate::internal::config::up::ASDF_PATH;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::internal::ConfigValue;
use crate::internal::ENV;

lazy_static! {
    pub static ref VENV_PATH: String = {
        let omni_data_home = ENV.data_home.clone();
        format!("{}/python-venv", omni_data_home)
    };
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigPython {
    pub version: Option<String>,
    pub with_venv: Option<bool>,
    #[serde(skip)]
    pub asdf_base: OnceCell<UpConfigAsdfBase>,
}

impl UpConfigPython {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        UpConfigPython {
            version: version_from_config(config_value),
            with_venv: with_venv_from_config(config_value),
            asdf_base: OnceCell::new(),
        }
    }

    pub fn up(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        self.asdf_base()?.up(progress)?;

        let desc = "python".to_string().light_blue();
        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, progress))
        };
        let progress_handler: Option<Box<&dyn ProgressHandler>> =
            Some(Box::new(progress_handler.as_ref()));
        let progress_handler = progress_handler.as_ref();

        if matches!(self.with_venv, Some(true) | None) {
            self.update_cache_for_venv(progress_handler.cloned());
            if !self.venv_present() {
                let msg = "setting up venv".to_string().green();
                progress_handler.map(|ph| ph.success_with_message(msg));
                self.venv_setup().unwrap();
            } else {
                let msg = "venv already set up".to_string().light_black();
                progress_handler.map(|ph| ph.success_with_message(msg));
            }
        }

        Ok(())
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        if self.venv_present() {
            std::fs::remove_dir_all(self.venv_dir()).unwrap()
        }
        self.asdf_base()?.down(progress)
    }

    pub fn asdf_base(&self) -> Result<&UpConfigAsdfBase, UpError> {
        self.asdf_base.get_or_try_init(|| {
            let version = if let Some(version) = &self.version {
                version.clone()
            } else {
                "latest".to_string()
            };

            let asdf_base = UpConfigAsdfBase::new("python", version.as_ref());

            Ok(asdf_base)
        })
    }

    fn venv_present(&self) -> bool {
        let dir = self.venv_dir();
        let dir_exists = dir.try_exists().unwrap();
        let pyvenv_exists = dir.join("pyvenv.cfg").try_exists().unwrap();
        if dir_exists ^ pyvenv_exists {
            panic!("a venv dir exists, but it was not set up");
        }
        pyvenv_exists
    }

    fn venv_setup(&self) -> std::io::Result<()> {
        let version = &self.asdf_base().unwrap().version;
        let tool_prefix = PathBuf::from(format!("{}/installs/python/{}", *ASDF_PATH, version));
        let python_path = tool_prefix.join("bin").join("python3");
        let venv_dir = self.venv_dir();
        let venv_bin = &venv_dir.join("bin");

        std::fs::create_dir_all(&venv_dir).unwrap();
        std::process::Command::new(python_path)
            .args(["-m", "venv", venv_dir.to_str().unwrap()])
            .output()
            .expect("failed to create venv");

        // The venv activate scripts can break a given shell session since the activate/deactivate
        // stashes and unstashes environment values (include PATH) outside of omni's dynenv.
        for script in ["activate", "activate.csh", "activate.fish", "Activate.ps1"] {
            //std::fs::remove_file(&venv_bin.join(script))?;
        }

        Ok(())
    }

    fn update_cache_for_venv(&self, progress_handler: Option<Box<&dyn ProgressHandler>>) {
        progress_handler
            .clone()
            .map(|progress_handler| progress_handler.progress("updating cache".to_string()));

        let result = Cache::exclusive(|cache| {
            let repo_id = if let Some(repo_id) = workdir(".").id() {
                repo_id
            } else {
                return false;
            };
            let version = &self.asdf_base().unwrap().version;

            let mut up_env = cache.up_environments.as_ref().unwrap().env.clone();
            let repo_up_env = up_env.get_mut(&repo_id).unwrap();
            assert!(repo_up_env
                .versions
                .iter()
                .any(|v| v.tool == "python" && v.version == *version));

            if repo_up_env.env_vars.get("VIRTUAL_ENV").is_none() {
                repo_up_env.env_vars.insert(
                    "VIRTUAL_ENV".to_string(),
                    self.venv_dir().to_str().unwrap().to_string(),
                );
                repo_up_env.versions.push(UpVersion {
                    tool: "python".to_string(),
                    version: version.to_owned(),
                    dir: "".to_string(),
                });
                cache.up_environments = Some(UpEnvironments {
                    env: up_env.clone(),
                    updated_at: OffsetDateTime::now_utc(),
                });

                true
            } else {
                false
            }
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

    fn venv_dir(&self) -> PathBuf {
        // TODO: it would be better to fix the dynenv's management of `:` appearing in a name
        let repo_id = workdir(".").id().unwrap().replace(":", "--");
        let version = &self.asdf_base().unwrap().version;
        let version_dir = PathBuf::from(&*VENV_PATH).join(version);
        version_dir.join(repo_id)
    }
}

fn with_venv_from_config(value: Option<&ConfigValue>) -> Option<bool> {
    if let Some(value) = value.and_then(|v| v.get("with_venv")) {
        value.as_bool()
    } else {
        None
    }
}

fn version_from_config(config: Option<&ConfigValue>) -> Option<String> {
    config.and_then(|value| {
        value
            .as_str()
            .or(value.as_float().map(|v| v.to_string()))
            .or(value.as_integer().map(|v| v.to_string()))
            .or(value.get("version").and_then(|version| version.as_str()))
    })
}
