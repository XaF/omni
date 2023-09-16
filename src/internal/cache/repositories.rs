use std::collections::BTreeSet;
use std::io;

use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_repositories_cache;
use crate::internal::cache::loaders::set_repositories_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;

const REPOSITORIES_CACHE_NAME: &str = "repositories";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepositoriesCache {
    #[serde(default = "BTreeSet::new", skip_serializing_if = "BTreeSet::is_empty")]
    pub trusted: BTreeSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl RepositoriesCache {
    pub fn has_trusted(&self, repository: &str) -> bool {
        self.trusted.contains(&repository.to_string())
    }

    pub fn add_trusted(&mut self, repository: &str) -> bool {
        if !self.has_trusted(repository) {
            self.trusted.insert(repository.to_string());
            self.updated_at = OffsetDateTime::now_utc();
            true
        } else {
            false
        }
    }
}

impl Empty for RepositoriesCache {
    fn is_empty(&self) -> bool {
        self.trusted.is_empty()
    }
}

impl CacheObject for RepositoriesCache {
    fn new_empty() -> Self {
        Self {
            trusted: BTreeSet::new(),
            updated_at: utils::origin_of_time(),
        }
    }

    fn get() -> Self {
        get_repositories_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(REPOSITORIES_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(
            REPOSITORIES_CACHE_NAME,
            processing_fn,
            set_repositories_cache,
        )
    }
}
