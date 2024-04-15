use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

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
use crate::internal::config;
use crate::internal::config::parser::EnvOperationConfig;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::config::up::utils::get_config_mod_times;

const UP_ENVIRONMENTS_CACHE_NAME: &str = "up_environments";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentsCache {
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, UpEnvironment>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl UpEnvironmentsCache {
    fn updated(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn set_config_hash(&mut self, workdir_id: &str) -> bool {
        let config_hash = config(".").up_hash();
        if let Some(env) = self.env.get_mut(workdir_id) {
            env.config_hash = config_hash.to_string();
        } else {
            let mut env = UpEnvironment::new();
            env.config_hash = config_hash.to_string();
            self.env.insert(workdir_id.to_string(), env);
        }
        self.updated();
        true
    }

    pub fn set_config_modtimes(&mut self, workdir_id: &str) -> bool {
        let config_modtimes = get_config_mod_times(".");
        if let Some(env) = self.env.get_mut(workdir_id) {
            env.config_modtimes = config_modtimes;
        } else {
            let mut env = UpEnvironment::new();
            env.config_modtimes = config_modtimes;
            self.env.insert(workdir_id.to_string(), env);
        }
        self.updated();
        true
    }

    pub fn set_env_vars(&mut self, workdir_id: &str, env_vars: Vec<EnvOperationConfig>) -> bool {
        if let Some(env) = self.env.get_mut(workdir_id) {
            env.env_vars = env_vars.into_iter().map(|e| e.into()).collect();
        } else {
            let mut env = UpEnvironment::new();
            env.env_vars = env_vars.into_iter().map(|e| e.into()).collect();
            self.env.insert(workdir_id.to_string(), env);
        }
        self.updated();
        true
    }

    pub fn add_env_var(&mut self, workdir_id: &str, key: &str, value: &str) -> bool {
        self.add_env_var_operation(workdir_id, key, value, EnvOperationEnum::Set)
    }

    pub fn add_env_var_operation(
        &mut self,
        workdir_id: &str,
        key: &str,
        value: &str,
        operation: EnvOperationEnum,
    ) -> bool {
        let up_env_var = UpEnvVar {
            name: key.to_string(),
            value: Some(value.to_string()),
            operation,
        };

        if let Some(env) = self.env.get_mut(workdir_id) {
            env.env_vars.push(up_env_var);
        } else {
            let mut env = UpEnvironment::new();
            env.env_vars.push(up_env_var);
            self.env.insert(workdir_id.to_string(), env);
        }
        self.updated();
        true
    }

    pub fn add_path(&mut self, workdir_id: &str, path: PathBuf) -> bool {
        if let Some(env) = self.env.get_mut(workdir_id) {
            env.paths.retain(|p| p != &path);
            env.paths.push(path);
        } else {
            let mut env = UpEnvironment::new();
            env.paths.push(path);
            self.env.insert(workdir_id.to_string(), env);
        }
        self.updated();
        true
    }

    pub fn add_paths(&mut self, workdir_id: &str, paths: Vec<PathBuf>) -> bool {
        for path in paths {
            self.add_path(workdir_id, path);
        }
        true
    }

    pub fn add_version(
        &mut self,
        workdir_id: &str,
        tool: &str,
        tool_real_name: Option<&str>,
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
            wd_up_env
                .versions
                .push(UpVersion::new(tool, tool_real_name, version, &dir));
        }

        self.updated();
        true
    }

    pub fn add_version_data_path(
        &mut self,
        workdir_id: &str,
        tool: &str,
        version: &str,
        dir: &str,
        data_path: &str,
    ) -> bool {
        if let Some(wd_up_env) = self.env.get_mut(workdir_id) {
            for exists in wd_up_env.versions.iter_mut() {
                if exists.tool == tool && exists.version == version && exists.dir == dir {
                    exists.data_path = Some(data_path.to_string());
                    self.updated();
                    return true;
                }
            }
        }

        false
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

        self.updated();
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<UpVersion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<UpEnvVar>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config_modtimes: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub config_hash: String,
}

impl UpEnvironment {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            paths: Vec::new(),
            env_vars: Vec::new(),
            config_modtimes: HashMap::new(),
            config_hash: String::new(),
        }
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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
