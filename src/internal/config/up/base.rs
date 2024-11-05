use itertools::Itertools;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::reshim;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpConfigGithubReleases;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::workdir;
use crate::omni_warning;

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfig {
    pub steps: Vec<UpConfigTool>,
    pub errors: Vec<UpError>,
}

impl Empty for UpConfig {
    fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl Serialize for UpConfig {
    // Serialization of UpConfig is serialization of the steps
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.steps.serialize(serializer)
    }
}

impl UpConfig {
    pub fn from_config_value(config_value: Option<ConfigValue>) -> Option<Self> {
        config_value.as_ref()?;

        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return None,
        };

        let config_array = match config_value.as_array() {
            Some(config_array) => config_array,
            None => return None,
        };

        let mut errors = Vec::new();
        let mut steps = Vec::new();
        for (value, index) in config_array.iter().zip(0..) {
            if value.is_str() {
                let up_name = value.as_str().unwrap();
                if let Some(up_config) = UpConfigTool::from_config_value(&up_name, None) {
                    steps.push(up_config);
                } else {
                    errors.push(UpError::Config(format!(
                        "invalid config for step {} ({})",
                        index + 1,
                        up_name
                    )));
                }
            } else if value.is_table() {
                let table = value.as_table().unwrap();
                if table.len() != 1 {
                    errors.push(UpError::Config(format!(
                        "invalid config for step {}: {}",
                        index + 1,
                        value
                    )));
                    continue;
                }

                let (up_name, config_value) = table.iter().next().unwrap();
                if let Some(up_config) =
                    UpConfigTool::from_config_value(up_name, Some(config_value))
                {
                    steps.push(up_config);
                } else {
                    errors.push(UpError::Config(format!(
                        "invalid config for step {} ({}): {}",
                        index + 1,
                        up_name,
                        config_value
                    )));
                }
            } else {
                errors.push(UpError::Config(format!(
                    "invalid config for step {}: {}",
                    index + 1,
                    value
                )));
            }
        }

        if steps.is_empty() && errors.is_empty() {
            return None;
        }

        Some(UpConfig { steps, errors })
    }

    // pub fn steps(&self) -> Vec<UpConfigTool> {
    // self.steps.clone()
    // }

    pub fn errors(&self) -> Vec<UpError> {
        self.errors.clone()
    }

    pub fn has_steps(&self) -> bool {
        !self.steps.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn clear_cache() {
        let workdir = workdir(".");
        if let Some(repo_id) = workdir.id() {
            if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| up_env.clear(&repo_id)) {
                omni_warning!(format!("failed to update cache: {}", err));
            }
        }
    }

    pub fn up(&self, options: &UpOptions, environment: &mut UpEnvironment) -> Result<(), UpError> {
        // Get current directory
        let current_dir = std::env::current_dir().expect("Failed to get current directory");

        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .collect::<Vec<&UpConfigTool>>();

        // Go through the steps
        let num_steps = steps.len() + 2;
        for (idx, step) in steps.iter().enumerate() {
            // Make sure that we're in the right directory
            let step_dir = current_dir.join(step.dir().unwrap_or("".to_string()));
            if let Err(error) = std::env::set_current_dir(&step_dir) {
                return Err(UpError::Exec(format!(
                    "failed to change directory to {}: {}",
                    step_dir.display(),
                    error
                )));
            }

            let mut progress_handler = UpProgressHandler::new(Some((idx + 1, num_steps)));
            if let Some(sync_file) = &options.lock_file {
                progress_handler.set_sync_file(sync_file);
            }

            step.up(options, environment, &progress_handler)?
        }

        // Save and assign the environment
        self.assign_environment(environment, Some((num_steps - 1, num_steps)), options)?;

        // Cleanup anything that's not needed
        self.cleanup(Some((num_steps, num_steps)), options)?;

        Ok(())
    }

    fn assign_environment(
        &self,
        environment: &mut UpEnvironment,
        progress: Option<(usize, usize)>,
        options: &UpOptions,
    ) -> Result<(), UpError> {
        let mut progress_handler = UpProgressHandler::new(progress);
        if let Some(sync_file) = &options.lock_file {
            progress_handler.set_sync_file(sync_file);
        }
        progress_handler.init("apply environment:".light_blue());

        let workdir = workdir(".");
        let workdir_id = match workdir.id() {
            Some(workdir_id) => workdir_id,
            None => {
                let err = "failed to get workdir id".to_string();
                progress_handler.error_with_message(err.clone());
                return Err(UpError::Exec(err));
            }
        };

        // Assign the version id to the workdir now that we have successfully set it up
        progress_handler.progress("associating workdir to environment".to_string());
        let mut new_env = true;
        let mut assigned_environment = "".to_string();
        if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| {
            (new_env, assigned_environment) =
                up_env.assign_environment(&workdir_id, options.commit_sha.clone(), environment);
            true
        }) {
            progress_handler.error_with_message(format!("failed to update cache: {}", err));
            return Err(UpError::Cache(err.to_string()));
        }

        if assigned_environment.is_empty() {
            progress_handler.error_with_message("failed to assign environment".to_string());
            return Err(UpError::Cache("failed to assign environment".to_string()));
        }

        // Go over the up configuration again, but this time to set the dependencies
        // as required by the `assigned_environment`
        if new_env {
            progress_handler.progress("committing environment dependencies".to_string());
            if let Err(err) = self.commit(&options, &assigned_environment) {
                progress_handler.error_with_message(format!(
                    "failed to commit environment dependencies: {}",
                    err
                ));
                return Err(UpError::Cache(err.to_string()));
            }
        }

        progress_handler.success_with_message("done".light_green());

        Ok(())
    }

    fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .collect::<Vec<&UpConfigTool>>();

        // Go through the steps
        let num_steps = steps.len() + 1;
        for (idx, step) in steps.iter().enumerate() {
            let mut progress_handler = UpProgressHandler::new(Some((idx + 1, num_steps)));
            if let Some(sync_file) = &options.lock_file {
                progress_handler.set_sync_file(sync_file);
            }

            step.commit(options, env_version_id)?
        }

        Ok(())
    }

    pub fn down(&self, options: &UpOptions) -> Result<(), UpError> {
        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .collect::<Vec<&UpConfigTool>>();

        // Go through the steps, in reverse
        let num_steps = steps.len();
        for (idx, step) in steps.iter().rev().enumerate() {
            // Update the dynamic environment so that if anything has changed
            // the command can consider it right away
            update_dynamic_env_for_command(".");

            let mut progress_handler = UpProgressHandler::new(Some((idx + 1, num_steps)));
            if let Some(sync_file) = &options.lock_file {
                progress_handler.set_sync_file(sync_file);
            }

            step.down(&progress_handler)?
        }

        // Cleanup anything that's not needed
        self.cleanup(Some((num_steps, num_steps)), options)?;

        Ok(())
    }

    /// Cleanup anything that's not needed anymore; this will call the cleanup
    /// method of every existing tool, so that it can cleanup dependencies from
    /// steps that do not exist anymore on top of previous versions of recently
    /// upgraded tools.
    pub fn cleanup(
        &self,
        progress: Option<(usize, usize)>,
        options: &UpOptions,
    ) -> Result<(), UpError> {
        let mut progress_handler = UpProgressHandler::new(progress);
        if let Some(sync_file) = &options.lock_file {
            progress_handler.set_sync_file(sync_file);
        }
        progress_handler.init("resources cleanup:".light_blue());

        let mut cleanups = vec![];

        // Call cleanup on the different operation types
        if let Some(cleanup) = UpConfigAsdfBase::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }
        if let Some(cleanup) = UpConfigHomebrew::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }
        if let Some(cleanup) = UpConfigGithubReleases::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }

        // Then cleanup the data path
        if let Some(cleanup) = self.cleanup_data_path(&progress_handler)? {
            cleanups.push(cleanup);
        }

        // Then regenerate the shims
        if let Some(reshim) = reshim(&progress_handler)? {
            cleanups.push(reshim);
        }

        if cleanups.is_empty() {
            progress_handler.success_with_message("nothing to do".light_black());
        } else {
            progress_handler.success_with_message(cleanups.join(", "));
        }

        Ok(())
    }

    pub fn cleanup_data_path(
        &self,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<Option<String>, UpError> {
        let wd = workdir(".");
        let wd_data_path = match wd.data_path() {
            Some(data_path) => data_path,
            None => return Ok(None),
        };

        // If the workdir data path does not exist, we're done
        if !wd_data_path.exists() {
            return Ok(None);
        }

        let expected_data_paths = self
            .steps
            .iter()
            .filter(|step| step.is_available() && step.was_upped())
            .flat_map(|step| step.data_paths())
            .filter(|data_path| data_path.starts_with(wd_data_path))
            .sorted()
            .dedup()
            .collect::<Vec<_>>();

        let (root_removed, num_removed, _) =
            cleanup_path(wd_data_path, expected_data_paths, progress_handler, true)?;

        if root_removed {
            return Ok(Some(format!(
                "removed workdir data path {}",
                wd_data_path.display().to_string().light_yellow()
            )));
        }

        if num_removed == 0 {
            return Ok(None);
        }

        Ok(Some(format!(
            "removed {} entr{} from the data path",
            num_removed.to_string().light_yellow(),
            if num_removed > 1 { "ies" } else { "y" }
        )))
    }
}
