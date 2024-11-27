use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepositoriesCache {}

impl RepositoriesCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn has_trusted(&self, workdir: &str) -> bool {
        let db = CacheManager::get();
        let trusted: bool = db
            .query_one(
                include_str!("sql/workdir_trusted_check.sql"),
                params![workdir],
            )
            .unwrap_or(false);
        trusted
    }

    pub fn add_trusted(&mut self, workdir: &str) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("sql/workdir_trusted_add.sql"),
            params![workdir],
        )?;
        Ok(inserted > 0)
    }

    pub fn remove_trusted(&mut self, workdir: &str) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let removed = db.execute(
            include_str!("sql/workdir_trusted_remove.sql"),
            params![workdir],
        )?;
        Ok(removed > 0)
    }

    pub fn check_fingerprint(
        &self,
        workdir: &str,
        fingerprint_type: &str,
        fingerprint: u64,
    ) -> bool {
        let db = CacheManager::get();
        let fingerprint_matches: bool = db
            .query_one(
                include_str!("sql/workdir_fingerprint_check.sql"),
                params![workdir, fingerprint_type, fingerprint],
            )
            .unwrap_or(false);
        fingerprint_matches
    }

    pub fn update_fingerprint(
        &mut self,
        workdir: &str,
        fingerprint_type: &str,
        fingerprint: u64,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let changed = if fingerprint == 0 {
            db.execute(
                include_str!("sql/workdir_fingerprint_remove.sql"),
                params![workdir, fingerprint_type],
            )?
        } else {
            db.execute(
                include_str!("sql/workdir_fingerprint_upsert.sql"),
                params![workdir, fingerprint_type, fingerprint],
            )?
        };
        Ok(changed > 0)
    }
}
