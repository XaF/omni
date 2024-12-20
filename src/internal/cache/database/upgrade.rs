use rusqlite::Connection;

use crate::internal::cache::migration::convert_cache;
use crate::internal::cache::migration::migrate_json_to_database;
use crate::internal::cache::CacheManagerError;

/// Upgrade the database schema
pub fn upgrade_database(conn: &Connection) -> Result<(), CacheManagerError> {
    // Get the current version of the database
    let current_version: usize = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    // Set up for getting to version 1 and migrating
    // from the JSON files
    if current_version < 1 {
        // Make sure that we have the latest version of the JSON cache
        convert_cache()?;

        // Create the tables
        conn.execute_batch(include_str!("sql/create_tables.sql"))?;

        // Migrate the JSON files to the database
        if let Err(err) = migrate_json_to_database(conn) {
            // Delete the database file if the migration fails, so it can be retried
            if let Some(path) = conn.path() {
                std::fs::remove_file(path)?;
            }

            return Err(err);
        }
    }

    if current_version < 2 {
        conn.execute_batch(include_str!("sql/upgrade_v1_to_v2.sql"))?;
    }

    Ok(())
}
