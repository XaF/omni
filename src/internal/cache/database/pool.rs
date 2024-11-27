#[cfg(test)]
use std::collections::HashMap;
#[cfg(not(test))]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Mutex;

use lazy_static::lazy_static;
use r2d2::Pool as R2d2Pool;
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Result as SqliteResult;
use rusqlite::Row;
use thiserror::Error;

use crate::internal::cache::database::upgrade_database;
use crate::internal::cache::migration::convert_cache;
use crate::internal::cache::migration::migrate_json_to_database;
#[cfg(not(test))]
use crate::internal::config::global_config;

/// Type alias for a SQLite connection pool
pub type SqlitePool = R2d2Pool<SqliteConnectionManager>;

/// Type alias for a pooled SQLite connection
pub type SqliteConnection = PooledConnection<SqliteConnectionManager>;

cfg_if::cfg_if! {
    if #[cfg(test)] {

        lazy_static! {
            // Map of test IDs to their respective pools
            static ref TEST_POOLS: Mutex<HashMap<String, SqlitePool>> = Mutex::new(HashMap::new());
        }

        fn ready_sqlite_pool() -> SqlitePool {
            // Get the test ID from environment
            let test_id = std::env::var("TEST_POOL_ID")
                .expect("TEST_POOL_ID must be set for tests");

            let mut pools = TEST_POOLS.lock().unwrap();

            // Return existing pool if we have one for this test
            if let Some(pool) = pools.get(&test_id) {
                return pool.clone();
            }

            // Create a new pool for this test
            let manager = SqliteConnectionManager::memory();
            let pool = R2d2Pool::builder()
                .max_size(3)
                .build(manager)
                .expect("Failed to create pool");

            // Initialize the database
            let conn = pool.get().expect("Couldn't get connection from pool");
            upgrade_database(&conn).expect("Failed to upgrade database");

            // Store and return the pool
            pools.insert(test_id, pool.clone());
            pool
        }

        pub(crate) fn get_conn() -> SqliteConnection {
            ready_sqlite_pool()
                .get()
                .expect("Couldn't get connection from pool")
        }

        // Helper to clean up a test's pool
        pub(crate) fn cleanup_test_pool() {
            if let Ok(test_id) = std::env::var("TEST_POOL_ID") {
                let mut pools = TEST_POOLS.lock().unwrap();
                pools.remove(&test_id);
            }
        }
    } else {
        lazy_static! {
            static ref SQLITE_POOL: SqlitePool = {
                let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
                let db_path = cache_dir_path.join("cache.db");

                // If the database file does not exist, make sure its parent directory does
                if !db_path.exists() {
                    if let Some(parent) = db_path.parent() {
                        std::fs::create_dir_all(parent).expect("Failed to create cache directory");
                    }
                }

                let manager = SqliteConnectionManager::file(db_path);
                let pool = R2d2Pool::builder()
                    .max_size(10)
                    .build(manager)
                    .expect("Failed to create pool");

                // Apply upgrades if necessary
                let conn = pool.get().expect("Couldn't get connection from pool");
                upgrade_database(&conn).expect("Failed to upgrade database");

                pool
            };
        }

        /// Get a pooled SQLite connection
        pub(crate) fn get_conn() -> SqliteConnection {
            SQLITE_POOL
                .get()
                .expect("Couldn't get connection from pool")
        }
    }
}
