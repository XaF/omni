use std::io;

use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;
use time::Duration;
use time::OffsetDateTime;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_omnipath_cache;
use crate::internal::cache::loaders::set_omnipath_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Expires;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::cache::CacheObject;
use crate::internal::config::global_config;

const OMNIPATH_CACHE_NAME: &str = "omnipath";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniPathCache {
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub updated: bool,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    #[serde(default = "String::new", skip_serializing_if = "String::is_empty")]
    pub update_error_log: String,
}

impl OmniPathCache {
    pub fn should_update(&self) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("sql/omnipath_should_update.sql"),
                params![global_config().path_repo_updates.interval],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn try_exclusive_update(&self) -> bool {
        let mut db = CacheManager::get();
        match db.transaction(|tx| {
            // Read the current updated_at timestamp
            let updated_at: String =
                tx.query_one(include_str!("sql/omnipath_get_updated_at.sql"), params![])?;

            // Update the updated_at timestamp
            let updated = tx.execute(
                include_str!("sql/omnipath_set_updated_at.sql"),
                params![updated_at],
            )?;

            Ok(updated > 0)
        }) {
            Ok(updated) => updated,
            Err(err) => {
                eprintln!("DEBUG: try_exclusive_update: ERROR: {:?}", err);
                false
            }
        }
    }

    pub fn update_error(&mut self, update_error_log: String) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/omnipath_set_update_error_log.sql"),
            params![update_error_log],
        )?;
        Ok(updated > 0)
    }

    pub fn try_exclusive_update_error_log(&self) -> Option<String> {
        let mut db = CacheManager::get();
        match db.transaction(|tx| {
            // Read the current update_error_log
            let update_error_log: String = tx.query_one(
                include_str!("sql/omnipath_get_update_error_log.sql"),
                params![],
            )?;

            // Clear the update_error_log
            let deleted = tx.execute(
                include_str!("sql/omnipath_clear_update_error_log.sql"),
                params![],
            )?;

            // Check if the deletion worked
            Ok(update_error_log)
        }) {
            Ok(update_error_log) => Some(update_error_log),
            Err(err) => {
                eprintln!("DEBUG: try_exclusive_update_error_log: ERROR: {:?}", err);
                None
            }
        }
    }
}

impl Expires for OmniPathCache {
    fn expired(&self) -> bool {
        self.expires_at < OffsetDateTime::now_utc()
    }
}

impl CacheObject for OmniPathCache {
    fn new_empty() -> Self {
        Self {
            updated: false,
            updated_at: utils::origin_of_time(),
            expires_at: utils::origin_of_time(),
            update_error_log: "".to_string(),
        }
    }

    fn get() -> Self {
        get_omnipath_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(OMNIPATH_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(OMNIPATH_CACHE_NAME, processing_fn, set_omnipath_cache)
    }
}
