use std::collections::HashSet;

use lazy_static::lazy_static;

use crate::internal::config::config;
use crate::internal::config::global_config;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::config::OmniConfig;
use crate::internal::env::ENV;

lazy_static! {
    #[derive(Debug)]
    pub static ref OMNIPATH: Vec<String> = omnipath();
}

pub fn omnipath() -> Vec<String> {
    let config = config(".");
    omnipath_from_config(&config)
}

pub fn omnipath_entries() -> Vec<PathEntryConfig> {
    let config = config(".");
    omnipath_entries_from_config(&config)
}

// pub fn global_omnipath() -> Vec<String> {
// let config = global_config();
// omnipath_from_config(&config)
// }

pub fn global_omnipath_entries() -> Vec<PathEntryConfig> {
    let config = global_config();
    omnipath_entries_from_config(&config)
}

fn omnipath_from_config(config: &OmniConfig) -> Vec<String> {
    let mut omnipath = vec![];
    let mut omnipath_seen = HashSet::new();

    for path in &config.path.prepend {
        if path.is_valid() && omnipath_seen.insert(path.as_string()) {
            omnipath.push(path.as_string());
        }
    }

    for path in &ENV.omnipath {
        if !path.is_empty() && omnipath_seen.insert(path.clone()) {
            omnipath.push(path.clone());
        }
    }

    for path in &config.path.append {
        if path.is_valid() && omnipath_seen.insert(path.as_string()) {
            omnipath.push(path.as_string());
        }
    }

    omnipath
}

fn omnipath_entries_from_config(config: &OmniConfig) -> Vec<PathEntryConfig> {
    let mut omnipath = vec![];
    let mut omnipath_seen = HashSet::new();

    for path in &config.path.prepend {
        if path.is_valid() && omnipath_seen.insert(path.as_string()) {
            omnipath.push(path.to_owned());
        }
    }

    for path in &ENV.omnipath {
        if !path.is_empty() && omnipath_seen.insert(path.clone()) {
            let entry = PathEntryConfig::from_path(path);
            if entry.is_valid() {
                omnipath.push(entry);
            }
        }
    }

    for path in &config.path.append {
        if path.is_valid() && omnipath_seen.insert(path.as_string()) {
            omnipath.push(path.to_owned());
        }
    }

    omnipath
}
