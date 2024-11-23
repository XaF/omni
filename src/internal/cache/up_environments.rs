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

#[cfg(test)]
mod tests {
    use super::*;

    mod up_environments_cache {
        use super::*;

        #[test]
        fn test_environment_management() {
            let mut cache = UpEnvironmentsCache::new_empty();
            let workdir_id = "test_workdir";
            let mut env = UpEnvironment::new();
            env.add_version(
                "python",
                Some("python"),
                "3.9.0",
                BTreeSet::from(["".to_string()]),
            );

            // Test environment assignment
            let (is_new, version_id) =
                cache.assign_environment(workdir_id, Some("abc123".to_string()), &mut env);
            assert!(is_new);
            assert!(cache.contains(workdir_id));

            // Test getting environment
            let stored_env = cache.get_env(workdir_id);
            assert!(stored_env.is_some());
            assert_eq!(stored_env.unwrap().versions.len(), 1);

            // Test getting environment by version
            let version_env = cache.get_env_version(&version_id);
            assert!(version_env.is_some());
            assert_eq!(version_env.unwrap().versions.len(), 1);

            // Test clearing environment
            assert!(cache.clear(workdir_id));
            assert!(!cache.contains(workdir_id));
        }

        #[test]
        fn test_cleanup() {
            let mut cache = UpEnvironmentsCache::new_empty();
            let workdir_id = "test_workdir";
            let mut env = UpEnvironment::new();

            // Add multiple environments
            for i in 0..3 {
                env.add_version(
                    "python",
                    Some("python"),
                    &format!("3.{}.0", i),
                    BTreeSet::from(["".to_string()]),
                );
                let (_, _) =
                    cache.assign_environment(workdir_id, Some(format!("sha{}", i)), &mut env);
            }

            let initial_count = cache.versioned_env.len();
            cache.cleanup();
            // Cleanup should maintain active environments
            assert!(cache.versioned_env.len() <= initial_count);
        }
    }

    mod up_environment_history {
        use super::*;

        #[test]
        fn test_add_first_entry() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };

            assert!(history.add("work1", Some("sha1".to_string()), "env1"));
            assert_eq!(history.entries.len(), 1);
            assert!(history.entries[0].is_open());
            assert_eq!(history.entries[0].workdir_id, "work1");
            assert_eq!(history.entries[0].head_sha, Some("sha1".to_string()));
            assert_eq!(history.entries[0].env_version_id, "env1");
        }

        #[test]
        fn test_add_same_environment() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };

            assert!(history.add("work1", Some("sha1".to_string()), "env1"));
            assert!(!history.add("work1", Some("sha1".to_string()), "env1")); // Should return false for same environment
            assert_eq!(history.entries.len(), 1);
            assert!(history.entries[0].is_open());
        }

        #[test]
        fn test_add_different_environment() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };

            assert!(history.add("work1", Some("sha1".to_string()), "env1"));
            assert!(history.add("work1", Some("sha2".to_string()), "env2"));

            assert_eq!(history.entries.len(), 2);
            assert!(history.entries[0].is_open());
            assert!(!history.entries[1].is_open());

            assert_eq!(history.entries[0].env_version_id, "env2");
            assert_eq!(history.entries[1].env_version_id, "env1");
        }

        #[test]
        fn test_add_multiple_workdirs() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };

            assert!(history.add("work1", Some("sha1".to_string()), "env1"));
            assert!(history.add("work2", Some("sha2".to_string()), "env2"));

            assert_eq!(history.entries.len(), 2);
            // Both entries should be open since they're for different workdirs
            assert!(history.entries.iter().all(|e| e.is_open()));
        }

        #[test]
        fn test_cleanup_single_open_per_workdir() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };

            // Add multiple open entries for same workdir
            history.add("work1", Some("sha1".to_string()), "env1");
            // Manually add another open entry for the same workdir
            history.entries.push(UpEnvironmentHistoryEntry::new(
                "work1",
                Some("sha2".to_string()),
                "env2",
            ));

            history.cleanup(None, None, None);

            // Should only have one open entry per workdir
            assert_eq!(history.entries.iter().filter(|e| e.is_open()).count(), 1);
        }

        #[test]
        fn test_cleanup_max_per_workdir() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };

            // Add multiple entries for same workdir
            history.add("work1", Some("sha1".to_string()), "env1");
            history.add("work1", Some("sha2".to_string()), "env2");
            history.add("work1", Some("sha3".to_string()), "env3");

            history.cleanup(None, Some(2), None);

            assert_eq!(history.entries.len(), 2);
            // Should keep the most recent entries
            assert!(history.entries[0].env_version_id == "env3");
        }

        #[test]
        fn test_cleanup_retention() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };
            let now = omni_now();

            // Add an old entry
            let mut old_entry =
                UpEnvironmentHistoryEntry::new("work1", Some("sha1".to_string()), "env1");
            old_entry.used_from_date = now - Duration::hours(5);
            old_entry.used_until_date = Some(now - Duration::hours(4));
            history.entries.push(old_entry);

            // Add a recent entry
            history.add("work1", Some("sha2".to_string()), "env2");

            history.cleanup(Some(Duration::hours(3)), None, None);

            assert_eq!(history.entries.len(), 1);
            assert_eq!(history.entries[0].env_version_id, "env2");
        }

        #[test]
        fn test_cleanup_max_total() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };

            // Add entries for different workdirs
            history.add("work1", Some("sha1.1".to_string()), "env1.1");
            history.add("work1", Some("sha1.2".to_string()), "env1.2");
            history.add("work2", Some("sha2.1".to_string()), "env2.1");
            history.add("work2", Some("sha2.2".to_string()), "env2.2");
            history.add("work3", Some("sha3.1".to_string()), "env3.1");
            history.add("work3", Some("sha3.2".to_string()), "env3.2");

            history.cleanup(None, None, Some(5));

            assert_eq!(history.entries.len(), 5);
            // Should keep open entries
            assert!(history.entries.iter().any(|e| e.env_version_id == "env3.2"));
            assert!(history.entries.iter().any(|e| e.env_version_id == "env2.2"));
            assert!(history.entries.iter().any(|e| e.env_version_id == "env1.2"));
            // And should keep the most recent closed entries
            assert!(history.entries.iter().any(|e| e.env_version_id == "env3.1"));
            assert!(history.entries.iter().any(|e| e.env_version_id == "env2.1"));

            // Now try to cleanup with a max of 2
            history.cleanup(None, None, Some(2));

            assert_eq!(history.entries.len(), 3);
            // Should still keep open entries
            assert!(history.entries.iter().any(|e| e.env_version_id == "env3.2"));
            assert!(history.entries.iter().any(|e| e.env_version_id == "env2.2"));
            assert!(history.entries.iter().any(|e| e.env_version_id == "env1.2"));
        }

        #[test]
        fn test_cleanup_sorting() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };
            let now = omni_now();

            // Add entries in mixed order with same close dates
            let mut entry1 =
                UpEnvironmentHistoryEntry::new("work1", Some("sha1".to_string()), "env1");
            entry1.used_from_date = now - Duration::hours(3);
            entry1.used_until_date = Some(now - Duration::hours(1));

            let mut entry2 =
                UpEnvironmentHistoryEntry::new("work1", Some("sha2".to_string()), "env2");
            entry2.used_from_date = now - Duration::hours(2);
            entry2.used_until_date = Some(now - Duration::hours(1));

            let entry3 = UpEnvironmentHistoryEntry::new("work2", Some("sha3".to_string()), "env3");

            history.entries.extend([entry1, entry2, entry3]);

            history.cleanup(None, None, None);

            // Check sorting:
            // 1. Open entries first
            assert!(history.entries[0].is_open());
            // 2. For same close dates, sort by start date
            assert!(history.entries[1].used_from_date > history.entries[2].used_from_date);
        }

        #[test]
        fn test_multiple_constraints() {
            let mut history = UpEnvironmentHistory {
                entries: Vec::new(),
            };
            let now = omni_now();

            // Add multiple entries with various dates and states
            for i in 0..10 {
                let mut entry = UpEnvironmentHistoryEntry::new(
                    &format!("work{}", i % 2),
                    Some(format!("sha{}", i)),
                    &format!("env{}", i),
                );
                if i < 4 {
                    entry.used_from_date = now - Duration::hours(5 - i as i64);
                    entry.used_until_date = Some(now - Duration::hours(4 - i as i64));
                }
                history.entries.push(entry);
            }

            // Apply multiple constraints
            history.cleanup(
                Some(Duration::hours(2)), // retain only last 2 hours
                Some(2),                  // max 2 per workdir
                Some(3),                  // max 3 total
            );

            assert!(history.entries.len() <= 3);
            // Should keep open entries
            assert!(history.entries.iter().any(|e| e.is_open()));
            // Should respect retention
            assert!(history
                .entries
                .iter()
                .all(|e| e.is_open() || e.used_until_date.unwrap() > (now - Duration::hours(2))));
        }
    }
}
