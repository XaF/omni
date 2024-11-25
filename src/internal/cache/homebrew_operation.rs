use rusqlite::params;
use rusqlite::Row;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::cache::FromRow;
use crate::internal::config::global_config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationCache {}

impl HomebrewOperationCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn add_tap(&self, tap_name: &str, tapped: bool) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("sql/homebrew_operation_add_tap.sql"),
            params![tap_name, tapped],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_tap_required_by(
        &self,
        env_version_id: &str,
        tap_name: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("sql/homebrew_operation_add_tap_required_by.sql"),
            params![tap_name, env_version_id],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        installed: bool,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("sql/homebrew_operation_add_install.sql"),
            params![install_name, install_version, is_cask, installed],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_install_required_by(
        &self,
        env_version_id: &str,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("sql/homebrew_operation_add_install_required_by.sql"),
            params![install_name, install_version, is_cask, env_version_id],
        )?;
        Ok(inserted > 0)
    }

    pub fn homebrew_bin_path(&self) -> Option<String> {
        let db = CacheManager::get();
        let bin_path: Option<String> = db
            .query_one(include_str!("sql/homebrew_operation_get_bin_path.sql"), &[])
            .unwrap_or_default();
        bin_path
    }

    pub fn set_homebrew_bin_path(&self, bin_path: String) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/homebrew_operation_set_bin_path.sql"),
            params![bin_path],
        )?;
        Ok(updated > 0)
    }

    pub fn updated_homebrew(&self) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/homebrew_operation_updated_homebrew.sql"),
            &[],
        )?;
        Ok(updated > 0)
    }

    pub fn should_update_homebrew(&self) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("sql/homebrew_operation_should_update_homebrew.sql"),
                params![global_config().cache.homebrew.update_expire],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn homebrew_install_bin_paths(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> Option<Vec<String>> {
        let db = CacheManager::get();
        let bin_paths: Option<String> = db
            .query_one(
                include_str!("sql/homebrew_operation_get_install_bin_paths.sql"),
                params![install_name, install_version, is_cask],
            )
            .unwrap_or_default();

        if let Some(bin_paths) = bin_paths {
            if !bin_paths.is_empty() {
                return serde_json::from_str(&bin_paths).ok();
            }
        }

        None
    }

    pub fn set_homebrew_install_bin_paths(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        bin_paths: Vec<String>,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/homebrew_operation_set_install_bin_paths.sql"),
            params![
                install_name,
                install_version,
                is_cask,
                serde_json::to_string(&bin_paths)?
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn updated_tap(&self, tap_name: &str) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/homebrew_operation_updated_tap.sql"),
            params![tap_name],
        )?;
        Ok(updated > 0)
    }

    pub fn should_update_tap(&self, tap_name: &str) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("sql/homebrew_operation_should_update_tap.sql"),
                params![tap_name, global_config().cache.homebrew.tap_update_expire],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn updated_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/homebrew_operation_updated_install.sql"),
            params![install_name, install_version, is_cask],
        )?;
        Ok(updated > 0)
    }

    pub fn should_update_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("sql/homebrew_operation_should_update_install.sql"),
                params![
                    install_name,
                    install_version,
                    is_cask,
                    global_config().cache.homebrew.install_update_expire
                ],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn checked_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("sql/homebrew_operation_checked_install.sql"),
            params![install_name, install_version, is_cask],
        )?;
        Ok(updated > 0)
    }

    pub fn should_check_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("sql/homebrew_operation_should_check_install.sql"),
                params![
                    install_name,
                    install_version,
                    is_cask,
                    global_config().cache.homebrew.install_check_expire,
                ],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn cleanup<F1, F2>(
        &self,
        mut delete_install_func: F1,
        mut delete_tap_func: F2,
    ) -> Result<(), CacheManagerError>
    where
        F1: FnMut(&str, Option<&str>, bool, (usize, usize)) -> Result<(), CacheManagerError>,
        F2: FnMut(&str, (usize, usize)) -> Result<(), CacheManagerError>,
    {
        let mut db = CacheManager::get();

        let config = global_config();
        let grace_period = config.cache.homebrew.cleanup_after;

        db.transaction(|tx| {
            // Get the list of formulas and casks that can be deleted
            let removable_installs: Vec<DeletableHomebrewInstall> = tx.query_as(
                include_str!("sql/homebrew_operation_list_removable_install.sql"),
                params![&grace_period],
            )?;

            let (install_installed, install_not_installed): (Vec<_>, Vec<_>) = removable_installs
                .into_iter()
                .partition(|install| install.installed);

            for install in install_not_installed {
                // Add the deletion to the transaction
                tx.execute(
                    include_str!("sql/homebrew_operation_remove_install.sql"),
                    params![install.name, install.version, install.cask],
                )?;
            }

            let total = install_installed.len();
            for (idx, install) in install_installed.iter().enumerate() {
                // Do the physical deletion
                delete_install_func(
                    &install.name,
                    install.version.as_deref(),
                    install.cask,
                    (idx, total),
                )?;

                // Add the deletion to the transaction
                tx.execute(
                    include_str!("sql/homebrew_operation_remove_install.sql"),
                    params![install.name, install.version, install.cask],
                )?;
            }

            // Get the list of taps that can be deleted
            let removable_taps: Vec<DeletableHomebrewTap> = tx.query_as(
                include_str!("sql/homebrew_operation_list_removable_tap.sql"),
                params![&grace_period],
            )?;

            // Partition the tapped and non-tapped ones
            let (tap_tapped, tap_not_tapped): (Vec<_>, Vec<_>) =
                removable_taps.into_iter().partition(|tap| tap.tapped);

            for tap in tap_not_tapped {
                // Add the deletion to the transaction
                tx.execute(
                    include_str!("sql/homebrew_operation_remove_tap.sql"),
                    params![tap.name],
                )?;
            }

            let total = tap_tapped.len();
            for (idx, tap) in tap_tapped.iter().enumerate() {
                // Do the physical deletion
                delete_tap_func(&tap.name, (idx, total))?;

                // Add the deletion to the transaction
                tx.execute(
                    include_str!("sql/homebrew_operation_remove_tap.sql"),
                    params![tap.name],
                )?;
            }

            Ok(())
        })?;

        Ok(())
    }
}

struct DeletableHomebrewInstall {
    name: String,
    version: Option<String>,
    cask: bool,
    installed: bool,
}

impl FromRow for DeletableHomebrewInstall {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        Ok(Self {
            name: row.get(0)?,
            version: row.get(1)?,
            cask: row.get(2)?,
            installed: row.get(3)?,
        })
    }
}

struct DeletableHomebrewTap {
    name: String,
    tapped: bool,
}

impl FromRow for DeletableHomebrewTap {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        Ok(Self {
            name: row.get(0)?,
            tapped: row.get(1)?,
        })
    }
}
