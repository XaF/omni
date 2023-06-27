use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::UpEnvironments;
use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::ConfigValue;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::git_env;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::Cache;
use crate::omni_warning;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfig {
    pub steps: Vec<UpConfigTool>,
    pub errors: Vec<UpError>,
}

impl UpConfig {
    pub fn from_config_value(config_value: &ConfigValue) -> Option<Self> {
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

    pub fn steps(&self) -> Vec<UpConfigTool> {
        self.steps.clone()
    }

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
        if let Err(err) = Cache::exclusive(|cache| {
            let git_env = git_env(".");
            let repo_id = git_env.id();
            if repo_id.is_none() {
                return false;
            }
            let repo_id = repo_id.unwrap();

            // Update the repository up cache
            let mut up_env = if let Some(up_cache) = &cache.up_environments {
                up_cache.env.clone()
            } else {
                return false;
            };

            if !up_env.contains_key(&repo_id) {
                return false;
            }

            up_env.remove(&repo_id);
            cache.up_environments = Some(UpEnvironments {
                env: up_env.clone(),
                updated_at: OffsetDateTime::now_utc(),
            });

            true
        }) {
            omni_warning!(format!("failed to update cache: {}", err));
        }
    }

    pub fn up(&self) -> Result<(), UpError> {
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
