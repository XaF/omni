use std::collections::HashMap;

use std::sync::Mutex;

use lazy_static::lazy_static;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::config_loader;
use crate::internal::config::flush_config_loader;
use crate::internal::config::OmniConfig;
use crate::internal::workdir;

lazy_static! {
    #[derive(Debug, Serialize, Deserialize, Clone)]
    static ref CONFIG_PER_PATH: Mutex<OmniConfigPerPath> = Mutex::new(OmniConfigPerPath::new());
}

pub fn config(path: &str) -> OmniConfig {
    let path = if path == "/" {
        path.to_owned()
    } else {
        std::fs::canonicalize(path)
            .unwrap_or(path.to_owned().into())
            .to_str()
            .unwrap()
            .to_owned()
    };

    let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
    config_per_path.get(&path).clone()
}

pub fn flush_config(path: &str) {
    if path == "/" {
        flush_config_loader("/");

        let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
        config_per_path.config.clear();

        return;
    }

    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();

    // Flush the config loader for the path
    flush_config_loader(&path);

    // Then flush the configuration
    let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
    config_per_path.config.remove(&path);
}

pub fn global_config() -> OmniConfig {
    config("/")
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniConfigPerPath {
    config: HashMap<String, OmniConfig>,
}

impl OmniConfigPerPath {
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &OmniConfig {
        // Get the git root path, if any
        let key = if path == "/" {
            path.to_owned()
        } else {
            let wd = workdir(path);
            if let Some(wd_root) = wd.root() {
                wd_root.to_owned()
            } else {
                path.to_owned()
            }
        };

        // Get the config for the path
        if !self.config.contains_key(&key) {
            let config_loader = config_loader(&key);
            let new_config: OmniConfig = config_loader.into();
            self.config.insert(key.clone(), new_config);
        }

        self.config.get(&key).unwrap()
    }
}
