use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;

use rusqlite::params;
use rusqlite::Row;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config;
use crate::internal::config::global_config;
use crate::internal::config::parser::EnvConfig;
use crate::internal::config::parser::EnvOperationConfig;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::config::up::utils::get_config_mod_times;
use crate::internal::env::data_home;

use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::cache::FromRow;
use crate::internal::cache::RowExt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentsCache {}

impl UpEnvironmentsCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn get_env(&self, workdir_id: &str) -> Option<UpEnvironment> {
        let env: UpEnvironment = CacheManager::get()
            .query_one(
                include_str!("sql/up_environments_get_workdir_env.sql"),
                &[&workdir_id],
            )
            .ok()?;
        Some(env)
    }

    pub fn clear(&mut self, workdir_id: &str) -> Result<bool, CacheManagerError> {
        let mut cleared = false;

        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Close the history entry for the workdir
            tx.execute(
                include_str!("sql/up_environments_close_workdir_history.sql"),
                params![&workdir_id],
            )?;

            // Clear the environment for the workdir
            tx.execute(
                include_str!("sql/up_environments_clear_workdir_env.sql"),
                params![&workdir_id],
            )?;

            // Check if the row was cleared
            cleared = tx.changes() == 1;

            Ok(())
        })?;

        Ok(cleared)
    }

    pub fn assign_environment(
        &mut self,
        workdir_id: &str,
        head_sha: Option<String>,
        environment: &mut UpEnvironment,
    ) -> Result<(bool, String), CacheManagerError> {
        let mut new_env = true;
        let env_hash = environment.hash_string();
        let env_version_id = format!("{}%{}", workdir_id, env_hash);
        let cache_env_config = global_config().cache.environment;

        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Check if the environment with the given id already exists
            let existing_env: Option<String> = tx.query_one(
                include_str!("sql/up_environments_get_workdir_env.sql"),
                params![&workdir_id],
            )?;
            new_env = existing_env.is_none();

            // Insert the environment version
            tx.execute(
                include_str!("sql/up_environments_insert_env_version.sql"),
                params![
                    &env_version_id,
                    serde_json::to_string(&environment.versions)?,
                    serde_json::to_string(&environment.paths)?,
                    serde_json::to_string(&environment.env_vars)?,
                    serde_json::to_string(&environment.config_modtimes)?,
                    environment.config_hash,
                ],
            )?;

            // Assign the environment to the workdir
            tx.execute(
                include_str!("sql/up_environments_set_workdir_env.sql"),
                params![&workdir_id, &env_version_id],
            )?;

            // Close any open history entry for the workdir
            tx.execute(
                include_str!("sql/up_environments_close_workdir_history.sql"),
                params![&workdir_id],
            )?;

            // Add an open history entry for the workdir
            tx.execute(
                include_str!("sql/up_environments_add_workdir_history.sql"),
                params![&workdir_id, &env_version_id, &head_sha],
            )?;

            // Cleanup history
            tx.execute(
                include_str!("sql/up_environments_cleanup_history_duplicate_opens.sql"),
                [],
            )?;
            tx.execute(
                include_str!("sql/up_environments_cleanup_history_retention.sql"),
                params![&cache_env_config.retention],
            )?;
            tx.execute(
                include_str!("sql/up_environments_cleanup_history_max_per_workdir.sql"),
                params![&cache_env_config.max_per_workdir],
            )?;
            tx.execute(
                include_str!("sql/up_environments_cleanup_history_max_total.sql"),
                params![&cache_env_config.max_total],
            )?;
            tx.execute(
                include_str!("sql/up_environments_delete_orphaned_env.sql"),
                [],
            )?;

            Ok(())
        })?;

        Ok((new_env, env_version_id))
    }

    pub fn environment_ids(&self) -> BTreeSet<String> {
        let environment_ids: Vec<String> = CacheManager::get()
            .query_as(include_str!("sql/up_environments_get_env_ids.sql"), &[])
            .unwrap();
        environment_ids.into_iter().collect()
    }
}

/// The environment configuration for a work directory
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironment {
    /// The versions of the tools to be loaded in the environment
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<UpVersion>,
    /// The paths to add to the PATH environment variable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<PathBuf>,
    /// The environment variables to set
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<UpEnvVar>,
    /// The modification times of the configuration files
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config_modtimes: BTreeMap<String, u64>,
    /// The hash of the configuration files
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub config_hash: String,
}

impl Hash for UpEnvironment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.versions.hash(state);
        self.paths.hash(state);
        self.env_vars.hash(state);
        self.config_modtimes.hash(state);
        self.config_hash.hash(state);
    }
}

impl FromRow for UpEnvironment {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        let versions_json: String = row.get(0)?;
        let versions: Vec<UpVersion> = serde_json::from_str(&versions_json)?;

        let paths_json: String = row.get(1)?;
        let paths: Vec<PathBuf> = serde_json::from_str(&paths_json)?;

        let env_vars_json: String = row.get(2)?;
        let env_vars: Vec<UpEnvVar> = serde_json::from_str(&env_vars_json)?;

        let config_modtimes_json: String = row.get(3)?;
        let config_modtimes: BTreeMap<String, u64> = serde_json::from_str(&config_modtimes_json)?;

        let config_hash: String = row.get(4)?;

        Ok(Self {
            versions,
            paths,
            env_vars,
            config_modtimes,
            config_hash,
        })
    }
}

impl UpEnvironment {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            paths: Vec::new(),
            env_vars: Vec::new(),
            config_modtimes: BTreeMap::new(),
            config_hash: String::new(),
        }
    }

    pub fn init(mut self) -> Self {
        self.config_hash = config(".").up_hash();
        self.config_modtimes = get_config_mod_times(".");
        self
    }

    pub fn hash_string(&self) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    pub fn versions_for_dir(&self, dir: &str) -> Vec<UpVersion> {
        let mut versions: BTreeMap<String, UpVersion> = BTreeMap::new();

        for version in self.versions.iter() {
            // Check if that version applies to the requested dir
            if !version.dir.is_empty()
                && dir != version.dir
                && !dir.starts_with(format!("{}/", version.dir).as_str())
            {
                continue;
            }

            // If there is already a version, check if the current one's dir is more specific
            if let Some(existing_version) = versions.get(&version.tool) {
                if existing_version.dir.len() > version.dir.len() {
                    continue;
                }
            }

            versions.insert(version.tool.clone(), version.clone());
        }

        versions.values().cloned().collect()
    }

    pub fn add_env_var<T>(&mut self, key: T, value: T) -> bool
    where
        T: AsRef<str>,
    {
        self.add_env_var_operation(key, value, EnvOperationEnum::Set)
    }

    pub fn add_env_var_operation<T>(
        &mut self,
        key: T,
        value: T,
        operation: EnvOperationEnum,
    ) -> bool
    where
        T: AsRef<str>,
    {
        let up_env_var = UpEnvVar {
            name: key.as_ref().to_string(),
            value: Some(value.as_ref().to_string()),
            operation,
        };

        self.env_vars.push(up_env_var);

        true
    }

    pub fn add_raw_env_vars(&mut self, env_vars: Vec<UpEnvVar>) -> bool {
        self.env_vars.extend(env_vars);
        true
    }

    pub fn add_path(&mut self, path: PathBuf) -> bool {
        self.paths.retain(|p| p != &path);

        // Prepend anything that starts with the data_home()
        if path.starts_with(data_home()) {
            self.paths.insert(0, path);
        } else {
            self.paths.push(path);
        }

        true
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) -> bool {
        for path in paths {
            self.add_path(path);
        }
        true
    }

    pub fn add_version(
        &mut self,
        tool: &str,
        tool_real_name: Option<&str>,
        version: &str,
        dirs: BTreeSet<String>,
    ) -> bool {
        let mut dirs = dirs;

        for exists in self.versions.iter() {
            if exists.tool == tool && exists.version == version {
                dirs.remove(&exists.dir);
                if dirs.is_empty() {
                    break;
                }
            }
        }

        if dirs.is_empty() {
            return false;
        }

        for dir in dirs {
            self.versions
                .push(UpVersion::new(tool, tool_real_name, version, &dir));
        }

        true
    }

    pub fn add_version_data_path(
        &mut self,
        tool: &str,
        version: &str,
        dir: &str,
        data_path: &str,
    ) -> bool {
        for exists in self.versions.iter_mut() {
            if exists.tool == tool && exists.version == version && exists.dir == dir {
                exists.data_path = Some(data_path.to_string());
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct UpVersion {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_real_name: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
}

impl UpVersion {
    pub fn new(tool: &str, tool_real_name: Option<&str>, version: &str, dir: &str) -> Self {
        Self {
            tool: tool.to_string(),
            tool_real_name: tool_real_name.map(|s| s.to_string()),
            version: version.to_string(),
            dir: dir.to_string(),
            data_path: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct UpEnvVar {
    #[serde(
        rename = "n",
        alias = "name",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(
        rename = "v",
        alias = "value",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<String>,
    #[serde(
        rename = "o",
        alias = "operation",
        default,
        skip_serializing_if = "EnvOperationEnum::is_default"
    )]
    pub operation: EnvOperationEnum,
}

impl From<EnvOperationConfig> for UpEnvVar {
    fn from(env_op: EnvOperationConfig) -> Self {
        Self {
            name: env_op.name,
            value: env_op.value,
            operation: env_op.operation,
        }
    }
}

impl From<EnvConfig> for Vec<UpEnvVar> {
    fn from(env_config: EnvConfig) -> Self {
        env_config
            .operations
            .into_iter()
            .map(|operation| operation.into())
            .collect()
    }
}
