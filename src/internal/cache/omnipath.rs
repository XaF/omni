use std::path::PathBuf;

use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::config::global_config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniPathCache {}

impl OmniPathCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn try_exclusive_update(&self) -> bool {
        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Read the current updated_at timestamp
            let updated_at: Option<(bool, Option<String>)> = tx.query_one_optional(
                include_str!("database/sql/omnipath_get_updated_at.sql"),
                params![global_config().path_repo_updates.interval],
            )?;

            let updated_at = match updated_at {
                Some((true, updated_at)) => updated_at,
                Some((false, _)) => {
                    return Err(CacheManagerError::Other("update not required".to_string()))
                }
                None => None,
            };

            // Update the updated_at timestamp
            let updated = tx.execute(
                include_str!("database/sql/omnipath_set_updated_at.sql"),
                params![updated_at],
            )?;

            Ok(updated > 0)
        })
        .unwrap_or_default()
    }

    pub fn update_error(&self, update_error_log: String) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/omnipath_set_update_error_log.sql"),
            params![update_error_log],
        )?;
        Ok(updated > 0)
    }

    pub fn try_exclusive_update_error_log(&self) -> Option<String> {
        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Read the current update_error_log
            let update_error_log: Option<String> = tx.query_one_optional(
                include_str!("database/sql/omnipath_get_update_error_log.sql"),
                params![],
            )?;

            let update_error_log = match update_error_log {
                Some(update_error_log) => update_error_log,
                None => {
                    return Err(CacheManagerError::Other(
                        "update_error_log not found".to_string(),
                    ))
                }
            };

            // Clear the update_error_log
            let deleted = tx.execute(
                include_str!("database/sql/omnipath_clear_update_error_log.sql"),
                params![],
            )?;

            if deleted == 0 {
                return Err(CacheManagerError::Other(
                    "could not delete update_error_log".to_string(),
                ));
            }

            // Check if the file is not empty
            let update_error_log = update_error_log.trim();
            if update_error_log.is_empty() {
                // We return 'Ok' because we want the transaction to commit,
                // since we've already cleared the update_error_log
                return Ok(None);
            }

            // Make sure the file exists before returning it
            let file_path = PathBuf::from(&update_error_log);
            if !file_path.exists() {
                // We return 'Ok' because we want the transaction to commit,
                // since we've already cleared the update_error_log
                return Ok(None);
            }

            // If we get here, we can return the update_error_log
            // since it is an actual file
            Ok(Some(update_error_log.to_string()))
        })
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::env::temp_dir as env_temp_dir;
    use std::fs::write as fs_write;
    use std::path::Path;

    use uuid::Uuid;

    use crate::internal::testutils::run_with_env;

    mod omnipath_cache {
        use super::*;

        fn create_fake_log_file() -> String {
            let tempdir = env_temp_dir();
            let uuid = Uuid::new_v4();
            let log_file = tempdir.as_path().join(format!("fake_error_{:x}.log", uuid));
            fs_write(&log_file, "Test error log").expect("Failed to write to log file");
            log_file.to_string_lossy().to_string()
        }

        #[test]
        fn test_try_exclusive_update() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();

                // First attempt should succeed
                assert!(cache.try_exclusive_update(), "First update should succeed");

                // Verify updated_at was set
                let db = CacheManager::get();
                let (_expired, updated_at): (bool, String) = db
                    .query_one(include_str!("database/sql/omnipath_get_updated_at.sql"), params![0])
                    .expect("Failed to get updated_at");

                assert!(!updated_at.is_empty(), "updated_at should be set");

                // Immediate second attempt should return false
                assert!(
                    !cache.try_exclusive_update(),
                    "Second immediate update should fail"
                );
            });
        }

        #[test]
        fn test_try_exclusive_update_invalid_timestamp() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();
                let db = CacheManager::get();

                // Insert invalid timestamp
                db.execute(
                    "INSERT OR REPLACE INTO metadata (key, value) VALUES ('omnipath.updated_at', ?)",
                    params!["invalid-timestamp"],
                )
                .expect("Failed to insert invalid timestamp");

                // Should handle invalid timestamp gracefully and allow update
                assert!(
                    cache.try_exclusive_update(),
                    "Should handle invalid timestamp"
                );
            });
        }

        #[test]
        fn test_null_timestamp() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();
                let db = CacheManager::get();

                // Insert NULL timestamp
                db.execute(
                    "INSERT OR REPLACE INTO metadata (key, value) VALUES ('omnipath.updated_at', NULL)",
                    params![],
                )
                .expect("Failed to insert NULL timestamp");

                // Should handle NULL timestamp gracefully
                assert!(cache.try_exclusive_update(), "Should handle NULL timestamp");
            });
        }

        #[test]
        fn test_update_error_log() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();
                let error_file = create_fake_log_file();

                // Set error log
                assert!(
                    cache
                        .update_error(error_file.clone())
                        .expect("Failed to update error log"),
                    "Setting error log should succeed"
                );

                // Verify error log was set
                let db = CacheManager::get();
                let stored_error: Option<String> = db
                    .query_one(
                        include_str!("database/sql/omnipath_get_update_error_log.sql"),
                        params![],
                    )
                    .expect("Failed to get error log");

                assert_eq!(
                    stored_error.as_deref(),
                    Some(error_file.as_str()),
                    "Stored error should match set error"
                );
            });
        }

        #[test]
        fn test_try_exclusive_update_error_log() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();
                let error_file = create_fake_log_file();

                // Initially should return None as no error is set
                assert!(
                    cache.try_exclusive_update_error_log().is_none(),
                    "Should return None when no error is set"
                );

                // Set error log
                cache
                    .update_error(error_file.clone())
                    .expect("Failed to update error log");

                // Try to exclusively get and clear error
                let retrieved_error = cache.try_exclusive_update_error_log();
                assert_eq!(
                    retrieved_error.as_deref(),
                    Some(error_file.as_str()),
                    "Retrieved error should match set error"
                );

                // Error should be cleared now
                let db = CacheManager::get();
                let stored_error: Option<String> = db
                    .query_one_optional(
                        include_str!("database/sql/omnipath_get_update_error_log.sql"),
                        params![],
                    )
                    .expect("Failed to get error log");

                assert!(stored_error.is_none(), "Error log should be cleared");

                // Second attempt should return None
                assert!(
                    cache.try_exclusive_update_error_log().is_none(),
                    "Second attempt should return None as error was cleared"
                );
            });
        }

        #[test]
        fn test_try_exclusive_update_error_log_empty() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();
                let db = CacheManager::get();

                // Add empty error log
                db.execute(
                    "INSERT OR REPLACE INTO metadata (key, value) VALUES ('omnipath.update_error_log', '')",
                    params![],
                ).expect("Failed to insert empty error log");

                // Check that the value is stored
                let stored_error: Option<String> = db
                    .query_one(
                        include_str!("database/sql/omnipath_get_update_error_log.sql"),
                        params![],
                    )
                    .expect("Failed to get error log");
                assert_eq!(
                    stored_error.as_deref(),
                    Some(""),
                    "Stored error should be empty"
                );

                // Check that None gets returned
                assert!(
                    cache.try_exclusive_update_error_log().is_none(),
                    "Should return None when empty error is set"
                );

                // Check that the entry is not stored anymore
                let stored_error: Option<String> = db
                    .query_one_optional(
                        include_str!("database/sql/omnipath_get_update_error_log.sql"),
                        params![],
                    )
                    .expect("Failed to get error log");
                assert!(stored_error.is_none(), "Error log should be cleared");
            });
        }

        #[test]
        fn test_try_exclusive_update_error_log_not_exists() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();
                let db = CacheManager::get();

                // Make sure we have a file that does not exist, or it will fail
                // the test for the wrong reasons
                let error_file_base = "/this/file/does/not/exist.log";
                let mut error_file = error_file_base.to_string();
                while Path::new(&error_file).exists() {
                    error_file = format!("{}.{:x}", error_file_base, Uuid::new_v4());
                }

                // Store the entry in the cache
                assert!(
                    cache.update_error(error_file.clone()).is_ok(),
                    "Failed to update error log"
                );

                // Check that the value is stored
                let stored_error: Option<String> = db
                    .query_one(
                        include_str!("database/sql/omnipath_get_update_error_log.sql"),
                        params![],
                    )
                    .expect("Failed to get error log");
                assert_eq!(
                    stored_error.as_deref(),
                    Some(error_file.as_str()),
                    "Stored error should match set error"
                );

                // Check that None gets returned
                assert!(
                    cache.try_exclusive_update_error_log().is_none(),
                    "Should return None when error log does not exist"
                );

                // Check that the entry is not stored anymore
                let stored_error: Option<String> = db
                    .query_one_optional(
                        include_str!("database/sql/omnipath_get_update_error_log.sql"),
                        params![],
                    )
                    .expect("Failed to get error log");
                assert!(stored_error.is_none(), "Error log should be cleared");
            });
        }

        #[test]
        fn test_sequential_error_log_access() {
            run_with_env(&[], || {
                let cache1 = OmniPathCache::get();
                let cache2 = OmniPathCache::get();
                let error_file = create_fake_log_file();

                // Set error using first cache instance
                cache1
                    .update_error(error_file.clone())
                    .expect("Failed to update error log");

                // First instance gets and clears the error
                let error1 = cache1.try_exclusive_update_error_log();
                assert_eq!(
                    error1.as_deref(),
                    Some(error_file.as_str()),
                    "First instance should get error"
                );

                // Second instance should get None as error was cleared
                let error2 = cache2.try_exclusive_update_error_log();
                assert!(error2.is_none(), "Second instance should get None");
            });
        }

        #[test]
        fn test_update_error_overwrite() {
            run_with_env(&[], || {
                let cache = OmniPathCache::get();

                // Set first error
                let error1 = create_fake_log_file();
                cache
                    .update_error(error1.clone())
                    .expect("Failed to update first error");

                // Set second error
                let error2 = create_fake_log_file();
                cache
                    .update_error(error2.clone())
                    .expect("Failed to update second error");

                // Verify only latest error is stored
                let retrieved_error = cache.try_exclusive_update_error_log();
                assert_eq!(
                    retrieved_error.as_deref(),
                    Some(error2.as_str()),
                    "Should only retrieve latest error"
                );
            });
        }
    }
}
