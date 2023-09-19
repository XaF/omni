use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;

use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_up_environments_cache;
use crate::internal::cache::loaders::set_up_environments_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;

const UP_ENVIRONMENTS_CACHE_NAME: &str = "up_environments";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentsCache {
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, UpEnvironment>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl UpEnvironmentsCache {
    pub fn set_env_vars(&mut self, workdir_id: &str, env_vars: HashMap<String, String>) {
        if let Some(env) = self.env.get_mut(workdir_id) {
            env.env_vars = env_vars;
        } else {
            let mut env = UpEnvironment::new();
            env.env_vars = env_vars;
            self.env.insert(workdir_id.to_string(), env);
        }
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn add_env_var(&mut self, workdir_id: &str, key: &str, value: &str) {
        if let Some(env) = self.env.get_mut(workdir_id) {
            env.env_vars.insert(key.to_string(), value.to_string());
        } else {
            let mut env = UpEnvironment::new();
            env.env_vars.insert(key.to_string(), value.to_string());
            self.env.insert(workdir_id.to_string(), env);
        }
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn add_version(
        &mut self,
        workdir_id: &str,
        tool: &str,
        version: &str,
        dirs: BTreeSet<String>,
    ) -> bool {
        let mut dirs = dirs;

        if let Some(wd_up_env) = self.env.get(workdir_id) {
            for exists in wd_up_env.versions.iter() {
                if exists.tool == tool && exists.version == version {
                    dirs.remove(&exists.dir);
                    if dirs.is_empty() {
                        break;
                    }
                }
            }
        }

        if dirs.is_empty() {
            return false;
        }

        let wd_up_env = self.env.get_mut(workdir_id);
        let wd_up_env = if let Some(wd_up_env) = wd_up_env {
            wd_up_env
        } else {
            let env = UpEnvironment::new();
            self.env.insert(workdir_id.to_string(), env);
            self.env.get_mut(workdir_id).unwrap()
        };

        for dir in dirs {
            wd_up_env.versions.push(UpVersion {
                tool: tool.to_string(),
                version: version.to_string(),
                dir: dir.to_string(),
            });
        }

        self.updated_at = OffsetDateTime::now_utc();

        true
    }

    pub fn contains(&self, workdir_id: &str) -> bool {
        self.env.contains_key(workdir_id)
    }

    pub fn get_env(&self, workdir_id: &str) -> Option<&UpEnvironment> {
        self.env.get(workdir_id)
    }

    pub fn clear(&mut self, workdir_id: &str) -> bool {
        if !self.contains(workdir_id) {
            return false;
        }

        self.env.remove(workdir_id);
        self.updated_at = OffsetDateTime::now_utc();
        true
    }
}

impl Empty for UpEnvironmentsCache {
    fn is_empty(&self) -> bool {
        self.env.is_empty()
    }
}

impl CacheObject for UpEnvironmentsCache {
    fn new_empty() -> Self {
        Self {
            env: HashMap::new(),
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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironment {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<UpVersion>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub env_vars: HashMap<String, String>,
}

impl UpEnvironment {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            env_vars: HashMap::new(),
        }
    }

    pub fn versions_for_dir(&self, dir: &str) -> Vec<UpVersion> {
        let mut versions: BTreeMap<String, UpVersion> = BTreeMap::new();

        for version in self.versions.iter() {
            // Check if that version applies to the requested dir
            if version.dir != ""
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpVersion {
    pub tool: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
}
