use std::collections::HashMap;

use blake3::Hasher;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use shell_escape::escape;

use crate::internal::cache::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config::up::asdf_tool_path;
use crate::internal::env::user_home;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;

const DATA_SEPARATOR: &str = "\x1C";
const DYNENV_VAR: &str = "__omni_dynenv";
const DYNENV_SEPARATOR: &str = ";";

pub fn update_dynamic_env(export_mode: DynamicEnvExportMode) {
    update_dynamic_env_with_path(export_mode, None);
}

pub fn update_dynamic_env_for_command<T: ToString>(path: T) {
    update_dynamic_env_with_path(DynamicEnvExportMode::Env, Some(path.to_string()));
}

pub fn update_dynamic_env_with_path(export_mode: DynamicEnvExportMode, path: Option<String>) {
    let cache = UpEnvironmentsCache::get();
    let mut current_env = DynamicEnv::from_env(cache.clone());
    let mut expected_env = DynamicEnv::new_with_path(path, cache.clone());

    if current_env.id() == expected_env.id() {
        return;
    }

    current_env.undo(export_mode.clone());
    expected_env.apply(export_mode.clone());

    if export_mode != DynamicEnvExportMode::Env {
        match (current_env.id(), expected_env.id()) {
            (0, 0) => {}
            (0, _) => {
                let features_str = if expected_env.features.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        " ({})",
                        expected_env
                            .features
                            .iter()
                            .map(|f| f.force_light_blue())
                            .join(", "),
                    )
                    .force_italic()
                };
                print_update(
                    format!(
                        "dynamic environment {}{}",
                        "enabled".to_string().force_light_green(),
                        features_str,
                    )
                    .as_str(),
                );
            }
            (_, 0) => {
                print_update(
                    format!(
                        "dynamic environment {}",
                        "disabled".to_string().force_light_red(),
                    )
                    .as_str(),
                );
            }
            (_, _) => {
                let features_str = if expected_env.features.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        " ({})",
                        expected_env
                            .features
                            .iter()
                            .map(|f| f.force_light_blue())
                            .join(", "),
                    )
                    .force_italic()
                };
                print_update(
                    format!(
                        "dynamic environment {}{}",
                        "updated".to_string().force_light_blue(),
                        features_str,
                    )
                    .as_str(),
                );
            }
        }
    }
}

fn print_update(status: &str) {
    eprintln!("{} {}", "omni:".to_string().force_light_cyan(), status);
}

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicEnvExportMode {
    Posix,
    Fish,
    Env,
}

pub struct DynamicEnv {
    path: Option<String>,
    id: OnceCell<u64>,
    data_str: Option<String>,
    data: Option<DynamicEnvData>,
    features: Vec<String>,
    cache: UpEnvironmentsCache,
}

impl DynamicEnv {
    fn new_with_path(path: Option<String>, cache: UpEnvironmentsCache) -> Self {
        Self {
            path,
            id: OnceCell::new(),
            data_str: None,
            data: None,
            features: Vec::new(),
            cache,
        }
    }

    pub fn from_env(cache: UpEnvironmentsCache) -> Self {
        let (cur_id, cur_data) = current_env();

        let id = OnceCell::new();
        id.set(cur_id).unwrap();

        Self {
            path: None,
            id,
            data_str: cur_data,
            data: None,
            features: Vec::new(),
            cache,
        }
    }

    pub fn id(&self) -> u64 {
        *self.id.get_or_init(|| {
            // Get the current path
            let path = self.path.clone().unwrap_or(".".to_string());

            // Get the workdir environment
            let workdir = workdir(&path);
            if !workdir.in_workdir() {
                return 0;
            }

            // Get the workdir id
            let workdir_id = match workdir.id() {
                Some(workdir_id) => workdir_id,
                None => return 0,
            };

            // Get the relative directory
            let dir = workdir.reldir(&path).unwrap_or("".to_string());

            // Check if repo is 'up' and should have its environment loaded
            let up_env = if let Some(up_env) = self.cache.get_env(&workdir_id) {
                up_env
            } else {
                return 0;
            };

            // Prepare the hash
            let mut hasher = Hasher::new();

            // Try and get the shell PPID by using the PPID environment variables
            let ppid = std::env::var("OMNI_SHELL_PPID").unwrap_or("".to_string());
            hasher.update(ppid.as_bytes());
            hasher.update(DATA_SEPARATOR.as_bytes());

            // Let's add the workdir location and the workdir id to the hash
            hasher.update(workdir.root().unwrap().as_bytes());
            hasher.update(DATA_SEPARATOR.as_bytes());
            hasher.update(workdir.id().unwrap().as_bytes());
            hasher.update(DATA_SEPARATOR.as_bytes());

            // Add the requested environments to the hash, sorted by key
            for (key, value) in up_env.env_vars.iter().sorted() {
                hasher.update(key.as_bytes());
                hasher.update(DATA_SEPARATOR.as_bytes());
                hasher.update(value.as_bytes());
                hasher.update(DATA_SEPARATOR.as_bytes());
            }

            // Add the requested paths to the hash
            for path in up_env.paths.iter().rev() {
                hasher.update(path.to_str().unwrap().as_bytes());
                hasher.update(DATA_SEPARATOR.as_bytes());
            }

            // Go over the tool versions in the up environment cache
            for toolversion in up_env.versions_for_dir(&dir).iter() {
                hasher.update(toolversion.tool.as_bytes());
                hasher.update(DATA_SEPARATOR.as_bytes());
                hasher.update(toolversion.version.as_bytes());
                hasher.update(DATA_SEPARATOR.as_bytes());
                if let Some(data_path) = &toolversion.data_path {
                    hasher.update(data_path.as_bytes());
                    hasher.update(DATA_SEPARATOR.as_bytes());
                }
            }

            // Convert the hash to a u64
            let hash_bytes = hasher.finalize();
            let hash_u64 = u64::from_le_bytes(hash_bytes.as_bytes()[..8].try_into().unwrap());

            // Return the hash
            hash_u64
        })
    }

    pub fn id_str(&self) -> String {
        format!("{:016x}", self.id())
    }

    pub fn apply(&mut self, export_mode: DynamicEnvExportMode) {
        let mut envsetter = DynamicEnvSetter::new();

        let mut up_env = None;
        let path = self.path.clone().unwrap_or(".".to_string());
        let workdir = workdir(&path);
        if workdir.in_workdir() {
            if let Some(workdir_id) = workdir.id() {
                up_env = self.cache.get_env(&workdir_id);
            } else {
                return;
            }
        }

        if let Some(up_env) = &up_env {
            // Add the requested environments to the hash, sorted by key
            if !up_env.env_vars.is_empty() {
                self.features.push("env".to_string());
            }
            for (key, value) in up_env.env_vars.iter() {
                envsetter.set_value(key, value);
            }

            // Add the requested paths
            for path in up_env.paths.iter().rev() {
                envsetter.prepend_to_list("PATH", path.to_str().unwrap());
            }

            // Go over the tool versions in the up environment cache
            let dir = workdir.reldir(&path).unwrap_or("".to_string());
            for toolversion in up_env.versions_for_dir(&dir).iter() {
                let tool = toolversion.tool.clone();
                let version = toolversion.version.clone();
                let version_minor = version.split('.').take(2).join(".");
                let tool_prefix = asdf_tool_path(&tool, &version);

                self.features.push(format!("{}:{}", tool, version));

                match tool.as_str() {
                    "ruby" => {
                        envsetter.remove_from_list_by_fn("PATH", || {
                            let mut values_to_remove = Vec::new();

                            if let Some(rubyroot) = std::env::var_os("RUBY_ROOT") {
                                values_to_remove
                                    .push(format!("{}/bin", rubyroot.to_str().unwrap()));
                            }

                            if let Some(gemroot) = std::env::var_os("GEM_ROOT") {
                                values_to_remove.push(format!("{}/bin", gemroot.to_str().unwrap()));
                            }

                            if let Some(gemhome) = std::env::var_os("GEM_HOME") {
                                values_to_remove.push(format!("{}/bin", gemhome.to_str().unwrap()));
                            }

                            values_to_remove
                        });
                        envsetter.set_value(
                            "GEM_HOME",
                            &format!("{}/lib/ruby/gems/{}.0", tool_prefix, version_minor),
                        );
                        envsetter.set_value(
                            "GEM_ROOT",
                            &format!("{}/lib/ruby/gems/{}.0", tool_prefix, version_minor),
                        );
                        envsetter.set_value("RUBY_ENGINE", "ruby");
                        envsetter.set_value("RUBY_ROOT", &tool_prefix);
                        envsetter.set_value("RUBY_VERSION", &version);
                        envsetter.prepend_to_list(
                            "GEM_PATH",
                            &format!("{}/lib/ruby/gems/{}.0", tool_prefix, version_minor),
                        );
                        envsetter.prepend_to_list(
                            "PATH",
                            &format!("{}/lib/ruby/gems/{}/bin", tool_prefix, version_minor),
                        );
                        envsetter.prepend_to_list("PATH", &format!("{}/bin", tool_prefix));
                    }
                    "rust" => {
                        envsetter.set_value("RUSTUP_HOME", &tool_prefix);
                        envsetter.set_value("CARGO_HOME", &tool_prefix);
                        envsetter.prepend_to_list("PATH", &format!("{}/bin", tool_prefix));
                    }
                    "golang" => {
                        if let Some(goroot) = std::env::var_os("GOROOT") {
                            envsetter.remove_from_list(
                                "PATH",
                                &format!("{}/bin", goroot.to_str().unwrap()),
                            );
                        }

                        if std::env::var_os("GOMODCACHE").is_none() {
                            let gopath = match std::env::var_os("GOPATH") {
                                Some(gopath) => match gopath.to_str() {
                                    Some("") | None => format!("{}/go", user_home()),
                                    Some(gopath) => gopath.to_string(),
                                },
                                None => format!("{}/go", user_home()),
                            };
                            envsetter.set_value("GOMODCACHE", &format!("{}/pkg/mod", gopath));
                        }

                        envsetter.set_value("GOROOT", &format!("{}/go", tool_prefix));
                        envsetter.set_value("GOVERSION", &version);
                        envsetter.prepend_to_list("GOPATH", &format!("{}/go", tool_prefix));
                        envsetter.prepend_to_list("PATH", &format!("{}/go/bin", tool_prefix));

                        // Handle the isolated GOPATH
                        if let Some(data_path) = &toolversion.data_path {
                            envsetter.prepend_to_list("GOPATH", data_path);
                            envsetter.prepend_to_list("PATH", &format!("{}/bin", data_path));
                        };
                    }
                    "python" => {
                        let tool_prefix = if let Some(data_path) = &toolversion.data_path {
                            envsetter.set_value("VIRTUAL_ENV", data_path);
                            data_path.clone()
                        } else {
                            tool_prefix
                        };

                        envsetter.unset_value("PYTHONHOME");
                        envsetter.prepend_to_list("PATH", &format!("{}/bin", tool_prefix));
                    }
                    // "nodejs" => {
                    // envsetter.set_value("NVM_DIR", "$HOME/.nvm");
                    // envsetter.set_value("NVM_BIN", "$NVM_DIR/versions/node/$NODE_VERSION/bin");
                    // envsetter.set_value("NODE_VERSION", &toolversion.version);
                    // }
                    _ => {
                        envsetter.prepend_to_list("PATH", &format!("{}/bin", tool_prefix));
                    }
                }
            }
        }

        // Set the OMNI_LOADED_FEATURES variable so that it can easily be used in
        // the shell to keep showing up loaded features in the prompt or anywhere
        // else users wish.
        if !self.features.is_empty() {
            envsetter.set_value("OMNI_LOADED_FEATURES", &self.features.join(" "));
        } else {
            envsetter.unset_value("OMNI_LOADED_FEATURES");
        }

        // Set the dynamic env variable so we can easily undo things
        let json_data = envsetter.get_env_data().to_json();
        if self.id() == 0 {
            envsetter.unset_value(DYNENV_VAR);
        } else {
            envsetter.set_value(
                DYNENV_VAR,
                &format!("{}{}{}", self.id_str(), DYNENV_SEPARATOR, json_data),
            );
        }

        self.data = Some(envsetter.get_env_data());
        self.data.clone().unwrap().export(export_mode.clone());
    }

    pub fn undo(&mut self, export_mode: DynamicEnvExportMode) {
        if self.data.is_none() && self.data_str.is_some() {
            let data: Result<DynamicEnvData, _> =
                serde_json::from_str(&self.data_str.clone().unwrap());
            if data.is_err() {
                return;
            }
            let data = data.unwrap();
            self.data = Some(data);
        }

        if self.data.is_none() {
            return;
        }

        let mut data = self.data.clone().unwrap();
        data.prepare_undo();
        data.export(export_mode.clone());
    }
}

enum DynamicEnvOperation {
    SetValue(String, String),
    UnsetValue(String),
    PrependToList(String, String),
    AppendToList(String, String),
    RemoveFromList(String, String),
    RemoveFromListByFn(String, Box<dyn Fn() -> Vec<String>>),
}

struct DynamicEnvSetter {
    operations: Vec<DynamicEnvOperation>,
}

impl DynamicEnvSetter {
    fn new() -> Self {
        DynamicEnvSetter {
            operations: Vec::new(),
        }
    }

    fn set_value(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::SetValue(
            key.to_string(),
            value.to_string(),
        ));
    }

    fn unset_value(&mut self, key: &str) {
        self.operations
            .push(DynamicEnvOperation::UnsetValue(key.to_string()));
    }

    fn prepend_to_list(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::PrependToList(
            key.to_string(),
            value.to_string(),
        ));
    }

    #[allow(dead_code)]
    fn append_to_list(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::AppendToList(
            key.to_string(),
            value.to_string(),
        ));
    }

    fn remove_from_list(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::RemoveFromList(
            key.to_string(),
            value.to_string(),
        ));
    }

    fn remove_from_list_by_fn<F>(&mut self, key: &str, f: F)
    where
        F: Fn() -> Vec<String> + 'static,
    {
        self.operations
            .push(DynamicEnvOperation::RemoveFromListByFn(
                key.to_string(),
                Box::new(f),
            ));
    }

    fn get_env_data(&self) -> DynamicEnvData {
        let mut data = DynamicEnvData::new();

        for operation in self.operations.iter() {
            match operation {
                DynamicEnvOperation::SetValue(key, value) => {
                    data.set_value(key, value);
                }
                DynamicEnvOperation::UnsetValue(key) => {
                    data.unset_value(key);
                }
                DynamicEnvOperation::PrependToList(key, value) => {
                    data.prepend_to_list(key, value);
                }
                DynamicEnvOperation::AppendToList(key, value) => {
                    data.append_to_list(key, value);
                }
                DynamicEnvOperation::RemoveFromList(key, value) => {
                    data.remove_from_list(key, value);
                }
                DynamicEnvOperation::RemoveFromListByFn(key, f) => {
                    let values_to_remove = f();
                    for value in values_to_remove.iter() {
                        data.remove_from_list(key, value);
                    }
                }
            }
        }

        data
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct DynamicEnvData {
    #[serde(
        rename = "v",
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty"
    )]
    values: HashMap<String, DynamicEnvValue>,
    #[serde(
        rename = "l",
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty"
    )]
    lists: HashMap<String, Vec<DynamicEnvListValue>>,
    #[serde(skip)]
    env: HashMap<String, Option<String>>,
}

impl DynamicEnvData {
    fn new() -> Self {
        DynamicEnvData {
            values: HashMap::new(),
            lists: HashMap::new(),
            env: HashMap::new(),
        }
    }

    fn env_set_var(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_string(), Some(value.to_string()));
    }

    fn env_unset_var(&mut self, key: &str) {
        if self.env.contains_key(key) || std::env::var(key).is_ok() {
            self.env.insert(key.to_string(), None);
        }
    }

    fn env_get_var(&self, key: &str) -> Option<String> {
        if self.env.contains_key(key) {
            self.env.get(key).unwrap().clone()
        } else if let Ok(val) = std::env::var(key) {
            Some(val)
        } else {
            None
        }
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    fn set_value(&mut self, key: &str, value: &str) {
        if !self.values.contains_key(key) {
            let prev = self.env_get_var(key);
            if prev.is_some() && prev.as_ref().unwrap() == value {
                return;
            }

            self.values.insert(
                key.to_string(),
                DynamicEnvValue {
                    prev,
                    curr: Some(value.to_string()),
                },
            );
        } else {
            self.values.get_mut(key).unwrap().curr = Some(value.to_string());
        }

        self.env_set_var(key, value);
    }

    fn unset_value(&mut self, key: &str) {
        if !self.values.contains_key(key) {
            let prev = self.env_get_var(key);
            if prev.is_none() {
                return;
            }

            self.values
                .insert(key.to_string(), DynamicEnvValue { prev, curr: None });
        } else {
            self.values.get_mut(key).unwrap().curr = None;
        }

        self.env_unset_var(key);
    }

    fn prepend_to_list(&mut self, key: &str, value: &str) {
        if !self.lists.contains_key(key) {
            self.lists.insert(key.to_string(), Vec::new());
        }

        self.lists.get_mut(key).unwrap().push(DynamicEnvListValue {
            operation: DynamicEnvListOperation::Add,
            value: value.to_string(),
            index: 0,
        });

        let cur_val = self.env_get_var(key).unwrap_or("".to_string());
        if cur_val.is_empty() {
            self.env_set_var(key, value);
        } else {
            self.env_set_var(key, &format!("{}:{}", value, cur_val));
        }
    }

    fn append_to_list(&mut self, key: &str, value: &str) {
        if !self.lists.contains_key(key) {
            self.lists.insert(key.to_string(), Vec::new());
        }

        let cur_val = self.env_get_var(key).unwrap_or("".to_string());

        let index = {
            let prev = cur_val.split(':').collect::<Vec<&str>>();
            prev.len()
        };

        self.lists.get_mut(key).unwrap().push(DynamicEnvListValue {
            operation: DynamicEnvListOperation::Add,
            value: value.to_string(),
            index,
        });

        if cur_val.is_empty() {
            self.env_set_var(key, value);
        } else {
            self.env_set_var(key, &format!("{}:{}", cur_val, value));
        }
    }

    fn remove_from_list(&mut self, key: &str, value: &str) {
        if !self.lists.contains_key(key) {
            self.lists.insert(key.to_string(), Vec::new());
        }

        if let Some(prev) = self.env_get_var(key) {
            let mut prev = prev.split(':').collect::<Vec<&str>>();
            if let Some(index) = prev.iter().position(|&r| r == value) {
                self.lists.get_mut(key).unwrap().push(DynamicEnvListValue {
                    operation: DynamicEnvListOperation::Del,
                    value: value.to_string(),
                    index,
                });

                prev.remove(index);
                self.env_set_var(key, &prev.join(":"));
            }
        };
    }

    fn prepare_undo(&mut self) {
        self.env = HashMap::new();

        for (key, value) in self.values.clone().iter() {
            let _existing_var = self.env_get_var(key);
            if value.curr.clone() != self.env_get_var(key) {
                // The user has manually changed the value, we don't want to
                // touch it here.
                continue;
            }

            if let Some(prev) = &value.prev {
                self.env_set_var(key, prev);
            } else {
                self.env_unset_var(key);
            }
        }

        for (key, operations) in self.lists.clone().iter() {
            // Load the content of the variables, as we'll need to "undo" the
            // operations we've done to the closest of our ability; since it's
            // a list, we'll also split it, so we're ready to "search and update"
            let cur_val = self.env_get_var(key).unwrap_or("".to_string());
            let mut cur_val = cur_val.split(':').collect::<Vec<&str>>();

            for operation in operations.iter().rev() {
                match operation.operation {
                    DynamicEnvListOperation::Add => {
                        // Search for the operation.value in the current list, and return the closest index
                        // with operation.index in case the value is there multiple times
                        let index = cur_val
                            .iter()
                            .enumerate()
                            .filter(|(_, &r)| r == operation.value)
                            .map(|(i, _)| (i.max(operation.index) - i.min(operation.index), i))
                            .min_by_key(|(d, _)| *d);

                        // If we found it, we can remove it from the list
                        if let Some((_, index)) = index {
                            cur_val.remove(index);
                        }
                    }
                    DynamicEnvListOperation::Del => {
                        cur_val.insert(operation.index, operation.value.as_str());
                    }
                }
            }

            // We can now write the restored value to the environment
            let cur_val = cur_val.join(":");
            self.env_set_var(key, &cur_val);
        }
    }

    fn export(&self, export_mode: DynamicEnvExportMode) {
        match export_mode {
            DynamicEnvExportMode::Posix => {
                self.export_posix();
                self.export_env();
            }
            DynamicEnvExportMode::Fish => {
                self.export_fish();
                self.export_env();
            }
            DynamicEnvExportMode::Env => {
                self.export_env();
            }
        }
    }

    fn export_env(&self) {
        for (key, value) in self.env.iter() {
            match value {
                Some(value) => {
                    std::env::set_var(key, value);
                }
                None => {
                    std::env::remove_var(key);
                }
            }
        }
    }

    fn export_posix(&self) {
        for (key, value) in self.env.iter() {
            match value {
                Some(value) => {
                    println!(
                        "export {}={}",
                        key,
                        escape(std::borrow::Cow::Borrowed(value))
                    );
                }
                None => {
                    println!("unset {}", key);
                }
            }
        }
    }

    fn export_fish(&self) {
        for (key, value) in self.env.iter() {
            match value {
                Some(value) => {
                    if key == "PATH" {
                        let path = value
                            .split(':')
                            .map(|s| escape(std::borrow::Cow::Borrowed(s)))
                            .join(" ");
                        println!("set -gx {} {}", key, path);
                    } else {
                        println!(
                            "set -gx {} {}",
                            key,
                            escape(std::borrow::Cow::Borrowed(value))
                        );
                    }
                }
                None => {
                    println!("set -e {}", key);
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct DynamicEnvValue {
    #[serde(rename = "p", default = "set_none", skip_serializing_if = "is_none")]
    prev: Option<String>,
    #[serde(rename = "c", default = "set_none", skip_serializing_if = "is_none")]
    curr: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
enum DynamicEnvListOperation {
    #[serde(rename = "a")]
    Add,
    #[serde(rename = "d")]
    Del,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct DynamicEnvListValue {
    #[serde(rename = "o")]
    operation: DynamicEnvListOperation,
    #[serde(rename = "v")]
    value: String,
    #[serde(rename = "i")]
    index: usize,
}

impl DynamicEnvListValue {
    // fn new(operation: DynamicEnvListOperation, value: &str, index: usize) -> Self {
    // Self {
    // operation: operation,
    // value: value.to_string(),
    // index: index,
    // }
    // }
}

fn set_none() -> Option<String> {
    None
}

fn is_none(value: &Option<String>) -> bool {
    value.is_none()
}

fn current_env() -> (u64, Option<String>) {
    let dynenv = std::env::var(DYNENV_VAR);
    if dynenv.is_err() {
        return (0, None);
    }
    let dynenv = dynenv.unwrap();

    let mut parts = dynenv.splitn(2, DYNENV_SEPARATOR);

    let cur_id = parts.next();
    let cur_id = match cur_id {
        None => None,
        Some("") => None,
        Some("0000000000000000") => None,
        Some(hex) => hex_to_id(hex),
    };
    if cur_id.is_none() {
        return (0, None);
    }

    let cur_id = cur_id.unwrap();
    let cur_data = parts.next().unwrap_or("{}");

    (cur_id, Some(cur_data.to_string()))
}

fn hex_to_id(hex: &str) -> Option<u64> {
    if hex.len() != 16 {
        return None;
    }
    let cur_id = u64::from_str_radix(hex, 16);
    if cur_id.is_err() {
        return None;
    }
    Some(cur_id.unwrap())
}
