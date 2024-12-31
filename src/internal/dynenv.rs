use std::collections::HashMap;
use std::collections::HashSet;

use blake3::Hasher;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use shell_escape::escape;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::config::up::mise::mise_path;
use crate::internal::config::up::mise_tool_path;
use crate::internal::config::up::utils::get_config_mod_times;
use crate::internal::env::shims_dir;
use crate::internal::env::user_home;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;

const DATA_SEPARATOR: &str = "\x1C";
const DYNENV_VAR: &str = "__omni_dynenv";
const DYNENV_SEPARATOR: &str = ";";
const WD_CONFIG_MODTIME_VAR: &str = "__omni_wd_config_modtime";

pub fn update_dynamic_env_for_command<T: ToString>(path: T) {
    DynamicEnvExportOptions::new(DynamicEnvExportMode::Env)
        .path(path.to_string())
        .apply();
}

pub fn update_dynamic_env_for_command_from_env<T: ToString>(path: T, environment: &UpEnvironment) {
    DynamicEnvExportOptions::new(DynamicEnvExportMode::Env)
        .path(path.to_string())
        .environment(environment)
        .apply();
}

fn remove_wd_config_modtime_var(export_mode: DynamicEnvExportMode) {
    let mut dynenvdata = DynamicEnvData::new();
    dynenvdata.env_unset_var(WD_CONFIG_MODTIME_VAR);
    dynenvdata.export(export_mode);
}

fn remove_shims_dir_from_path(export_mode: DynamicEnvExportMode) {
    let mut dynenvdata = DynamicEnvData::new();
    dynenvdata.remove_all_from_list("PATH", shims_dir().to_str().unwrap());
    dynenvdata.export(export_mode);
}

fn check_workdir_config_updated(
    export_mode: DynamicEnvExportMode,
    path: Option<String>,
    cache: &UpEnvironmentsCache,
) {
    let wdpath = path.unwrap_or(".".to_string());

    let wdid = if let Some(wdid) = workdir(&wdpath).id() {
        wdid
    } else {
        remove_wd_config_modtime_var(export_mode.clone());
        return;
    };

    // Check if we need notify the user about the workdir configuration
    // files. If not, we will just skip the rest of the function.
    let config = config(&wdpath);
    let notify_updated = config.up_command.notify_workdir_config_updated;
    let notify_available = config.up_command.notify_workdir_config_available;

    if !notify_updated && !notify_available {
        remove_wd_config_modtime_var(export_mode.clone());
        return;
    }

    // Get the mod times for the config files in the workdir
    let modtimes = get_config_mod_times(&wdpath);

    // Get the cache for the workdir
    let mut notify_change = false;
    let mut change_type = "update";
    if let Some(wdcache) = cache.get_env(&wdid) {
        if notify_updated {
            for config_file in wdcache.config_modtimes.keys() {
                if !modtimes.contains_key(config_file) {
                    notify_change = true;
                    break;
                }
            }

            if !notify_change {
                for (config_file, modtime) in modtimes.iter() {
                    match wdcache.config_modtimes.get(config_file) {
                        Some(known_modtime) => {
                            if *known_modtime != *modtime {
                                notify_change = true;
                                break;
                            }
                        }
                        None => {
                            notify_change = true;
                            break;
                        }
                    }
                }
            }
        }
    } else if notify_available && !modtimes.is_empty() {
        notify_change = true;
        change_type = "set up";
    }
    if !notify_change {
        remove_wd_config_modtime_var(export_mode.clone());
        return;
    }

    // Flatten the mod times in order of the config files paths
    let flattened = modtimes
        .iter()
        .sorted_by_key(|(config_file, _)| config_file.to_owned())
        .map(|(_, modtime)| modtime)
        .join(",");
    let expected_value = format!("{}:{}", wdid, flattened);
    let hashed = blake3::hash(expected_value.as_bytes()).to_hex()[..16].to_string();

    // Check if we have, in the environment, a variable that
    // indicates that the user has already been notified
    // about the config file being updated
    if let Some(env_var) = std::env::var_os(WD_CONFIG_MODTIME_VAR) {
        if let Ok(env_var) = env_var.into_string() {
            if env_var == hashed {
                return;
            }
        }
    }

    // If we get here, last check: we want to read the `up` configuration
    // of the workdir and hash it, and check if it is the same as the hash
    // in the cache. If it is, we don't need to notify the user, but we
    // still need to set the environment variable to avoid checking on
    // every prompt.
    if let Some(wdcache) = cache.get_env(&wdid) {
        if wdcache.config_hash == config.up_hash() {
            notify_change = false;
        }
    }

    if notify_change {
        print_update(
            format!(
                "run {} to {} the dependencies",
                "omni up".force_light_blue(),
                change_type.force_light_yellow(),
            )
            .as_str(),
        );
    }

    // Set the environment variable to indicate that the user
    // has been notified about the config file being updated
    let mut dynenvdata = DynamicEnvData::new();
    dynenvdata.env_set_var(WD_CONFIG_MODTIME_VAR, &hashed);
    dynenvdata.export(export_mode.clone());
}

pub fn update_dynamic_env(options: &DynamicEnvExportOptions) {
    if !options.keep_shims {
        remove_shims_dir_from_path(options.mode.clone());
    }

    let cache = UpEnvironmentsCache::get();
    let mut current_env = DynamicEnv::from_env(cache.clone());
    let mut expected_env = DynamicEnv::new(cache.clone())
        .with_path(options.path.clone())
        .with_environment(options.environment.as_ref());

    if !options.is_quiet() {
        check_workdir_config_updated(options.mode.clone(), options.path.clone(), &cache);
    }

    if current_env.id() == expected_env.id() {
        return;
    }

    current_env.undo(options.mode.clone());
    expected_env.apply(options.mode.clone(), options.keep_shims);

    if !options.is_quiet() {
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
                        "enabled".force_light_green(),
                        features_str,
                    )
                    .as_str(),
                );
            }
            (_, 0) => {
                print_update(
                    format!("dynamic environment {}", "disabled".force_light_red(),).as_str(),
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
                        "updated".force_light_blue(),
                        features_str,
                    )
                    .as_str(),
                );
            }
        }
    }
}

fn print_update(status: &str) {
    eprintln!("{} {}", "omni:".force_light_cyan(), status);
}

#[derive(Debug, Clone, Default)]
pub struct DynamicEnvExportOptions {
    mode: DynamicEnvExportMode,
    quiet: bool,
    keep_shims: bool,
    path: Option<String>,
    environment: Option<UpEnvironment>,
}

impl DynamicEnvExportOptions {
    pub fn new(mode: DynamicEnvExportMode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    pub fn keep_shims(mut self, keep_shims: bool) -> Self {
        self.keep_shims = keep_shims;
        self
    }

    pub fn path(mut self, path: String) -> Self {
        self.path = Some(path);
        self
    }

    pub fn environment(mut self, environment: &UpEnvironment) -> Self {
        self.environment = Some(environment.clone());
        self
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet || self.mode == DynamicEnvExportMode::Env
    }

    pub fn apply(&self) {
        update_dynamic_env(self);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum DynamicEnvExportMode {
    Posix,
    Fish,
    #[default]
    Env,
}

pub struct DynamicEnv {
    path: Option<String>,
    environment: OnceCell<Option<UpEnvironment>>,
    id: OnceCell<u64>,
    data_str: Option<String>,
    data: Option<DynamicEnvData>,
    features: Vec<String>,
    cache: UpEnvironmentsCache,
}

impl Default for DynamicEnv {
    fn default() -> Self {
        Self {
            path: None,
            environment: OnceCell::new(),
            id: OnceCell::new(),
            data_str: None,
            data: None,
            features: Vec::new(),
            cache: UpEnvironmentsCache::get(),
        }
    }
}

impl DynamicEnv {
    fn new(cache: UpEnvironmentsCache) -> Self {
        Self {
            cache,
            ..Default::default()
        }
    }

    fn with_path(mut self, path: Option<String>) -> Self {
        self.path = path;
        self
    }

    #[allow(unused_mut)]
    fn with_environment(mut self, environment: Option<&UpEnvironment>) -> Self {
        if let Some(environment) = environment {
            self.environment
                .set(Some(environment.clone()))
                .expect("failed to set environment (already set?)");
        }
        self
    }

    pub fn from_env(cache: UpEnvironmentsCache) -> Self {
        let (cur_id, cur_data) = current_env();

        let id = OnceCell::new();
        id.set(cur_id).unwrap();

        Self {
            id,
            data_str: cur_data,
            cache,
            ..Default::default()
        }
    }

    pub fn environment(&self) -> Option<UpEnvironment> {
        self.environment
            .get_or_init(|| {
                let path = self.path.clone().unwrap_or(".".to_string());
                let workdir = workdir(&path);
                match workdir.id() {
                    Some(workdir_id) => self.cache.get_env(&workdir_id),
                    None => None,
                }
            })
            .clone()
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

            // Make sure there is a workdir id
            if workdir.id().is_none() {
                return 0;
            }

            // Get the relative directory
            let dir = workdir.reldir(&path).unwrap_or("".to_string());

            // Check if repo is 'up' and should have its environment loaded
            let up_env = match self.environment() {
                Some(up_env) => up_env,
                None => return 0,
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

            // Add the requested environment operations to the hash
            for env_var in up_env.env_vars.iter() {
                hasher.update(env_var.operation.as_bytes());
                hasher.update(DATA_SEPARATOR.as_bytes());
                hasher.update(env_var.name.as_bytes());
                hasher.update(DATA_SEPARATOR.as_bytes());
                if let Some(value) = &env_var.value {
                    hasher.update(value.as_bytes());
                    hasher.update(DATA_SEPARATOR.as_bytes());
                }
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

    pub fn apply(&mut self, export_mode: DynamicEnvExportMode, keep_shims: bool) {
        let mut envsetter = DynamicEnvSetter::new();

        let mut up_env = None;
        let path = self.path.clone().unwrap_or(".".to_string());
        let workdir = workdir(&path);
        if workdir.in_workdir() {
            up_env = self.environment();
        }

        if let Some(up_env) = &up_env {
            // Add the requested environments
            if !up_env.env_vars.is_empty() {
                self.features.push("env".to_string());
            }
            for env_var in up_env.env_vars.iter() {
                match (env_var.operation, env_var.value.clone()) {
                    (EnvOperationEnum::Set, Some(value)) => {
                        envsetter.set_value(&env_var.name, &value);
                    }
                    (EnvOperationEnum::Set, None) => {
                        envsetter.unset_value(&env_var.name);
                    }
                    (EnvOperationEnum::Prepend, Some(value)) => {
                        envsetter.prepend_to_list(&env_var.name, &value);
                    }
                    (EnvOperationEnum::Append, Some(value)) => {
                        envsetter.append_to_list(&env_var.name, &value);
                    }
                    (EnvOperationEnum::Remove, Some(value)) => {
                        envsetter.remove_from_list(&env_var.name, &value);
                    }
                    (EnvOperationEnum::Prefix, Some(value)) => {
                        envsetter.prefix_value(&env_var.name, &value);
                    }
                    (EnvOperationEnum::Suffix, Some(value)) => {
                        envsetter.suffix_value(&env_var.name, &value);
                    }
                    (_, None) => {}
                }
            }

            if !keep_shims {
                // Remove the shims directory from the PATH
                envsetter.remove_all_from_list("PATH", shims_dir().to_str().unwrap());
            }

            // Add the requested paths
            for path in up_env.paths.iter().rev() {
                envsetter.prepend_to_list("PATH", path.to_str().unwrap());
            }

            // Go over the tool versions in the up environment cache
            let dir = workdir.reldir(&path).unwrap_or("".to_string());
            for toolversion in up_env.versions_for_dir(&dir).iter() {
                let tool = toolversion.tool.clone();
                let normalized_name = toolversion.normalized_name.clone();
                let version = toolversion.version.clone();
                let version_minor = version.split('.').take(2).join(".");
                let tool_prefix = mise_tool_path(&normalized_name, &version);
                let bin_path = toolversion.bin_path.clone();

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
                        envsetter.set_value("RUSTUP_HOME", &format!("{}/rustup", mise_path()));
                        envsetter.set_value("CARGO_HOME", &format!("{}/cargo", mise_path()));
                        envsetter.set_value("RUSTUP_TOOLCHAIN", &version);
                        envsetter.prepend_to_list("PATH", &tool_prefix);
                    }
                    "go" => {
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

                        envsetter.set_value("GOROOT", &tool_prefix);
                        envsetter.set_value("GOVERSION", &version);

                        let gorootbin = format!("{}/bin", tool_prefix);
                        envsetter.set_value("GOBIN", &gorootbin);
                        envsetter.prepend_to_list("PATH", &gorootbin);

                        // Handle the isolated GOPATH
                        if let Some(data_path) = &toolversion.data_path {
                            envsetter.prepend_to_list("GOPATH", data_path);

                            let gobin = format!("{}/bin", data_path);
                            envsetter.set_value("GOBIN", &gobin);
                            envsetter.prepend_to_list("PATH", &gobin);
                        }
                    }
                    "python" => {
                        let tool_prefix = if let Some(data_path) = &toolversion.data_path {
                            envsetter.set_value("VIRTUAL_ENV", data_path);
                            data_path.clone()
                        } else {
                            tool_prefix
                        };

                        envsetter.unset_value("PYTHONHOME");
                        envsetter.prepend_to_list("PATH", &format!("{}/{}", tool_prefix, bin_path));
                    }
                    "nodejs" => {
                        envsetter.set_value("NODE_VERSION", &version);
                        envsetter.prepend_to_list("PATH", &format!("{}/{}", tool_prefix, bin_path));

                        // Handle the isolated NPM prefix
                        if let Some(data_path) = &toolversion.data_path {
                            envsetter.set_value("npm_config_prefix", data_path);
                            envsetter.prepend_to_list("PATH", &format!("{}/bin", data_path));
                        };
                    }
                    _ => {
                        envsetter.prepend_to_list("PATH", &format!("{}/{}", tool_prefix, bin_path));
                    }
                }
            }
        }

        // If any FLAGS variable is set, we can clean it up by removing the duplicate
        // flags; this is particularly useful when using nix, since we will just be appending all
        // flags to variables like CFLAGS, CPPFLAGS, LDFLAGS, etc.
        envsetter.set_value_by_fn("CFLAGS", dedup_flags);
        envsetter.set_value_by_fn("CPPFLAGS", dedup_flags);
        envsetter.set_value_by_fn("LDFLAGS", dedup_flags);

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
    /// Set a value for a variable
    SetValue(String, String),
    /// Set a value for a variable by a function; if the function returns None,
    /// the variable will not be touched
    SetValueByFn(String, Box<dyn Fn(Option<String>) -> Option<String>>),
    /// Unset a variable
    UnsetValue(String),
    /// Prefix a value to a variable
    PrefixValue(String, String),
    /// Suffix a value to a variable
    SuffixValue(String, String),
    /// Prepend a value to a list, using ':' as separator
    PrependToList(String, String),
    /// Append a value to a list, using ':' as separator
    AppendToList(String, String),
    /// Remove a value from a list, using ':' as separator
    RemoveFromList(String, String),
    /// Remove all occurrences of a value from a list, using ':' as separator
    RemoveAllFromList(String, String),
    /// Remove values from a list by a function, using ':' as separator;
    /// the function should return a list of values to remove
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

    fn set_value_by_fn<F>(&mut self, key: &str, f: F)
    where
        F: Fn(Option<String>) -> Option<String> + 'static,
    {
        self.operations.push(DynamicEnvOperation::SetValueByFn(
            key.to_string(),
            Box::new(f),
        ));
    }

    fn unset_value(&mut self, key: &str) {
        self.operations
            .push(DynamicEnvOperation::UnsetValue(key.to_string()));
    }

    fn prefix_value(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::PrefixValue(
            key.to_string(),
            value.to_string(),
        ));
    }

    fn suffix_value(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::SuffixValue(
            key.to_string(),
            value.to_string(),
        ));
    }

    fn prepend_to_list(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::PrependToList(
            key.to_string(),
            value.to_string(),
        ));
    }

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

    fn remove_all_from_list(&mut self, key: &str, value: &str) {
        self.operations.push(DynamicEnvOperation::RemoveAllFromList(
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
                DynamicEnvOperation::SetValueByFn(key, f) => {
                    if let Some(value) = f(data.env_get_var(key)) {
                        data.set_value(key, &value);
                    }
                }
                DynamicEnvOperation::UnsetValue(key) => {
                    data.unset_value(key);
                }
                DynamicEnvOperation::PrefixValue(key, value) => {
                    data.prefix_value(key, value);
                }
                DynamicEnvOperation::SuffixValue(key, value) => {
                    data.suffix_value(key, value);
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
                DynamicEnvOperation::RemoveAllFromList(key, value) => {
                    data.remove_all_from_list(key, value);
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

    fn prefix_value(&mut self, key: &str, value: &str) {
        let curr = match self.values.get_mut(key) {
            Some(envvalue) => {
                let curr = format!(
                    "{}{}",
                    value,
                    envvalue.curr.clone().unwrap_or("".to_string())
                );
                envvalue.curr = Some(curr.clone());

                curr
            }
            None => {
                let prev = self.env_get_var(key);
                let curr = format!("{}{}", value, prev.clone().unwrap_or("".to_string()));

                self.values.insert(
                    key.to_string(),
                    DynamicEnvValue {
                        prev,
                        curr: Some(curr.clone()),
                    },
                );

                curr
            }
        };

        self.env_set_var(key, &curr);
    }

    fn suffix_value(&mut self, key: &str, value: &str) {
        let curr = match self.values.get_mut(key) {
            Some(envvalue) => {
                let curr = format!(
                    "{}{}",
                    envvalue.curr.clone().unwrap_or("".to_string()),
                    value
                );
                envvalue.curr = Some(curr.clone());

                curr
            }
            None => {
                let prev = self.env_get_var(key);
                let curr = format!("{}{}", prev.clone().unwrap_or("".to_string()), value);

                self.values.insert(
                    key.to_string(),
                    DynamicEnvValue {
                        prev,
                        curr: Some(curr.clone()),
                    },
                );

                curr
            }
        };

        self.env_set_var(key, &curr);
    }

    fn prepend_to_list(&mut self, key: &str, value: &str) {
        if !self.lists.contains_key(key) {
            self.lists.insert(key.to_string(), Vec::new());
        }

        let (cur_val, operation) = match self.env_get_var(key) {
            Some(cur_val) => (cur_val, DynamicEnvListOperation::Add),
            None => ("".to_string(), DynamicEnvListOperation::Create),
        };

        self.lists.get_mut(key).unwrap().push(DynamicEnvListValue {
            operation,
            value: value.to_string(),
            index: 0,
        });

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

        let (cur_val, operation) = match self.env_get_var(key) {
            Some(cur_val) => (cur_val, DynamicEnvListOperation::Add),
            None => ("".to_string(), DynamicEnvListOperation::Create),
        };

        let index = {
            let prev = cur_val.split(':').collect::<Vec<&str>>();
            prev.len()
        };

        self.lists.get_mut(key).unwrap().push(DynamicEnvListValue {
            operation,
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
        if let Some(prev) = self.env_get_var(key) {
            let mut prev = prev.split(':').collect::<Vec<&str>>();
            if let Some(index) = prev.iter().position(|&r| r == value) {
                if !self.lists.contains_key(key) {
                    self.lists.insert(key.to_string(), Vec::new());
                }

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

    fn remove_all_from_list(&mut self, key: &str, value: &str) {
        if let Some(prev) = self.env_get_var(key) {
            let mut prev = prev.split(':').collect::<Vec<&str>>();
            let indexes = prev
                .iter()
                .enumerate()
                .filter(|(_, &r)| r == value)
                .map(|(i, _)| i)
                .collect::<Vec<usize>>();

            // Exit early if the value is not in the list
            if indexes.is_empty() {
                return;
            }

            if !self.lists.contains_key(key) {
                self.lists.insert(key.to_string(), Vec::new());
            }

            for index in indexes.iter().rev() {
                self.lists.get_mut(key).unwrap().push(DynamicEnvListValue {
                    operation: DynamicEnvListOperation::Del,
                    value: value.to_string(),
                    index: *index,
                });

                prev.remove(*index);
            }

            self.env_set_var(key, &prev.join(":"));
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
            if operations
                .iter()
                .any(|o| o.operation == DynamicEnvListOperation::Create)
            {
                self.env_unset_var(key);
                continue;
            }

            // Load the content of the variables, as we'll need to "undo" the
            // operations we've done to the closest of our ability; since it's
            // a list, we'll also split it, so we're ready to "search and update"
            let cur_val = self.env_get_var(key).unwrap_or("".to_string());
            let mut cur_val = cur_val.split(':').collect::<Vec<&str>>();

            for operation in operations.iter().rev() {
                match operation.operation {
                    DynamicEnvListOperation::Create => {
                        unreachable!();
                    }
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
    #[serde(rename = "p", default, skip_serializing_if = "Option::is_none")]
    prev: Option<String>,
    #[serde(rename = "c", default, skip_serializing_if = "Option::is_none")]
    curr: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
enum DynamicEnvListOperation {
    #[serde(rename = "c")]
    Create,
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
    let cur_id = match cur_id {
        Some(cur_id) => cur_id,
        None => return (0, None),
    };
    let cur_data = parts.next().unwrap_or("{}");

    (cur_id, Some(cur_data.to_string()))
}

fn hex_to_id(hex: &str) -> Option<u64> {
    if hex.len() != 16 {
        return None;
    }
    match u64::from_str_radix(hex, 16) {
        Ok(cur_id) => Some(cur_id),
        Err(_) => None,
    }
}

/// This allows to dedup flags in environment variables
/// such as CFLAGS, CPPFLAGS, LDFLAGS, etc.
/// NOTE: this is not handling escaped spaces properly,
/// which means that if a path contains ` -` it will be
/// split into two different "flags". This is however a
/// very rare case, and it's not worth the effort to handle
/// for now.
fn dedup_flags(flags: Option<String>) -> Option<String> {
    if let Some(flags) = flags {
        let mut seen = HashSet::new();
        return Some(
            flags
                .split(" -")
                .filter(|f| seen.insert(f.to_string()))
                .join(" -"),
        );
    }
    None
}
