use std::collections::HashSet;

use lazy_static::lazy_static;

use crate::config::config;
use crate::config::global_config;
use crate::config::OmniConfig;
use crate::env::ENV;

lazy_static! {
    #[derive(Debug)]
    pub static ref OMNIPATH: Vec<String> = omnipath();
}

pub fn omnipath() -> Vec<String> {
    let config = config(".");
    omnipath_from_config(&config)
}

pub fn global_omnipath() -> Vec<String> {
    let config = global_config();
    omnipath_from_config(&config)
}

fn omnipath_from_config(config: &OmniConfig) -> Vec<String> {
    let mut omnipath = vec![];
    let mut omnipath_seen = HashSet::new();

    for path in &config.path.prepend {
        if !path.is_empty() && omnipath_seen.insert(path) {
            omnipath.push(path.clone());
        }
    }

    for path in &ENV.omnipath {
        if !path.is_empty() && omnipath_seen.insert(path) {
            omnipath.push(path.clone());
        }
    }

    for path in &config.path.append {
        if !path.is_empty() && omnipath_seen.insert(path) {
            omnipath.push(path.clone());
        }
    }

    omnipath
}
