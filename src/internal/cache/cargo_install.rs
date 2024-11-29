use rusqlite::params;
use rusqlite::Row;
use serde::Deserialize;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::config::global_config;
use crate::internal::env::now as omni_now;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoInstallOperationCache {}

impl CargoInstallOperationCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn add_versions(
        &self,
        crate_name: &str,
        versions: &CargoInstallVersions,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/cargo_install_operation_add_versions.sql"),
            params![crate_name, serde_json::to_string(&versions.versions)?],
        )?;
        Ok(inserted > 0)
    }

    pub fn get_versions(&self, crate_name: &str) -> Option<CargoInstallVersions> {
        let db = CacheManager::get();
        let versions: Option<CargoInstallVersions> = db
            .query_one(
                include_str!("database/sql/cargo_install_operation_get_versions.sql"),
                params![crate_name],
            )
            .ok();
        versions
    }

    pub fn add_installed(
        &self,
        crate_name: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/cargo_install_operation_add.sql"),
            params![crate_name, version],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_required_by(
        &self,
        env_version_id: &str,
        crate_name: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/cargo_install_operation_add_required_by.sql"),
            params![crate_name, version, env_version_id],
        )?;
        Ok(inserted > 0)
    }

    pub fn list_installed(&self) -> Result<Vec<CargoInstalled>, CacheManagerError> {
        let db = CacheManager::get();
        let installed: Vec<CargoInstalled> = db.query_as(
            include_str!("database/sql/cargo_install_operation_list_installed.sql"),
            params![],
        )?;
        Ok(installed)
    }

    pub fn cleanup(&self) -> Result<(), CacheManagerError> {
        let config = global_config();
        let db = CacheManager::get();

        db.execute(
            include_str!("database/sql/cargo_install_operation_cleanup_installed.sql"),
            params![&config.cache.cargo_install.cleanup_after],
        )?;

        db.execute(
            include_str!("database/sql/cargo_install_operation_cleanup_versions.sql"),
            params![&config.cache.cargo_install.versions_retention],
        )?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoInstalled {
    pub crate_name: String,
    pub version: String,
}

impl FromRow for CargoInstalled {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        Ok(Self {
            crate_name: row.get("crate")?,
            version: row.get("version")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoInstallVersions {
    #[serde(alias = "Versions")]
    pub versions: Vec<String>,
    #[serde(default = "OffsetDateTime::now_utc", with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

impl CargoInstallVersions {
    pub fn new(versions: Vec<String>) -> Self {
        Self {
            versions,
            fetched_at: omni_now(),
        }
    }

    pub fn is_fresh(&self) -> bool {
        self.fetched_at >= omni_now()
    }

    pub fn is_stale(&self, ttl: u64) -> bool {
        let duration = time::Duration::seconds(ttl as i64);
        self.fetched_at + duration < OffsetDateTime::now_utc()
    }
}

impl FromRow for CargoInstallVersions {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        let versions_str: String = row.get("versions")?;
        let versions: Vec<String> = serde_json::from_str(&versions_str)?;

        let fetched_at_str: String = row.get("fetched_at")?;
        let fetched_at: OffsetDateTime = OffsetDateTime::parse(&fetched_at_str, &Rfc3339)?;

        Ok(Self {
            versions,
            fetched_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::internal::cache::database::get_conn;
    use crate::internal::testutils::run_with_env;

    mod cargo_install_operation_cache {
        use super::*;
        use time::OffsetDateTime;

        #[test]
        fn test_add_and_get_versions() {
            run_with_env(&[], || {
                let cache = CargoInstallOperationCache::get();
                let crate_name = "github.com/test/pkg";

                // Create test versions
                let versions = CargoInstallVersions {
                    versions: vec!["v1.0.0".to_string(), "v1.1.0".to_string()],
                    fetched_at: OffsetDateTime::now_utc(),
                };

                // Test adding versions
                assert!(cache
                    .add_versions(crate_name, &versions)
                    .expect("Failed to add versions"));

                // Test retrieving versions
                let retrieved = cache
                    .get_versions(crate_name)
                    .expect("Failed to get versions");
                assert_eq!(retrieved.versions.len(), 2);
                assert!(retrieved.versions.contains(&"v1.0.0".to_string()));
                assert!(retrieved.versions.contains(&"v1.1.0".to_string()));

                // Test retrieving non-existent package
                let non_existent = cache.get_versions("non/existent");
                assert!(non_existent.is_none());
            });
        }

        #[test]
        fn test_add_and_list_installed() {
            run_with_env(&[], || {
                let cache = CargoInstallOperationCache::get();
                let crate_name = "github.com/test/pkg";
                let version = "v1.0.0";

                // Test adding installed version
                assert!(cache
                    .add_installed(crate_name, version)
                    .expect("Failed to add installed version"));

                // Test listing installed versions
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), 1);
                assert_eq!(installed[0].crate_name, crate_name);
                assert_eq!(installed[0].version, version);

                // Test adding duplicate installed version
                assert!(cache
                    .add_installed(crate_name, version)
                    .expect("Failed to add duplicate installed version"));

                // Verify no duplicates in list
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), 1);
            });
        }

        #[test]
        fn test_add_required_by() {
            run_with_env(&[], || {
                let cache = CargoInstallOperationCache::get();
                let crate_name = "github.com/test/pkg";
                let version = "v1.0.0";
                let env_version_id = "test-env-id";

                // Add environment version first for foreign key constraint
                let conn = get_conn();
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![env_version_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");

                // Try adding required_by without installed - should fail
                let result = cache.add_required_by(env_version_id, crate_name, version);
                assert!(result.is_err(), "Should fail without installed version");

                // Add installed version
                cache
                    .add_installed(crate_name, version)
                    .expect("Failed to add installed version");

                // Now add required_by - should succeed
                assert!(cache
                    .add_required_by(env_version_id, crate_name, version)
                    .expect("Failed to add required by relationship"));

                // Verify the relationship exists
                let required_exists: bool = conn
                    .query_row(
                        concat!(
                            "SELECT EXISTS(",
                            "  SELECT 1 FROM cargo_install_required_by ",
                            "  WHERE crate = ?1 AND version = ?2 AND env_version_id = ?3",
                            ")",
                        ),
                        params![crate_name, version, env_version_id],
                        |row| row.get(0),
                    )
                    .expect("Failed to query required by relationship");
                assert!(required_exists);
            });
        }

        #[test]
        fn test_multiple_required_by() {
            run_with_env(&[], || {
                let cache = CargoInstallOperationCache::get();
                let crate_name = "github.com/test/pkg";
                let version = "v1.0.0";
                let env_version_ids = vec!["env-1", "env-2", "env-3"];

                // Add installed version first
                cache
                    .add_installed(crate_name, version)
                    .expect("Failed to add installed version");

                // Add environments
                let conn = get_conn();
                for env_id in &env_version_ids {
                    conn.execute(
                        include_str!("database/sql/up_environments_insert_env_version.sql"),
                        params![env_id, "{}", "[]", "[]", "{}", "hash"],
                    )
                    .expect("Failed to add environment version");
                }

                // Add requirements for each environment
                for env_id in &env_version_ids {
                    assert!(cache
                        .add_required_by(env_id, crate_name, version)
                        .expect("Failed to add requirement"));
                }

                // Verify requirements
                for env_id in &env_version_ids {
                    let required: bool = conn
                        .query_row(
                            concat!(
                                "SELECT EXISTS(",
                                "  SELECT 1 FROM cargo_install_required_by ",
                                "  WHERE crate = ?1 AND version = ?2 AND env_version_id = ?3",
                                ")",
                            ),
                            params![crate_name, version, env_id],
                            |row| row.get(0),
                        )
                        .expect("Failed to query requirement");
                    assert!(required, "Requirement for {} should exist", env_id);
                }
            });
        }

        #[test]
        fn test_cleanup() {
            run_with_env(&[], || {
                let cache = CargoInstallOperationCache::get();

                // Add two packages
                let pkg1 = "github.com/test/pkg1";
                let pkg2 = "github.com/test/pkg2";
                let version = "v1.0.0";

                // Add installations
                cache
                    .add_installed(pkg1, version)
                    .expect("Failed to add pkg1 installation");
                cache
                    .add_installed(pkg2, version)
                    .expect("Failed to add pkg2 installation");

                let conn = get_conn();

                // Set pkg1's last_required_at to old date (should be cleaned up)
                conn.execute(
                    concat!(
                        "UPDATE cargo_installed ",
                        "SET last_required_at = '1970-01-01T00:00:00.000Z' ",
                        "WHERE crate = ?1",
                    ),
                    params![pkg1],
                )
                .expect("Failed to update last_required_at for pkg1");

                // Keep pkg2's last_required_at recent (should not be cleaned up)
                conn.execute(
                    concat!(
                        "UPDATE cargo_installed ",
                        "SET last_required_at = datetime('now') ",
                        "WHERE crate = ?1",
                    ),
                    params![pkg2],
                )
                .expect("Failed to update last_required_at for pkg2");

                // Run cleanup
                cache.cleanup().expect("Failed to cleanup");

                // Verify pkg1 was cleaned up
                let pkg1_exists: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM cargo_installed WHERE crate = ?1)",
                        params![pkg1],
                        |row| row.get(0),
                    )
                    .expect("Failed to query pkg1");
                assert!(!pkg1_exists, "Old installation should have been cleaned up");

                // Verify pkg2 still exists
                let pkg2_exists: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM cargo_installed WHERE crate = ?1)",
                        params![pkg2],
                        |row| row.get(0),
                    )
                    .expect("Failed to query pkg2");
                assert!(
                    pkg2_exists,
                    "Recent installation should not have been cleaned up"
                );

                // Verify through list_installed
                let installed = cache.list_installed().expect("Failed to list installed");
                assert_eq!(installed.len(), 1);
                assert_eq!(installed[0].crate_name, pkg2);
            });
        }

        #[test]
        fn test_update_versions() {
            run_with_env(&[], || {
                let cache = CargoInstallOperationCache::get();
                let crate_name = "github.com/test/pkg";

                // Create initial versions
                let versions1 = CargoInstallVersions {
                    versions: vec!["v1.0.0".to_string()],
                    fetched_at: OffsetDateTime::now_utc(),
                };

                // Add initial versions
                assert!(cache
                    .add_versions(crate_name, &versions1)
                    .expect("Failed to add initial versions"));

                // Create updated versions
                let versions2 = CargoInstallVersions {
                    versions: vec!["v1.0.0".to_string(), "v1.1.0".to_string()],
                    fetched_at: OffsetDateTime::now_utc(),
                };

                // Update versions
                assert!(cache
                    .add_versions(crate_name, &versions2)
                    .expect("Failed to update versions"));

                // Verify updated versions
                let retrieved = cache
                    .get_versions(crate_name)
                    .expect("Failed to get versions");
                assert_eq!(retrieved.versions.len(), 2);
                assert!(retrieved.versions.contains(&"v1.1.0".to_string()));
            });
        }

        #[test]
        fn test_cleanup_cascade() {
            run_with_env(&[], || {
                let cache = CargoInstallOperationCache::get();
                let crate_name = "github.com/test/pkg";
                let version = "v1.0.0";
                let env_id = "test-env";

                // Add environment
                let conn = get_conn();
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![env_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");

                // Add installation and requirement
                cache
                    .add_installed(crate_name, version)
                    .expect("Failed to add installed version");
                cache
                    .add_required_by(env_id, crate_name, version)
                    .expect("Failed to add requirement");

                // Remove environment
                conn.execute(
                    "DELETE FROM env_versions WHERE env_version_id = ?1",
                    params![env_id],
                )
                .expect("Failed to remove environment");

                // Verify that the requirement has been cleaned up
                let requirement_exists: bool = conn
                    .query_row(
                        concat!(
                            "SELECT EXISTS(",
                            "  SELECT 1 FROM cargo_install_required_by ",
                            "  WHERE crate = ?1 AND version = ?2 AND env_version_id = ?3",
                            ")",
                        ),
                        params![crate_name, version, env_id],
                        |row| row.get(0),
                    )
                    .expect("Failed to query requirement");
                assert!(
                    !requirement_exists,
                    "Requirement should be cleaned up via cascade"
                );
            });
        }
    }
}
