use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkdirsCache {}

impl WorkdirsCache {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::testutils::run_with_env_and_cache;

    mod workdirs_cache {
        use super::*;

        // Helper function to generate test workdir trust IDs
        fn test_trust_id(index: u32) -> String {
            format!("trust_{}_{:x}", index, index * 31)
        }

        #[test]
        fn test_trusted_workdir_operations() {
            run_with_env_and_cache(&[], || {
                let mut cache = WorkdirsCache::get();
                let workdir = test_trust_id(1);

                // Initially should not be trusted
                assert!(
                    !cache.has_trusted(&workdir),
                    "New workdir should not be trusted"
                );

                // Add to trusted
                assert!(
                    cache
                        .add_trusted(&workdir)
                        .expect("Failed to add trusted workdir"),
                    "Adding trusted workdir should succeed"
                );

                // Should now be trusted
                assert!(cache.has_trusted(&workdir), "Workdir should now be trusted");

                // Adding again should succeed but return false (no new row)
                assert!(
                    !cache
                        .add_trusted(&workdir)
                        .expect("Failed to add trusted workdir again"),
                    "Re-adding trusted workdir should return false"
                );

                // Remove from trusted
                assert!(
                    cache
                        .remove_trusted(&workdir)
                        .expect("Failed to remove trusted workdir"),
                    "Removing trusted workdir should succeed"
                );

                // Should no longer be trusted
                assert!(
                    !cache.has_trusted(&workdir),
                    "Workdir should no longer be trusted"
                );

                // Removing again should succeed but return false (no row affected)
                assert!(
                    !cache
                        .remove_trusted(&workdir)
                        .expect("Failed to remove trusted workdir again"),
                    "Re-removing trusted workdir should return false"
                );
            });
        }

        #[test]
        fn test_fingerprint_operations() {
            run_with_env_and_cache(&[], || {
                let mut cache = WorkdirsCache::get();
                let workdir = test_trust_id(2);
                let fp_type = "test_type";
                let fingerprint = 12345u64;

                // Initially should not match any fingerprint
                assert!(
                    !cache.check_fingerprint(&workdir, fp_type, fingerprint),
                    "Should not match non-existent fingerprint"
                );

                // Add fingerprint
                assert!(
                    cache
                        .update_fingerprint(&workdir, fp_type, fingerprint)
                        .expect("Failed to update fingerprint"),
                    "Adding fingerprint should succeed"
                );

                // Should match exact fingerprint
                assert!(
                    cache.check_fingerprint(&workdir, fp_type, fingerprint),
                    "Should match exact fingerprint"
                );

                // Should not match different fingerprint
                assert!(
                    !cache.check_fingerprint(&workdir, fp_type, fingerprint + 1),
                    "Should not match different fingerprint"
                );

                // Update fingerprint
                let new_fingerprint = 67890u64;
                assert!(
                    cache
                        .update_fingerprint(&workdir, fp_type, new_fingerprint)
                        .expect("Failed to update fingerprint"),
                    "Updating fingerprint should succeed"
                );

                // Should match new fingerprint
                assert!(
                    cache.check_fingerprint(&workdir, fp_type, new_fingerprint),
                    "Should match new fingerprint"
                );

                // Remove fingerprint with 0
                assert!(
                    cache
                        .update_fingerprint(&workdir, fp_type, 0)
                        .expect("Failed to remove fingerprint"),
                    "Removing fingerprint should succeed"
                );

                // Should not match any fingerprint after removal
                assert!(
                    !cache.check_fingerprint(&workdir, fp_type, new_fingerprint),
                    "Should not match after removal"
                );
            });
        }

        #[test]
        fn test_multiple_fingerprint_types() {
            run_with_env_and_cache(&[], || {
                let mut cache = WorkdirsCache::get();
                let workdir = test_trust_id(3);
                let fp_types = ["type1", "type2", "type3"];
                let fingerprints = [111u64, 222u64, 333u64];

                // Add different fingerprint types
                for (fp_type, &fingerprint) in fp_types.iter().zip(fingerprints.iter()) {
                    assert!(
                        cache
                            .update_fingerprint(&workdir, fp_type, fingerprint)
                            .expect("Failed to update fingerprint"),
                        "Adding fingerprint should succeed"
                    );
                }

                // Check each type independently
                for (fp_type, &fingerprint) in fp_types.iter().zip(fingerprints.iter()) {
                    assert!(
                        cache.check_fingerprint(&workdir, fp_type, fingerprint),
                        "Should match correct fingerprint for type"
                    );

                    // Should not match with wrong type
                    assert!(
                        !cache.check_fingerprint(&workdir, "wrong_type", fingerprint),
                        "Should not match fingerprint with wrong type"
                    );
                }
            });
        }

        #[test]
        fn test_multiple_workdirs() {
            run_with_env_and_cache(&[], || {
                let mut cache = WorkdirsCache::get();
                let workdirs: Vec<String> = (4..7).map(test_trust_id).collect();
                let fp_type = "test_type";
                let fingerprint = 12345u64;

                // Add fingerprints for different workdirs
                for workdir in &workdirs {
                    assert!(
                        cache
                            .update_fingerprint(workdir, fp_type, fingerprint)
                            .expect("Failed to update fingerprint"),
                        "Adding fingerprint should succeed"
                    );

                    // Also mark as trusted
                    assert!(
                        cache
                            .add_trusted(workdir)
                            .expect("Failed to add trusted workdir"),
                        "Adding trusted workdir should succeed"
                    );
                }

                // Check each workdir independently
                for workdir in &workdirs {
                    assert!(
                        cache.check_fingerprint(workdir, fp_type, fingerprint),
                        "Should match fingerprint for correct workdir"
                    );
                    assert!(
                        cache.has_trusted(workdir),
                        "Should be trusted for correct workdir"
                    );

                    // Should not match with wrong workdir
                    assert!(
                        !cache.check_fingerprint("invalid_trust_id", fp_type, fingerprint),
                        "Should not match fingerprint with wrong workdir"
                    );
                    assert!(
                        !cache.has_trusted("invalid_trust_id"),
                        "Should not be trusted for wrong workdir"
                    );
                }
            });
        }

        #[test]
        fn test_zero_fingerprint_behavior() {
            run_with_env_and_cache(&[], || {
                let mut cache = WorkdirsCache::get();
                let workdir = test_trust_id(8);
                let fp_type = "test_type";

                // Case 1: No fingerprint exists yet
                // Should return true when checking for fingerprint 0
                assert!(
                    cache.check_fingerprint(&workdir, fp_type, 0),
                    "Should match zero fingerprint when no fingerprint exists"
                );

                // Case 2: Add non-zero fingerprint
                assert!(
                    cache
                        .update_fingerprint(&workdir, fp_type, 12345)
                        .expect("Failed to update fingerprint"),
                    "Adding non-zero fingerprint should succeed"
                );

                // Should return false when checking for fingerprint 0
                assert!(
                    !cache.check_fingerprint(&workdir, fp_type, 0),
                    "Should not match zero fingerprint when non-zero fingerprint exists"
                );

                // Case 3: Update to zero fingerprint
                let db = CacheManager::get();
                db.execute(
                    concat!(
                        "UPDATE workdir_fingerprints SET fingerprint = 0 ",
                        "WHERE workdir_id = ?1 AND fingerprint_type = ?2",
                    ),
                    params![workdir, fp_type],
                )
                .expect("Failed to update to zero fingerprint");

                // Should return true when checking for fingerprint 0
                assert!(
                    cache.check_fingerprint(&workdir, fp_type, 0),
                    "Should match zero fingerprint after setting to zero"
                );

                // Should not match non-zero fingerprint
                assert!(
                    !cache.check_fingerprint(&workdir, fp_type, 12345),
                    "Should not match non-zero fingerprint when zero is set"
                );

                // Case 4: Remove fingerprint (also using 0)
                assert!(
                    cache
                        .update_fingerprint(&workdir, fp_type, 0)
                        .expect("Failed to remove fingerprint"),
                    "Removing fingerprint should succeed"
                );

                // Should return true when checking for fingerprint 0
                assert!(
                    cache.check_fingerprint(&workdir, fp_type, 0),
                    "Should match zero fingerprint after removal"
                );
            });
        }
    }
}
