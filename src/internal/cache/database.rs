use std::path::PathBuf;

use rusqlite::Connection;
use thiserror::Error;

use crate::internal::cache::migration::convert_cache;
use crate::internal::cache::migration::migrate_json_to_database;
use crate::internal::config::global_config;

const CREATE_SQL: &str = include_str!("schemas/create.sql");

#[derive(Error, Debug)]
pub enum CacheManagerError {
    #[error("SQL error: {0}")]
    SqlError(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

pub struct CacheManager {
    pub conn: Connection,
}

impl CacheManager {
    pub fn get() -> Self {
        Self::new().expect("Failed to create cache manager")
    }

    fn new() -> Result<Self, CacheManagerError> {
        let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
        let db_path = cache_dir_path.join("cache.db");

        let conn = Connection::open(db_path)?;

        let manager = CacheManager { conn };
        manager.upgrade_database()?;

        Ok(manager)
    }

    fn upgrade_database(&self) -> Result<(), CacheManagerError> {
        // Get the current version of the database
        let current_version: usize = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        // Set up for getting to version 1 and migrating
        // from the JSON files
        if current_version == 0 {
            // Make sure that we have the latest version of the JSON cache
            convert_cache()?;

            // Create the tables
            self.conn.execute_batch(CREATE_SQL)?;

            // Migrate the JSON files to the database
            migrate_json_to_database(&self.conn)?;
        }

        // Run the migration SQL, which should handle any migrations
        // of schemas from any version of the db to the latest
        // self.conn.execute_batch(MIGRATE_SQL)?;

        Ok(())
    }
}
