use std::collections::HashSet;

use lazy_static::lazy_static;

use crate::internal::config::config;
use crate::internal::env::ENV;

lazy_static! {
    #[derive(Debug)]
    pub static ref OMNIPATH: Vec<String> = omnipath();
}

pub fn omnipath() -> Vec<String> {
    let mut omnipath = vec![];
    let mut omnipath_seen = HashSet::new();
    let config = config(".");

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
