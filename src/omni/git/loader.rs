// use std::io::BufRead;
// use std::sync::Mutex;
// use std::process::exit;
// use std::collections::HashSet;

use lazy_static::lazy_static;

// use crate::config::CONFIG;

lazy_static! {
    #[derive(Debug)]
    pub static ref REPO_LOADER: RepoLoader = RepoLoader::new();
}


#[derive(Debug, Clone)]
pub struct RepoLoader {
    // pub repos: Vec<Repo>,
}

impl RepoLoader {
    pub fn new() -> Self {
        // let mut repos = vec![];

        // Self {
            // repos: repos,
        // }
        Self {}
    }
}
