use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;
use std::io;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use time::Duration;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_up_environments_cache;
use crate::internal::cache::loaders::set_up_environments_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;
use crate::internal::config;
use crate::internal::config::global_config;
use crate::internal::config::parser::EnvConfig;
use crate::internal::config::parser::EnvOperationConfig;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::config::up::utils::get_config_mod_times;
use crate::internal::env::data_home;
use crate::internal::env::now as omni_now;

const UP_ENVIRONMENTS_CACHE_NAME: &str = "up_environments";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentsCache {
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub workdir_env: BTreeMap<String, String>,
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub versioned_env: BTreeMap<String, UpEnvironment>,
    #[serde(default, skip_serializing_if = "UpEnvironmentHistory::is_empty")]
    pub history: UpEnvironmentHistory,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl UpEnvironmentsCache {
    pub fn generate_version_id(workdir_id: &str) -> String {
        let uuid = uuid::Uuid::new_v4();
        let short_uuid = uuid.to_string()[..8].to_string();
        format!("{}%{}", workdir_id, short_uuid)
    }

    fn updated(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn contains(&self, workdir_id: &str) -> bool {
        self.workdir_env.contains_key(workdir_id)
    }

    pub fn get_env(&self, workdir_id: &str) -> Option<&UpEnvironment> {
        let env_version_id = self.workdir_env.get(workdir_id)?;
        self.get_env_version(env_version_id)
    }

    pub fn get_env_version(&self, env_version_id: &str) -> Option<&UpEnvironment> {
        self.versioned_env.get(env_version_id)
    }

    pub fn clear(&mut self, workdir_id: &str) -> bool {
        if !self.contains(workdir_id) {
            return false;
        }

        self.workdir_env.remove(workdir_id);
        self.history.close(workdir_id);
        self.updated();
        true
    }

    pub fn assign_environment(
        &mut self,
        workdir_id: &str,
        head_sha: Option<String>,
        environment: &mut UpEnvironment,
    ) -> (bool, String) {
        let mut new_env = true;
        let env_hash = environment.hash_string();
        let env_version_id = format!("{}%{}", workdir_id, env_hash);

        // Check if the version already exists in the cache
        if let Some(existing_env) = self.versioned_env.get_mut(&env_version_id) {
            existing_env.assigning();
            new_env = false;
        } else {
            environment.assigning();
            self.versioned_env
                .insert(env_version_id.clone(), environment.clone());
        }

        self.workdir_env
            .insert(workdir_id.to_string(), env_version_id.clone());
        self.history.add(workdir_id, head_sha, &env_version_id);

        self.cleanup();
        (new_env, env_version_id)
    }

    pub fn environment_ids(&self) -> BTreeSet<String> {
        self.versioned_env.keys().cloned().collect()
    }

    pub fn cleanup(&mut self) {
        let config = global_config().cache.environment;
        self.history.cleanup(
            Some(Duration::seconds(config.retention as i64)),
            config.max_per_workdir,
            config.max_total,
        );

        // Get all the environment IDs that are in the history or active
        // (active should also be in the history but.. just in case)
        let keep_env_ids: Vec<_> = self
            .workdir_env
            .values()
            .chain(self.history.environment_ids().iter())
            .cloned()
            .collect();

        // Remove any environment that is not in the history or active
        self.versioned_env.retain(|id, _| keep_env_ids.contains(id));

        // Mark as updated
        self.updated();
    }
}

impl Empty for UpEnvironmentsCache {
    fn is_empty(&self) -> bool {
        self.workdir_env.is_empty() && self.versioned_env.is_empty()
    }
}

impl CacheObject for UpEnvironmentsCache {
    fn new_empty() -> Self {
        Self {
            workdir_env: BTreeMap::new(),
            versioned_env: BTreeMap::new(),
            history: UpEnvironmentHistory::default(),
            updated_at: utils::origin_of_time(),
        }
    }

    fn get() -> Self {
        get_up_environments_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(UP_ENVIRONMENTS_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(
            UP_ENVIRONMENTS_CACHE_NAME,
            processing_fn,
            set_up_environments_cache,
        )
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
    /// The time this environment was last updated
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub last_assigned_at: OffsetDateTime,
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

impl UpEnvironment {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            paths: Vec::new(),
            env_vars: Vec::new(),
            config_modtimes: BTreeMap::new(),
            config_hash: String::new(),
            last_assigned_at: utils::origin_of_time(),
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

    pub fn assigning(&mut self) {
        self.last_assigned_at = OffsetDateTime::now_utc();
    }

    pub fn add_env_var(&mut self, key: &str, value: &str) -> bool {
        self.add_env_var_operation(key, value, EnvOperationEnum::Set)
    }

    pub fn add_env_var_operation(
        &mut self,
        key: &str,
        value: &str,
        operation: EnvOperationEnum,
    ) -> bool {
        let up_env_var = UpEnvVar {
            name: key.to_string(),
            value: Some(value.to_string()),
            operation,
        };

        self.env_vars.push(up_env_var);

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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(transparent)]
pub struct UpEnvironmentHistory {
    pub entries: Vec<UpEnvironmentHistoryEntry>,
}

impl Empty for UpEnvironmentHistory {
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl UpEnvironmentHistory {
    /// Adds a new entry if needed
    pub fn add(
        &mut self,
        workdir_id: &str,
        head_sha: Option<String>,
        env_version_id: &str,
    ) -> bool {
        // Check if we have an open entry for this workdir
        if let Some(open_entry) = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.workdir_id == workdir_id && entry.is_open())
        {
            // Check if the environment version is different, in which case
            // we can return right away since we are already on the correct
            // version in the history
            if open_entry.env_version_id == env_version_id && open_entry.head_sha == head_sha {
                return false;
            }

            // Close the current entry
            open_entry.used_until_date = Some(omni_now());
        }

        // If we get here, we need to add the new entry, at the beginning of the list
        // to keep the most recent entries at the top
        self.entries.insert(
            0,
            UpEnvironmentHistoryEntry::new(workdir_id, head_sha, env_version_id),
        );

        // Return true to indicate that we added a new entry
        true
    }

    /// Closes the most recent open entry for the given workdir
    pub fn close(&mut self, workdir_id: &str) -> bool {
        if let Some(open_entry) = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.workdir_id == workdir_id && entry.is_open())
        {
            open_entry.used_until_date = Some(omni_now());
            true
        } else {
            false
        }
    }

    /// Cleans up history entries based on given constraints:
    /// - Ensures only one open entry per workdir (closes duplicates)
    /// - If max_per_workdir specified, keeps only that many entries per workdir (keeping open ones)
    /// - Removes entries older than max_retention if specified
    /// - Limits total entries to max_total if specified
    /// - Sorts entries with most recent at the top (open entries first)
    pub fn cleanup(
        &mut self,
        max_retention: Option<Duration>,
        max_per_workdir: Option<usize>,
        max_total: Option<usize>,
    ) {
        let now = omni_now();

        // Go over all the entries to make sure that we have only one open per work directory
        let mut workdirs_open: BTreeSet<String> = BTreeSet::new();
        for entry in self.entries.iter_mut() {
            if entry.is_open() && !workdirs_open.insert(entry.workdir_id.clone()) {
                // If we already have an open entry for this workdir, close this one
                entry.used_until_date = Some(now);
            }
        }

        // Apply retention policy if specified
        if let Some(retention) = max_retention {
            let cutoff = now - retention;
            self.entries.retain(|entry| {
                entry.is_open() || entry.used_until_date.map_or(true, |date| date > cutoff)
            });
        }

        // Sort entries with most recent at top (open entries first)
        self.entries.sort_by(|a, b| {
            match (a.used_until_date, b.used_until_date) {
                (None, None) => b.used_from_date.cmp(&a.used_from_date), // Both open, compare by start date
                (None, Some(_)) => std::cmp::Ordering::Less, // a is open, should come first
                (Some(_), None) => std::cmp::Ordering::Greater, // b is open, should come first
                (Some(date_a), Some(date_b)) => match date_b.cmp(&date_a) {
                    std::cmp::Ordering::Equal => b.used_from_date.cmp(&a.used_from_date), // Same end date, use start date
                    other => other,
                },
            }
        });

        // Apply max_per_workdir limit if specified
        if let Some(max_per_workdir) = max_per_workdir {
            let mut workdir_counts: HashMap<String, usize> = HashMap::new();
            self.entries.retain(|entry| {
                let count = workdir_counts.entry(entry.workdir_id.clone()).or_insert(0);
                *count += 1;
                entry.is_open() || *count <= max_per_workdir
            });
        }

        // Apply max_total limit if specified
        if let Some(max_total) = max_total {
            if self.entries.len() > max_total {
                // Count open entries to ensure we don't remove them
                let open_count = self.entries.iter().filter(|e| e.is_open()).count();
                let to_keep = max_total.max(open_count);

                // Remove oldest closed entries until we reach max_total
                self.entries.truncate(to_keep);
            }
        }
    }

    pub fn environment_ids(&self) -> BTreeSet<String> {
        self.entries
            .iter()
            .map(|entry| entry.env_version_id.clone())
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentHistoryEntry {
    #[serde(rename = "wd")]
    pub workdir_id: String,
    #[serde(rename = "sha", skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    #[serde(rename = "env")]
    pub env_version_id: String,
    #[serde(
        rename = "from",
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339"
    )]
    pub used_from_date: OffsetDateTime,
    #[serde(
        rename = "until",
        default,
        skip_serializing_if = "Option::is_none",
        with = "utils::optional_rfc3339"
    )]
    pub used_until_date: Option<OffsetDateTime>,
}

impl UpEnvironmentHistoryEntry {
    pub fn new(workdir_id: &str, head_sha: Option<String>, env_version_id: &str) -> Self {
        Self {
            workdir_id: workdir_id.to_string(),
            head_sha,
            env_version_id: env_version_id.to_string(),
            used_from_date: omni_now(),
            used_until_date: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.used_until_date.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
