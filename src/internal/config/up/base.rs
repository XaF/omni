use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::ConfigValue;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::workdir;
use crate::omni_warning;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfig {
    pub steps: Vec<UpConfigTool>,
    pub errors: Vec<UpError>,
}

impl UpConfig {
    pub fn from_config_value(config_value: Option<ConfigValue>) -> Option<Self> {
        if config_value.is_none() {
            return None;
        }

        let config_value = config_value.unwrap();
        if !config_value.is_array() {
            return None;
        }

        let mut errors = Vec::new();
        let mut steps = Vec::new();
        for (value, index) in config_value.as_array().unwrap().iter().zip(0..) {
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

        if steps.len() == 0 && errors.len() == 0 {
            return None;
        }

        Some(UpConfig {
            steps: steps,
            errors: errors,
        })
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

    pub fn up(&self) -> Result<(), UpError> {
        // Get current directory
        let current_dir = std::env::current_dir().expect("Failed to get current directory");

        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .cloned()
            .collect::<Vec<UpConfigTool>>();

        // Go through the steps
        let num_steps = steps.len() + 1;
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

            // Update the dynamic environment so that if anything has changed
            // the command can consider it right away
            update_dynamic_env_for_command(".");

            if let Err(error) = step.up(Some((idx + 1, num_steps))) {
                return Err(error);
            }
        }

        // This is a special case, as we could have multiple versions of a single
        // tool loaded in the same repo (for some reason...) we need to clean up
        // the unused ones _at the end_ of the process
        UpConfigAsdfBase::cleanup_unused(steps.clone(), Some((num_steps, num_steps)))?;

        Ok(())
    }

    pub fn down(&self) -> Result<(), UpError> {
        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .cloned()
            .collect::<Vec<UpConfigTool>>();

        // Go through the steps, in reverse
        let num_steps = steps.len();
        for (idx, step) in steps.iter().rev().enumerate() {
            // Update the dynamic environment so that if anything has changed
            // the command can consider it right away
            update_dynamic_env_for_command(".");

            if let Err(error) = step.down(Some((idx + 1, num_steps))) {
                return Err(error);
            }
        }

        UpConfigAsdfBase::cleanup_unused(Vec::new(), Some((num_steps, num_steps)))?;

        Ok(())
    }
}
