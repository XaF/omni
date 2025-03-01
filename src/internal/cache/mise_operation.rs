use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::utils;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::env::now as omni_now;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MiseOperationCache {}

impl MiseOperationCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn updated_mise(&self) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/mise_operation_updated_mise.sql"),
            &[],
        )?;
        Ok(updated > 0)
    }

    pub fn updated_mise_plugin(&self, plugin: &str) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/mise_operation_updated_plugin.sql"),
            params![plugin],
        )?;
        Ok(updated > 0)
    }

    pub fn set_mise_plugin_versions(
        &self,
        plugin: &str,
        versions: MisePluginVersions,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/mise_operation_updated_plugin_versions.sql"),
            params![plugin, serde_json::to_string(&versions.versions)?],
        )?;
        Ok(updated > 0)
    }

    pub fn should_update_mise(&self) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("database/sql/mise_operation_should_update_mise.sql"),
                params![global_config().cache.mise.update_expire],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn should_update_mise_plugin(&self, plugin: &str) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("database/sql/mise_operation_should_update_plugin.sql"),
                params![plugin, global_config().cache.mise.plugin_update_expire,],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn get_mise_plugin_versions(&self, plugin: &str) -> Option<MisePluginVersions> {
        let db = CacheManager::get();
        let versions: Option<MisePluginVersions> = db
            .query_one(
                include_str!("database/sql/mise_operation_get_plugin_versions.sql"),
                params![plugin],
            )
            .unwrap_or_default();
        versions
    }

    pub fn add_installed(
        &self,
        tool: &str,
        plugin_name: &str,
        normalized_name: &str,
        version: &str,
        bin_path: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/mise_operation_add_installed.sql"),
            params![
                tool,
                plugin_name,
                normalized_name,
                version,
                serde_json::to_string(&vec![bin_path])?
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_required_by(
        &self,
        env_version_id: &str,
        normalized_name: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/mise_operation_add_required_by.sql"),
            params![normalized_name, version, env_version_id],
        )?;
        Ok(inserted > 0)
    }

    pub fn cleanup<F>(&self, mut delete_func: F) -> Result<(), CacheManagerError>
    where
        F: FnMut(&str, &str) -> Result<(), CacheManagerError>,
    {
        let mut db = CacheManager::get();

        let config = global_config();
        let grace_period = config.cache.mise.cleanup_after;

        db.transaction(|tx| {
            // Get the list of tools and versions that can be deleted
            let deletable_tools: Vec<DeletableMiseTool> = tx.query_as(
                include_str!("database/sql/mise_operation_list_removable.sql"),
                params![&grace_period],
            )?;

            for tool in deletable_tools {
                // Do the physical deletion of the tool and version
                delete_func(&tool.tool, &tool.version)?;

                // Add the deletion of that tool and version to the transaction
                tx.execute(
                    include_str!("database/sql/mise_operation_remove.sql"),
                    params![tool.tool, tool.version],
                )?;
            }

            Ok(())
        })?;

        db.execute(
            include_str!("database/sql/mise_operation_cleanup_versions.sql"),
            params![&config.cache.mise.plugin_versions_retention],
        )?;

        Ok(())
    }
}

#[derive(Debug)]
struct DeletableMiseTool {
    tool: String,
    version: String,
}

impl FromRow for DeletableMiseTool {
    fn from_row(row: &rusqlite::Row) -> Result<Self, CacheManagerError> {
        let tool: String = row.get(0)?;
        let version: String = row.get(1)?;
        Ok(Self { tool, version })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MisePluginVersions {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub fetched_at: OffsetDateTime,
}

impl FromRow for MisePluginVersions {
    fn from_row(row: &rusqlite::Row) -> Result<Self, CacheManagerError> {
        let versions_json: String = row.get(0)?;
        let versions: Vec<String> = serde_json::from_str(&versions_json)?;

        let fetched_at_str: String = row.get(1)?;
        let fetched_at = OffsetDateTime::parse(&fetched_at_str, &Rfc3339)?;

        Ok(Self {
            versions,
            fetched_at,
        })
    }
}

impl MisePluginVersions {
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

    pub fn get(&self, matcher: &VersionMatcher) -> Option<String> {
        self.versions
            .iter()
            .rev()
            .find(|v| matcher.matches(v))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::internal::cache::database::get_conn;
    use crate::internal::testutils::run_with_env;

    mod mise_operation_cache {
        use super::*;

        #[test]
        fn test_should_update_mise() {
            run_with_env(&[], || {
                let cache = MiseOperationCache::get();

                // First time should return true as no data exists
                assert!(cache.should_update_mise());

                // Update mise
                cache.updated_mise().expect("Failed to update mise");

                // Should now return false as we just updated
                assert!(!cache.should_update_mise());
            });
        }

        #[test]
        fn test_should_update_mise_plugin() {
            run_with_env(&[], || {
                let cache = MiseOperationCache::get();
                let plugin = "test-plugin";

                // First time should return true as no data exists
                assert!(cache.should_update_mise_plugin(plugin));

                // Update plugin
                cache
                    .updated_mise_plugin(plugin)
                    .expect("Failed to update plugin");

                // Should now return false as we just updated
                assert!(!cache.should_update_mise_plugin(plugin));
            });
        }

        #[test]
        fn test_set_and_get_plugin_versions() {
            run_with_env(&[], || {
                let cache = MiseOperationCache::get();
                let plugin = "test-plugin";

                // Initially should return None
                assert!(cache.get_mise_plugin_versions(plugin).is_none());

                // Create test versions
                let versions = MisePluginVersions::new(vec![
                    "1.0.0".to_string(),
                    "1.1.0".to_string(),
                    "2.0.0".to_string(),
                ]);

                // Set versions
                cache
                    .set_mise_plugin_versions(plugin, versions.clone())
                    .expect("Failed to set plugin versions");

                // Get versions and verify
                let retrieved = cache
                    .get_mise_plugin_versions(plugin)
                    .expect("Failed to get plugin versions");

                assert_eq!(retrieved.versions, versions.versions);
                assert!(retrieved.fetched_at <= OffsetDateTime::now_utc());
            });
        }

        #[test]
        fn test_add_installed_and_required_by() {
            run_with_env(&[], || {
                let cache = MiseOperationCache::get();

                let tool = "test-tool";
                let plugin = "test-plugin";
                let norm_plugin = "test-plugin-normalized";
                let version = "1.0.0";
                let bin_path = "bin/path/blah";
                let env_version_id = "test-env";

                // Add installed tool
                assert!(cache
                    .add_installed(tool, plugin, norm_plugin, version, bin_path)
                    .expect("Failed to add installed tool"));

                // Before adding a required_by, we need to add the environment version
                // otherwise the foreign key constraint will fail
                let conn = get_conn();
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![env_version_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");

                // Add required_by relationship
                assert!(cache
                    .add_required_by(env_version_id, norm_plugin, version)
                    .expect("Failed to add required_by relationship"));
            });
        }

        #[test]
        fn test_cleanup() {
            run_with_env(&[], || {
                // Directly inject a tool in the database, so we can use a very old date
                let conn = get_conn();

                let mut installed_stmt = conn
                    .prepare("INSERT INTO mise_installed (tool, plugin_name, normalized_name, version, bin_paths, last_required_at) VALUES (?, ?, ?, ?, '[]', ?)")
                    .expect("Failed to prepare statement");

                installed_stmt
                    .execute(params![
                        "test-tool",
                        "test-plugin",
                        "test-plugin-normalized",
                        "1.0.0",
                        "1970-01-01T00:00:00Z"
                    ])
                    .expect("Failed to insert test tool to remove");
                installed_stmt
                    .execute(params![
                        "test-tool",
                        "test-plugin",
                        "test-plugin-normalized",
                        "1.1.0",
                        "1970-01-01T00:00:00Z"
                    ])
                    .expect("Failed to insert test tool to keep because of requirement");
                installed_stmt
                    .execute(params![
                        "test-tool",
                        "test-plugin",
                        "test-plugin-normalized",
                        "1.2.0",
                        omni_now().format(&Rfc3339).expect("Failed to format date"),
                    ])
                    .expect("Failed to insert test tool to keep because of date");

                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params!["test-env", "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");

                let mut required_by_stmt = conn
                    .prepare("INSERT INTO mise_installed_required_by (normalized_name, version, env_version_id) VALUES (?, ?, ?)")
                    .expect("Failed to prepare statement");

                required_by_stmt
                    .execute(params!["test-plugin-normalized", "1.1.0", "test-env"])
                    .expect("Failed to insert required_by relationship");

                let cache = MiseOperationCache::get();

                // Mock deletion function
                let mut deleted_tools = Vec::new();
                let delete_func = |tool: &str, version: &str| {
                    deleted_tools.push((tool.to_string(), version.to_string()));
                    Ok(())
                };

                // Run cleanup
                cache.cleanup(delete_func).expect("Failed to cleanup");

                // Verify that the tool has been deleted
                assert_eq!(deleted_tools.len(), 1);
                assert_eq!(
                    deleted_tools[0],
                    ("test-plugin-normalized".to_string(), "1.0.0".to_string())
                );

                // Verify that the tool has been removed from the database
                let tool_in_db = conn
                    .query_row(
                        "SELECT COUNT(*) FROM mise_installed WHERE tool = ? AND version = ?",
                        params![deleted_tools[0].0, deleted_tools[0].1],
                        |row| row.get::<_, i64>(0),
                    )
                    .expect("Failed to query tool in database");
                assert_eq!(tool_in_db, 0);
            });
        }
    }

    mod mise_plugin_versions {
        use super::*;

        #[test]
        fn test_new() {
            let versions = vec![
                "1.0.0".to_string(),
                "1.1.0".to_string(),
                "2.0.0".to_string(),
            ];
            let plugin_versions = MisePluginVersions::new(versions.clone());

            assert_eq!(plugin_versions.versions, versions);
            assert!(plugin_versions.fetched_at <= OffsetDateTime::now_utc());
        }

        #[test]
        fn test_freshness() {
            let versions = vec!["1.0.0".to_string()];
            let plugin_versions = MisePluginVersions::new(versions);

            // Test is_fresh
            assert!(plugin_versions.is_fresh());

            // Test is_stale
            assert!(!plugin_versions.is_stale(3600)); // Not stale after 1 hour
        }
    }
}
