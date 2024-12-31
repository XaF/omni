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

use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentsCache {}

impl UpEnvironmentsCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn get_env(&self, workdir_id: &str) -> Option<UpEnvironment> {
        let env: UpEnvironment = CacheManager::get()
            .query_one(
                include_str!("database/sql/up_environments_get_workdir_env.sql"),
                &[&workdir_id],
            )
            .ok()?;
        Some(env)
    }

    pub fn clear(&self, workdir_id: &str) -> Result<bool, CacheManagerError> {
        let mut cleared = false;

        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Close the history entry for the workdir
            tx.execute(
                include_str!("database/sql/up_environments_close_workdir_history.sql"),
                params![&workdir_id],
            )?;

            // Clear the environment for the workdir
            tx.execute(
                include_str!("database/sql/up_environments_clear_workdir_env.sql"),
                params![&workdir_id],
            )?;

            // Check if the row was cleared
            cleared = tx.changes() == 1;

            Ok(())
        })?;

        Ok(cleared)
    }

    pub fn assign_environment(
        &self,
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
            new_env = match tx.query_one::<bool>(
                include_str!("database/sql/up_environments_check_env_version_exists.sql"),
                params![&env_version_id],
            ) {
                Ok(found) => !found,
                Err(CacheManagerError::SqlError(rusqlite::Error::QueryReturnedNoRows)) => true,
                Err(err) => return Err(err),
            };

            if new_env {
                // Insert the environment version
                tx.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![
                        &env_version_id,
                        serde_json::to_string(&environment.versions)?,
                        serde_json::to_string(&environment.paths)?,
                        serde_json::to_string(&environment.env_vars)?,
                        serde_json::to_string(&environment.config_modtimes)?,
                        environment.config_hash,
                    ],
                )?;
            }

            // Check if this is a new active environment for the work directory
            let replace_env: bool = match tx.query_one::<String>(
                include_str!("database/sql/up_environments_get_workdir_env.sql"),
                params![&workdir_id],
            ) {
                Ok(current_env_version_id) => current_env_version_id != env_version_id,
                Err(CacheManagerError::SqlError(rusqlite::Error::QueryReturnedNoRows)) => true,
                Err(err) => return Err(err),
            };

            if replace_env {
                // Assign the environment to the workdir
                tx.execute(
                    include_str!("database/sql/up_environments_set_workdir_env.sql"),
                    params![&workdir_id, &env_version_id],
                )?;
            }

            // Check if the currently active open entry is for a different
            // env_version_id or head_sha, in which case we can close the current
            // entry and open a new one
            let replace_history: bool = match tx.query_one::<(String, Option<String>)>(
                include_str!("database/sql/up_environments_get_workdir_history_open.sql"),
                params![&workdir_id],
            ) {
                Ok((current_env_version_id, current_head_sha)) => {
                    current_env_version_id != env_version_id || current_head_sha != head_sha
                }
                Err(CacheManagerError::SqlError(rusqlite::Error::QueryReturnedNoRows)) => true,
                Err(err) => return Err(err),
            };

            if replace_history {
                // Close any open history entry for the workdir
                tx.execute(
                    include_str!("database/sql/up_environments_close_workdir_history.sql"),
                    params![&workdir_id],
                )?;

                // Add an open history entry for the workdir
                tx.execute(
                    include_str!("database/sql/up_environments_add_workdir_history.sql"),
                    params![&workdir_id, &env_version_id, &head_sha],
                )?;
            }

            // Cleanup history
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_duplicate_opens.sql"),
                [],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_retention.sql"),
                params![&cache_env_config.retention],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_max_per_workdir.sql"),
                params![&cache_env_config.max_per_workdir],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_max_total.sql"),
                params![&cache_env_config.max_total],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_delete_orphaned_env.sql"),
                [],
            )?;

            Ok(())
        })?;

        Ok((new_env, env_version_id))
    }

    #[cfg(test)]
    pub fn environment_ids(&self) -> BTreeSet<String> {
        let environment_ids: Vec<String> = CacheManager::get()
            .query_as(
                include_str!("database/sql/up_environments_get_env_ids.sql"),
                &[],
            )
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

        let versions: Vec<UpVersion> = match serde_json::from_str(&versions_json) {
            Ok(versions) => versions,
            Err(_err) => {
                let old_versions: Vec<OldUpVersion> = serde_json::from_str(&versions_json)?;
                old_versions.iter().map(|v| v.to_owned().into()).collect()
            }
        };

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
        plugin_name: &str,
        normalized_name: &str,
        version: &str,
        bin_path: &str,
        dirs: BTreeSet<String>,
    ) -> bool {
        let mut dirs = dirs;

        for exists in self.versions.iter() {
            if exists.normalized_name == normalized_name && exists.version == version {
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
            self.versions.push(UpVersion::new(
                tool,
                plugin_name,
                normalized_name,
                version,
                bin_path,
                &dir,
            ));
        }

        true
    }

    pub fn add_version_data_path(
        &mut self,
        normalized_name: &str,
        version: &str,
        dir: &str,
        data_path: &str,
    ) -> bool {
        for exists in self.versions.iter_mut() {
            if exists.normalized_name == normalized_name
                && exists.version == version
                && exists.dir == dir
            {
                exists.data_path = Some(data_path.to_string());
                return true;
            }
        }

        false
    }
}

// TODO: deprecated, remove after leaving time to migrate to the new UpVersion
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OldUpVersion {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_real_name: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct UpVersion {
    pub tool: String,
    pub plugin_name: String,
    pub normalized_name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bin_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
}

impl From<OldUpVersion> for UpVersion {
    fn from(args: OldUpVersion) -> Self {
        Self {
            tool: args.tool_real_name.unwrap_or(args.tool.clone()),
            plugin_name: args.tool.clone(),
            normalized_name: args.tool,
            version: args.version,
            bin_path: "bin".to_string(),
            dir: args.dir,
            data_path: args.data_path,
        }
    }
}

impl UpVersion {
    pub fn new(
        tool: &str,
        plugin_name: &str,
        normalized_name: &str,
        version: &str,
        bin_path: &str,
        dir: &str,
    ) -> Self {
        Self {
            tool: tool.to_string(),
            plugin_name: plugin_name.to_string(),
            normalized_name: normalized_name.to_string(),
            version: version.to_string(),
            bin_path: bin_path.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::internal::testutils::run_with_env;
    use crate::internal::ConfigLoader;
    use crate::internal::ConfigValue;

    mod up_environments_cache {
        use super::*;

        #[test]
        fn test_get_and_assign_environment() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // Initially no environment exists
                assert!(cache.get_env(workdir_id).is_none());

                // Assign environment
                let (is_new, _env_id) = cache
                    .assign_environment(workdir_id, Some("test-sha".to_string()), &mut env)
                    .expect("Failed to assign environment");
                assert!(is_new);

                // Get environment and verify
                let retrieved = cache
                    .get_env(workdir_id)
                    .expect("Failed to get environment");
                assert_eq!(retrieved.config_hash, env.config_hash);
            });
        }

        #[test]
        fn test_assign_already_existing_environment() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // Assign environment
                let (is_new, _env_id) = cache
                    .assign_environment(workdir_id, Some("test-sha".to_string()), &mut env)
                    .expect("Failed to assign environment");
                assert!(is_new);

                // Assign environment again
                let (is_new, _env_id) = cache
                    .assign_environment(workdir_id, Some("test-sha".to_string()), &mut env)
                    .expect("Failed to assign environment");
                assert!(!is_new);
            });
        }

        #[test]
        fn test_clear_environment() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // Initially no environment exists
                assert!(cache.get_env(workdir_id).is_none());

                // Assign environment
                let (_is_new, _env_id) = cache
                    .assign_environment(workdir_id, Some("dumb".to_string()), &mut env)
                    .expect("Failed to assign environment");

                // Verify it now has an environment
                assert!(cache.get_env(workdir_id).is_some());

                // Clear environment
                let cleared = cache
                    .clear(workdir_id)
                    .expect("Failed to clear environment");
                assert!(cleared);

                // Verify environment is cleared
                assert!(cache.get_env(workdir_id).is_none());
            });
        }

        #[test]
        fn test_environment_ids() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // Initially no environments
                assert!(cache.environment_ids().is_empty());

                // Assign environment
                cache
                    .assign_environment(workdir_id, None, &mut env)
                    .expect("Failed to assign environment");

                // Verify environment id exists
                let ids = cache.environment_ids();
                assert_eq!(ids.len(), 1);
            });
        }

        #[test]
        fn test_assign_environment_with_different_sha() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // First assignment
                let (is_new, env_id1) = cache
                    .assign_environment(workdir_id, Some("sha1".to_string()), &mut env)
                    .expect("Failed to assign environment");
                assert!(is_new);

                // Same environment, different SHA
                let (is_new, env_id2) = cache
                    .assign_environment(workdir_id, Some("sha2".to_string()), &mut env)
                    .expect("Failed to assign environment");
                assert!(!is_new);
                assert_eq!(env_id1, env_id2);
            });
        }

        #[test]
        fn test_assign_environment_without_sha() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // Assign without SHA
                let (is_new, _) = cache
                    .assign_environment(workdir_id, None, &mut env)
                    .expect("Failed to assign environment");
                assert!(is_new);

                // Verify environment exists
                assert!(cache.get_env(workdir_id).is_some());
            });
        }

        #[test]
        fn test_multiple_workdir_environments() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let mut env = UpEnvironment::new().init();

                // Assign to multiple workdirs
                let workdirs = ["workdir1", "workdir2", "workdir3"];
                for workdir in workdirs {
                    let (is_new, _) = cache
                        .assign_environment(workdir, None, &mut env)
                        .expect("Failed to assign environment");
                    assert!(is_new);
                }

                // Verify each workdir has environment
                for workdir in workdirs {
                    assert!(cache.get_env(workdir).is_some());
                }

                // Verify environment_ids contains all workdirs
                let ids = cache.environment_ids();
                assert_eq!(ids.len(), workdirs.len());
            });
        }

        #[test]
        fn test_clear_nonexistent_environment() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let cleared = cache.clear("nonexistent-workdir").expect("Failed to clear");
                assert!(!cleared);
            });
        }

        #[test]
        fn test_environment_history_cleanup() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // Create multiple history entries
                for sha in &["sha1", "sha2", "sha3"] {
                    cache
                        .assign_environment(workdir_id, Some(sha.to_string()), &mut env)
                        .expect("Failed to assign environment");
                }

                // Clear and verify cleanup
                let cleared = cache.clear(workdir_id).expect("Failed to clear");
                assert!(cleared);
                assert!(cache.get_env(workdir_id).is_none());
            });
        }

        #[test]
        fn test_assign_modified_environment() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();
                let workdir_id = "test-workdir";
                let mut env = UpEnvironment::new().init();

                // Initial assignment
                let (is_new, env_id1) = cache
                    .assign_environment(workdir_id, None, &mut env)
                    .expect("Failed to assign environment");
                assert!(is_new);

                // Modify environment
                env.add_env_var("TEST_VAR", "test_value");
                let (is_new, env_id2) = cache
                    .assign_environment(workdir_id, None, &mut env)
                    .expect("Failed to assign environment");
                assert!(is_new);
                assert_ne!(env_id1, env_id2);

                // Verify modified environment
                let retrieved = cache
                    .get_env(workdir_id)
                    .expect("Failed to get environment");
                assert_eq!(retrieved.env_vars.len(), 1);
                assert_eq!(retrieved.env_vars[0].name, "TEST_VAR");
            });
        }

        #[test]
        fn test_environment_retention_max_total_keep_open() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();

                // Write the max_total to the config file
                let expected_max_total = 5;
                if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                    // Write to cache.environment.max_total, using a yaml string
                    *config_value = ConfigValue::from_str(
                        format!(
                            "cache:\n  environment:\n    max_total: {}",
                            expected_max_total
                        )
                        .as_str(),
                    )
                    .expect("Failed to create config value");

                    true
                }) {
                    panic!("Failed to edit main user config file: {}", err);
                }

                // Check if the config was written correctly
                let max_total = match global_config().cache.environment.max_total {
                    None => panic!("Failed to set max_total (None)"),
                    Some(n) if n != expected_max_total => panic!(
                        "Failed to set max_total (expected {}, got {})",
                        expected_max_total, n
                    ),
                    Some(n) => n,
                };

                // Create environments up to max_total limit
                let mut env = UpEnvironment::new().init();
                for i in 0..(max_total + 3) {
                    let workdir = format!("workdir{}", i);
                    cache
                        .assign_environment(&workdir, None, &mut env)
                        .expect("Failed to assign environment");
                }

                // Verify that we keep the open environments, so none has been removed here
                let ids = cache.environment_ids();
                assert_eq!(ids.len(), max_total + 3);
            });
        }

        #[test]
        fn test_environment_retention_max_total() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();

                // Write the max_total to the config file
                let expected_max_total = 5;
                if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                    // Write to cache.environment.max_total, using a yaml string
                    *config_value = ConfigValue::from_str(
                        format!(
                            "cache:\n  environment:\n    max_total: {}",
                            expected_max_total
                        )
                        .as_str(),
                    )
                    .expect("Failed to create config value");

                    true
                }) {
                    panic!("Failed to edit main user config file: {}", err);
                }

                // Check if the config was written correctly
                let max_total = match global_config().cache.environment.max_total {
                    None => panic!("Failed to set max_total (None)"),
                    Some(n) if n != expected_max_total => panic!(
                        "Failed to set max_total (expected {}, got {})",
                        expected_max_total, n
                    ),
                    Some(n) => n,
                };

                // Create environments up to max_total limit
                for i in 0..(max_total + 3) {
                    let mut env = UpEnvironment::new().init();
                    env.add_env_var("TEST_VAR".to_string(), format!("value{}", i));

                    cache
                        .assign_environment("workdir", None, &mut env)
                        .expect("Failed to assign environment");
                }

                // Verify that we keep only kept max_total environments
                let ids = cache.environment_ids();
                assert_eq!(ids.len(), max_total);
            });
        }

        #[test]
        fn test_environment_retention_max_per_workdir() {
            run_with_env(&[], || {
                let cache = UpEnvironmentsCache::get();

                // Write the max_total to the config file
                let expected_max_per_workdir = 2;
                if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                    // Write to cache.environment.max_total, using a yaml string
                    *config_value = ConfigValue::from_str(
                        format!(
                            "cache:\n  environment:\n    max_per_workdir: {}",
                            expected_max_per_workdir
                        )
                        .as_str(),
                    )
                    .expect("Failed to create config value");

                    true
                }) {
                    panic!("Failed to edit main user config file: {}", err);
                }

                // Check if the config was written correctly
                let max_per_workdir = match global_config().cache.environment.max_per_workdir {
                    None => panic!("Failed to set max_per_workdir (None)"),
                    Some(n) if n != expected_max_per_workdir => panic!(
                        "Failed to set max_per_workdir (expected {}, got {})",
                        expected_max_per_workdir, n
                    ),
                    Some(n) => n,
                };

                // Create environments up to max_total limit
                let num_workdirs = 5;
                for i in 0..num_workdirs {
                    let workdir = format!("workdir{}", i);
                    for j in 0..(max_per_workdir + 3) {
                        let mut env = UpEnvironment::new().init();
                        env.add_env_var("TEST_VAR".to_string(), format!("value.{}.{}", i, j));

                        cache
                            .assign_environment(&workdir, None, &mut env)
                            .expect("Failed to assign environment");
                    }
                }

                let expected_total = num_workdirs * max_per_workdir;
                let ids = cache.environment_ids();
                assert_eq!(ids.len(), expected_total);
            });
        }
    }

    mod up_environment {
        use super::*;

        #[test]
        fn test_new_and_init() {
            let env = UpEnvironment::new().init();
            assert!(env.versions.is_empty());
            assert!(env.paths.is_empty());
            assert!(env.env_vars.is_empty());
            assert!(!env.config_hash.is_empty());
            assert!(!env.config_modtimes.is_empty());
        }

        #[test]
        fn test_versions_for_dir() {
            let mut env = UpEnvironment::new();

            // Add versions for different directories
            env.add_version(
                "tool1",
                "plugin1",
                "plugin-1",
                "1.0.0",
                "bin/path/1",
                BTreeSet::from(["dir1".to_string()]),
            );
            env.add_version(
                "tool2",
                "plugin2",
                "plugin-2",
                "2.0.0",
                "bin/path/2",
                BTreeSet::from(["dir1/subdir".to_string()]),
            );
            env.add_version(
                "tool3",
                "plugin3",
                "plugin-3",
                "3.0.0",
                "bin/path/3",
                BTreeSet::from(["dir2".to_string()]),
            );

            // Test dir1 versions
            let dir1_versions = env.versions_for_dir("dir1");
            assert_eq!(dir1_versions.len(), 1);
            assert_eq!(dir1_versions[0].tool, "tool1");

            // Test dir1/subdir versions
            let subdir_versions = env.versions_for_dir("dir1/subdir");
            assert_eq!(subdir_versions.len(), 2);
            assert_eq!(subdir_versions[0].tool, "tool1");
            assert_eq!(subdir_versions[1].tool, "tool2");

            // Test dir2 versions
            let dir2_versions = env.versions_for_dir("dir2");
            assert_eq!(dir2_versions.len(), 1);
            assert_eq!(dir2_versions[0].tool, "tool3");
        }

        #[test]
        fn test_env_vars() {
            let mut env = UpEnvironment::new();

            // Test adding basic env var
            assert!(env.add_env_var("KEY1", "value1"));
            assert_eq!(env.env_vars.len(), 1);
            assert_eq!(env.env_vars[0].name, "KEY1");
            assert_eq!(env.env_vars[0].value, Some("value1".to_string()));

            // Test adding env var with operation
            assert!(env.add_env_var_operation("KEY2", "value2", EnvOperationEnum::Append));
            assert_eq!(env.env_vars[1].operation, EnvOperationEnum::Append);

            // Test adding raw env vars
            let raw_vars = vec![UpEnvVar {
                name: "KEY3".to_string(),
                value: Some("value3".to_string()),
                operation: EnvOperationEnum::Set,
            }];
            assert!(env.add_raw_env_vars(raw_vars));
            assert_eq!(env.env_vars.len(), 3);
        }

        #[test]
        fn test_paths() {
            let mut env = UpEnvironment::new();
            let data_home_path = PathBuf::from(data_home()).join("test");
            let regular_path = PathBuf::from("/usr/local/bin");

            // Test adding single path
            assert!(env.add_path(regular_path.clone()));
            assert_eq!(env.paths.len(), 1);

            // Test data_home path gets prepended
            assert!(env.add_path(data_home_path.clone()));
            assert_eq!(env.paths[0], data_home_path);

            // Test adding multiple paths
            assert!(env.add_paths(vec![PathBuf::from("/path1"), PathBuf::from("/path2")]));
            assert_eq!(env.paths.len(), 4);
        }

        #[test]
        fn test_version_management() {
            let mut env = UpEnvironment::new();

            // Test adding version
            assert!(env.add_version(
                "tool1",
                "plugin1",
                "plugin-1",
                "1.0.0",
                "bin/path/1",
                BTreeSet::from(["dir1".to_string()])
            ));
            assert_eq!(env.versions.len(), 1);

            // Test adding same version doesn't duplicate
            assert!(!env.add_version(
                "tool1",
                "plugin1",
                "plugin-1",
                "1.0.0",
                "bin/path/1",
                BTreeSet::from(["dir1".to_string()])
            ));
            assert_eq!(env.versions.len(), 1);

            // Test adding data path
            assert!(env.add_version_data_path("plugin-1", "1.0.0", "dir1", "/data/path"));
            assert_eq!(env.versions[0].data_path, Some("/data/path".to_string()));
        }
    }

    mod up_version {
        use super::*;

        #[test]
        fn test_new() {
            let version = UpVersion::new(
                "tool1",
                "plugin1",
                "plugin-1",
                "1.0.0",
                "bin/path/1",
                "dir1",
            );
            assert_eq!(version.tool, "tool1");
            assert_eq!(version.plugin_name, "plugin1");
            assert_eq!(version.normalized_name, "plugin-1");
            assert_eq!(version.version, "1.0.0");
            assert_eq!(version.bin_path, "bin/path/1");
            assert_eq!(version.dir, "dir1");
            assert!(version.data_path.is_none());
        }
    }

    mod up_env_var {
        use super::*;

        #[test]
        fn test_from_env_operation_config() {
            let config = EnvOperationConfig {
                name: "TEST_VAR".to_string(),
                value: Some("test_value".to_string()),
                operation: EnvOperationEnum::Set,
            };

            let env_var: UpEnvVar = config.into();
            assert_eq!(env_var.name, "TEST_VAR");
            assert_eq!(env_var.value, Some("test_value".to_string()));
            assert_eq!(env_var.operation, EnvOperationEnum::Set);
        }

        #[test]
        fn test_from_env_config() {
            let config = EnvConfig {
                operations: vec![
                    EnvOperationConfig {
                        name: "VAR1".to_string(),
                        value: Some("value1".to_string()),
                        operation: EnvOperationEnum::Set,
                    },
                    EnvOperationConfig {
                        name: "VAR2".to_string(),
                        value: Some("value2".to_string()),
                        operation: EnvOperationEnum::Append,
                    },
                ],
            };

            let env_vars: Vec<UpEnvVar> = config.into();
            assert_eq!(env_vars.len(), 2);
            assert_eq!(env_vars[0].name, "VAR1");
            assert_eq!(env_vars[1].name, "VAR2");
        }
    }
}
