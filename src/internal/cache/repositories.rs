use std::collections::BTreeSet;
use std::collections::HashMap;
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
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub fingerprints: HashMap<String, RepositoryFingerprints>,
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

    pub fn check_fingerprint(
        &self,
        repository: &str,
        fingerprint_type: &str,
        fingerprint: u64,
    ) -> bool {
        let cur_fingerprint = match self.fingerprints.get(repository) {
            Some(repo) => repo.get(fingerprint_type),
            None => 0,
        };

        cur_fingerprint == fingerprint
    }

    pub fn update_fingerprint(
        &mut self,
        repository: &str,
        fingerprint_type: &str,
        fingerprint: u64,
    ) -> bool {
        if self.check_fingerprint(repository, fingerprint_type, fingerprint) {
            return false;
        }

        if let None = self.fingerprints.get(repository) {
            if fingerprint == 0 {
                return false;
            }
            self.fingerprints
                .insert(repository.to_string(), RepositoryFingerprints::new());
        }

        if let Some(repo) = self.fingerprints.get_mut(repository) {
            let updated = if fingerprint == 0 {
                let mut updated = repo.fingerprints.remove(fingerprint_type).is_some();

                if repo.is_empty() {
                    self.fingerprints.remove(repository);
                    updated = true;
                }

                updated
            } else {
                repo.fingerprints
                    .insert(fingerprint_type.to_string(), fingerprint);
                true
            };

            if updated {
                self.updated_at = OffsetDateTime::now_utc();
                return true;
            }

            return false;
        }

        unreachable!();
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
            fingerprints: HashMap::new(),
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepositoryFingerprints {
    #[serde(
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty",
        flatten
    )]
    fingerprints: HashMap<String, u64>,
}

impl RepositoryFingerprints {
    pub fn new() -> Self {
        Self {
            fingerprints: HashMap::new(),
        }
    }

    pub fn get(&self, fingerprint: &str) -> u64 {
        if let Some(fingerprint) = self.fingerprints.get(fingerprint) {
            *fingerprint
        } else {
            0
        }
    }
}

impl Empty for RepositoryFingerprints {
    fn is_empty(&self) -> bool {
        self.fingerprints.is_empty()
    }
}
