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

    // pub fn should_update(&self) -> bool {
    // let db = CacheManager::get();
    // let should_update: bool = db
    // .query_row(
    // include_str!("sql/omnipath_should_update.sql"),
    // params![global_config().path_repo_updates.interval],
    // |row| row.get(0),
    // )
    // .unwrap_or(true);
    // should_update
    // }

    pub fn try_exclusive_update(&self) -> bool {
        let mut db = CacheManager::get();
        match db.transaction(|tx| {
            // Read the current updated_at timestamp
            let updated_at: Option<(bool, Option<String>)> = tx.query_one_optional(
                include_str!("sql/omnipath_get_updated_at.sql"),
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
                include_str!("sql/omnipath_set_updated_at.sql"),
                params![updated_at],
            )?;

            Ok(updated > 0)
        }) {
            Ok(updated) => updated,
            Err(_err) => false,
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
            let update_error_log: Option<String> = tx.query_one_optional(
                include_str!("sql/omnipath_get_update_error_log.sql"),
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
                include_str!("sql/omnipath_clear_update_error_log.sql"),
                params![],
            )?;

            if deleted == 0 {
                return Err(CacheManagerError::Other(
                    "could not delete update_error_log".to_string(),
                ));
            }

            Ok(update_error_log)
        }) {
            Ok(update_error_log) => Some(update_error_log),
            Err(_err) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::internal::testutils::run_with_env_and_cache;

    mod omnipath_cache {
        use super::*;

        #[test]
        fn test_try_exclusive_update() {
            run_with_env_and_cache(&[], || {
                let cache = OmniPathCache::get();

                // First attempt should succeed
                assert!(cache.try_exclusive_update(), "First update should succeed");

                // Verify updated_at was set
                let db = CacheManager::get();
                let (_expired, updated_at): (bool, String) = db
                    .query_one(include_str!("sql/omnipath_get_updated_at.sql"), params![0])
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
            run_with_env_and_cache(&[], || {
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
            run_with_env_and_cache(&[], || {
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
            run_with_env_and_cache(&[], || {
                let mut cache = OmniPathCache::get();
                let error_msg = "Test error message".to_string();

                // Set error log
                assert!(
                    cache
                        .update_error(error_msg.clone())
                        .expect("Failed to update error log"),
                    "Setting error log should succeed"
                );

                // Verify error log was set
                let db = CacheManager::get();
                let stored_error: Option<String> = db
                    .query_one(
                        include_str!("sql/omnipath_get_update_error_log.sql"),
                        params![],
                    )
                    .expect("Failed to get error log");

                assert_eq!(
                    stored_error.as_deref(),
                    Some(error_msg.as_str()),
                    "Stored error should match set error"
                );
            });
        }

        #[test]
        fn test_try_exclusive_update_error_log() {
            run_with_env_and_cache(&[], || {
                let mut cache = OmniPathCache::get();
                let error_msg = "Test error message".to_string();

                // Initially should return None as no error is set
                assert!(
                    cache.try_exclusive_update_error_log().is_none(),
                    "Should return None when no error is set"
                );

                // Set error log
                cache
                    .update_error(error_msg.clone())
                    .expect("Failed to update error log");

                // Try to exclusively get and clear error
                let retrieved_error = cache.try_exclusive_update_error_log();
                assert_eq!(
                    retrieved_error.as_deref(),
                    Some(error_msg.as_str()),
                    "Retrieved error should match set error"
                );

                // Error should be cleared now
                let db = CacheManager::get();
                let stored_error: Option<String> = db
                    .query_one_optional(
                        include_str!("sql/omnipath_get_update_error_log.sql"),
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
        fn test_concurrent_error_log_access() {
            run_with_env_and_cache(&[], || {
                let mut cache1 = OmniPathCache::get();
                let cache2 = OmniPathCache::get();
                let error_msg = "Test error message".to_string();

                // Set error using first cache instance
                cache1
                    .update_error(error_msg.clone())
                    .expect("Failed to update error log");

                // First instance gets and clears the error
                let error1 = cache1.try_exclusive_update_error_log();
                assert_eq!(
                    error1.as_deref(),
                    Some(error_msg.as_str()),
                    "First instance should get error"
                );

                // Second instance should get None as error was cleared
                let error2 = cache2.try_exclusive_update_error_log();
                assert!(error2.is_none(), "Second instance should get None");
            });
        }

        #[test]
        fn test_update_error_overwrite() {
            run_with_env_and_cache(&[], || {
                let mut cache = OmniPathCache::get();

                // Set first error
                let error1 = "First error".to_string();
                cache
                    .update_error(error1.clone())
                    .expect("Failed to update first error");

                // Set second error
                let error2 = "Second error".to_string();
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

        #[test]
        fn test_error_log_empty_string() {
            run_with_env_and_cache(&[], || {
                let mut cache = OmniPathCache::get();

                // Set empty error message
                assert!(
                    cache
                        .update_error("".to_string())
                        .expect("Failed to update error log"),
                    "Should be able to set empty error message"
                );

                // Try to retrieve it
                let retrieved_error = cache.try_exclusive_update_error_log();
                assert_eq!(
                    retrieved_error.as_deref(),
                    Some(""),
                    "Should be able to retrieve empty error message"
                );
            });
        }
    }
}
