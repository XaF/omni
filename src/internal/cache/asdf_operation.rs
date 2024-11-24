use std::collections::BTreeSet;

use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::utils;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::cache::FromRow;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::env::now as omni_now;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfOperationCache {}

impl AsdfOperationCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn updated_asdf(&self) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(include_str!("sql/asdf_operation_updated_asdf.sql"), &[])?;
        Ok(updated > 0)
    }

    pub fn updated_asdf_plugin(&self, plugin: &str) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/asdf_operation_updated_plugin.sql"),
            params![plugin],
        )?;
        Ok(updated > 0)
    }

    pub fn set_asdf_plugin_versions(
        &self,
        plugin: &str,
        versions: AsdfPluginVersions,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/asdf_operation_updated_plugin_versions.sql"),
            params![plugin, serde_json::to_string(&versions.versions)?],
        )?;
        Ok(updated > 0)
    }

    pub fn should_update_asdf(&self) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("sql/asdf_operation_should_update_asdf.sql"),
                params![global_config().cache.asdf.update_expire],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn should_update_asdf_plugin(&self, plugin: &str) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("sql/asdf_operation_should_update_plugin.sql"),
                params![plugin, global_config().cache.asdf.plugin_update_expire,],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn get_asdf_plugin_versions(&self, plugin: &str) -> Option<AsdfPluginVersions> {
        let db = CacheManager::get();
        let versions: Option<AsdfPluginVersions> = db
            .query_one(
                include_str!("sql/asdf_operation_get_plugin_versions.sql"),
                params![plugin],
            )
            .unwrap_or_default();
        versions
    }

    pub fn add_installed(
        &self,
        tool: &str,
        version: &str,
        tool_real_name: Option<&str>,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("sql/asdf_operation_add_installed.sql"),
            params![tool, tool_real_name, version],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_required_by(
        &self,
        env_version_id: &str,
        tool: &str,
        version: &str,
        tool_real_name: Option<&str>,
    ) -> Result<bool, CacheManagerError> {
        let mut db = CacheManager::get();
        let mut inserted = false;

        db.transaction(|tx| {
            // Get the current list of required_by
            let required_by_json: Option<String> = tx.query_one::<Option<String>>(
                include_str!("sql/asdf_operation_get_required_by.sql"),
                params![tool, version],
            )?;
            let mut required_by: BTreeSet<String> = match required_by_json {
                Some(required_by_json) => serde_json::from_str(&required_by_json)?,
                None => BTreeSet::new(),
            };

            if !required_by.insert(env_version_id.to_string()) {
                // Nothing to do, let's exit early
                return Ok(());
            }

            // Insert the new required_by
            tx.execute(
                include_str!("sql/asdf_operation_add_required_by.sql"),
                params![
                    tool,
                    tool_real_name,
                    version,
                    serde_json::to_string(&required_by)?,
                ],
            )?;

            inserted = true;

            Ok(())
        })?;

        Ok(inserted)
    }

    pub fn cleanup<F>(&self, mut delete_func: F) -> Result<(), CacheManagerError>
    where
        F: FnMut(&str, &str) -> Result<(), CacheManagerError>,
    {
        let mut db = CacheManager::get();
        let mut removed = 0;

        let config = global_config();
        let grace_period = config.cache.asdf.cleanup_after;

        db.transaction(|tx| {
            let deletable_tools: Vec<DeletableAsdfTool> = tx.query_as(
                include_str!("sql/asdf_operation_list_removable.sql"),
                params![&grace_period],
            )?;

            for tool in deletable_tools {
                delete_func(&tool.tool, &tool.version)?;
                removed += 1;
            }

            Ok(())
        })?;

        Ok(())
    }
}

struct DeletableAsdfTool {
    tool: String,
    version: String,
}

impl FromRow for DeletableAsdfTool {
    fn from_row(row: &rusqlite::Row) -> Result<Self, CacheManagerError> {
        let tool: String = row.get(0)?;
        let version: String = row.get(1)?;
        Ok(Self { tool, version })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AsdfPluginVersions {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub fetched_at: OffsetDateTime,
}

impl FromRow for AsdfPluginVersions {
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

impl AsdfPluginVersions {
    pub fn new(versions: Vec<String>) -> Self {
        Self {
            versions,
            fetched_at: OffsetDateTime::now_utc(),
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
