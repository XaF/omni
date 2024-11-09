use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_homebrew_operation_cache;
use crate::internal::cache::loaders::set_homebrew_operation_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;
use crate::internal::config::global_config;
use crate::internal::env::now as omni_now;

const HOMEBREW_OPERATION_CACHE_NAME: &str = "homebrew_operation";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<HomebrewInstalled>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub tapped: Vec<HomebrewTapped>,
    #[serde(
        default = "HomebrewOperationUpdateCache::new",
        skip_serializing_if = "HomebrewOperationUpdateCache::is_empty"
    )]
    pub update_cache: HomebrewOperationUpdateCache,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

impl HomebrewOperationCache {
    pub fn updated(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn add_tap(&mut self, tap_name: &str, tapped: bool) -> bool {
        self.add_tap_required_by("", tap_name, tapped)
    }

    pub fn add_tap_required_by(
        &mut self,
        env_version_id: &str,
        tap_name: &str,
        tapped: bool,
    ) -> bool {
        let inserted = if let Some(tap) = self.tapped.iter_mut().find(|t| t.name == tap_name) {
            tap.tapped = tap.tapped || tapped;
            let inserted = match env_version_id.is_empty() {
                true => true,
                false => tap.required_by.insert(env_version_id.to_string()),
            };
            if inserted || tap.last_required_at < omni_now() {
                tap.last_required_at = omni_now();
                true
            } else {
                false
            }
        } else {
            let tap = HomebrewTapped {
                name: tap_name.to_string(),
                tapped,
                required_by: [env_version_id.to_string()].iter().cloned().collect(),
                last_required_at: omni_now(),
            };
            self.tapped.push(tap);
            true
        };

        if inserted {
            self.updated();
        }

        inserted
    }

    pub fn add_install(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        installed: bool,
    ) -> bool {
        self.add_install_required_by("", install_name, install_version, is_cask, installed)
    }

    pub fn add_install_required_by(
        &mut self,
        env_version_id: &str,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        installed: bool,
    ) -> bool {
        let inserted =
            if let Some(install) = self.installed.iter_mut().find(|i| {
                i.name == install_name && i.cask == is_cask && i.version == install_version
            }) {
                install.installed = install.installed || installed;
                let inserted = match env_version_id.is_empty() {
                    true => true,
                    false => install.required_by.insert(env_version_id.to_string()),
                };
                if inserted || install.last_required_at < omni_now() {
                    install.last_required_at = omni_now();
                    true
                } else {
                    false
                }
            } else {
                let install = HomebrewInstalled {
                    name: install_name.to_string(),
                    version: install_version,
                    cask: is_cask,
                    installed,
                    required_by: [env_version_id.to_string()].iter().cloned().collect(),
                    last_required_at: omni_now(),
                };
                self.installed.push(install);
                true
            };

        if inserted {
            self.updated();
        }

        inserted
    }

    pub fn homebrew_bin_path(&self) -> Option<String> {
        self.update_cache.homebrew_bin_path()
    }

    pub fn set_homebrew_bin_path(&mut self, bin_path: String) {
        self.update_cache
            .set_homebrew_bin_path(bin_path.to_string());
        self.updated();
    }

    pub fn updated_homebrew(&mut self) {
        self.update_cache.updated_homebrew();
        self.updated();
    }

    pub fn should_update_homebrew(&self) -> bool {
        self.update_cache
            .should_update_homebrew(Duration::from_secs(
                global_config().cache.homebrew.update_expire,
            ))
    }

    pub fn homebrew_install_bin_paths(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> Option<Vec<String>> {
        self.update_cache
            .homebrew_install_bin_paths(install_name, install_version, is_cask)
    }

    pub fn set_homebrew_install_bin_paths(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        bin_paths: Vec<String>,
    ) {
        self.update_cache.set_homebrew_install_bin_paths(
            install_name,
            install_version,
            is_cask,
            bin_paths,
        );
        self.updated();
    }

    pub fn updated_tap(&mut self, tap_name: &str) {
        self.update_cache.updated_homebrew_tap(tap_name);
        self.updated();
    }

    pub fn should_update_tap(&self, tap_name: &str) -> bool {
        self.update_cache.should_update_homebrew_tap(
            tap_name,
            Duration::from_secs(global_config().cache.homebrew.tap_update_expire),
        )
    }

    pub fn updated_install(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) {
        self.update_cache
            .updated_homebrew_install(install_name, install_version, is_cask);
        self.updated();
    }

    pub fn should_update_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> bool {
        self.update_cache.should_update_homebrew_install(
            install_name,
            install_version,
            is_cask,
            Duration::from_secs(global_config().cache.homebrew.install_update_expire),
        )
    }

    pub fn checked_install(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) {
        self.update_cache
            .checked_homebrew_install(install_name, install_version, is_cask);
        self.updated();
    }

    pub fn should_check_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> bool {
        self.update_cache.should_check_homebrew_install(
            install_name,
            install_version,
            is_cask,
            Duration::from_secs(global_config().cache.homebrew.install_check_expire),
        )
    }
}

impl Empty for HomebrewOperationCache {
    fn is_empty(&self) -> bool {
        self.installed.is_empty() && self.tapped.is_empty() && self.update_cache.is_empty()
    }
}

impl CacheObject for HomebrewOperationCache {
    fn new_empty() -> Self {
        Self {
            installed: Vec::new(),
            tapped: Vec::new(),
            update_cache: HomebrewOperationUpdateCache::new(),
            updated_at: utils::origin_of_time(),
        }
    }

    fn get() -> Self {
        get_homebrew_operation_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(HOMEBREW_OPERATION_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(
            HOMEBREW_OPERATION_CACHE_NAME,
            processing_fn,
            set_homebrew_operation_cache,
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewInstalled {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub cask: bool,
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub installed: bool,
    #[serde(default = "BTreeSet::new", skip_serializing_if = "BTreeSet::is_empty")]
    pub required_by: BTreeSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

impl HomebrewInstalled {
    pub fn removable(&self) -> bool {
        // If the formula is required by any workdir, it should not be removed.
        if !self.required_by.is_empty() {
            return false;
        }

        // If the formula was not installed by omni, and is not required by any
        // workdir, it can be removed without any grace period or extra logic
        if !self.installed {
            return true;
        }

        // If the formula was installed by omni, and is not required by any
        // workdir, it can be removed after the grace period.
        let config = global_config();
        let grace_period = config.cache.homebrew.cleanup_after;
        let grace_period = time::Duration::seconds(grace_period as i64);

        (self.last_required_at + grace_period) < omni_now()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewTapped {
    pub name: String,
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub tapped: bool,
    #[serde(default = "BTreeSet::new", skip_serializing_if = "BTreeSet::is_empty")]
    pub required_by: BTreeSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

impl HomebrewTapped {
    pub fn removable(&self) -> bool {
        // If the tap is required by any workdir, it should not be removed.
        if !self.required_by.is_empty() {
            return false;
        }

        // If the tap was not installed by omni, and is not required by any
        // workdir, it can be removed without any grace period or extra logic
        if !self.tapped {
            return true;
        }

        // If the tap was installed by omni, and is not required by any
        // workdir, it can be removed after the grace period.
        let config = global_config();
        let grace_period = config.cache.homebrew.cleanup_after;
        let grace_period = time::Duration::seconds(grace_period as i64);

        (self.last_required_at + grace_period) < omni_now()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationUpdateCache {
    #[serde(
        default = "HomebrewOperationUpdateCacheHomebrew::new",
        skip_serializing_if = "HomebrewOperationUpdateCacheHomebrew::is_empty"
    )]
    pub homebrew: HomebrewOperationUpdateCacheHomebrew,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub install: HashMap<String, HomebrewOperationUpdateCacheInstall>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub tap: HashMap<String, HomebrewOperationUpdateCacheTap>,
}

impl HomebrewOperationUpdateCache {
    pub fn new() -> Self {
        Self {
            homebrew: HomebrewOperationUpdateCacheHomebrew::new(),
            install: HashMap::new(),
            tap: HashMap::new(),
        }
    }

    fn install_key(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> String {
        format!(
            "{}{}{}",
            if is_cask { "cask:" } else { "formula:" },
            install_name,
            if let Some(install_version) = install_version {
                format!("@{}", install_version)
            } else {
                "".to_string()
            },
        )
    }

    pub fn updated_homebrew(&mut self) {
        self.homebrew.updated_at = OffsetDateTime::now_utc();
    }

    pub fn should_update_homebrew(&self, expire_after: Duration) -> bool {
        (self.homebrew.updated_at + expire_after) < OffsetDateTime::now_utc()
    }

    pub fn homebrew_bin_path(&self) -> Option<String> {
        self.homebrew.bin_path.clone()
    }

    pub fn set_homebrew_bin_path(&mut self, bin_path: String) {
        self.homebrew.bin_path = Some(bin_path);
    }

    pub fn homebrew_install_bin_paths(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) -> Option<Vec<String>> {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install) = self.install.get(&key) {
            install.bin_paths.clone()
        } else {
            None
        }
    }

    pub fn set_homebrew_install_bin_paths(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        bin_paths: Vec<String>,
    ) {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install) = self.install.get_mut(&key) {
            install.bin_paths = Some(bin_paths);
        } else {
            let mut install = HomebrewOperationUpdateCacheInstall::new();
            install.bin_paths = Some(bin_paths);
            self.install.insert(key, install);
        }
    }

    pub fn updated_homebrew_tap(&mut self, tap_name: &str) {
        let key = tap_name.to_string();
        if let Some(tap) = self.tap.get_mut(&key) {
            tap.updated_at = OffsetDateTime::now_utc();
        } else {
            let mut tap = HomebrewOperationUpdateCacheTap::new();
            tap.updated_at = OffsetDateTime::now_utc();
            self.tap.insert(tap_name.to_string(), tap);
        }
    }

    pub fn updated_homebrew_install(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install) = self.install.get_mut(&key) {
            install.updated_at = OffsetDateTime::now_utc();
        } else {
            let mut install = HomebrewOperationUpdateCacheInstall::new();
            install.updated_at = OffsetDateTime::now_utc();
            self.install.insert(key, install);
        }
    }

    pub fn removed_homebrew_install(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) {
        let key = self.install_key(install_name, install_version, is_cask);
        self.install.remove(&key);
    }

    pub fn should_update_homebrew_tap(&self, tap_name: &str, expire_after: Duration) -> bool {
        let key = tap_name.to_string();
        if let Some(tap) = self.tap.get(&key) {
            (tap.updated_at + expire_after) < OffsetDateTime::now_utc()
        } else {
            true
        }
    }

    pub fn should_update_homebrew_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        expire_after: Duration,
    ) -> bool {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install) = self.install.get(&key) {
            (install.updated_at + expire_after) < OffsetDateTime::now_utc()
        } else {
            true
        }
    }

    pub fn checked_homebrew_install(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install) = self.install.get_mut(&key) {
            install.checked_at = OffsetDateTime::now_utc();
        } else {
            let mut install = HomebrewOperationUpdateCacheInstall::new();
            install.checked_at = OffsetDateTime::now_utc();
            self.install.insert(key, install);
        }
    }

    pub fn should_check_homebrew_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        expire_after: Duration,
    ) -> bool {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install) = self.install.get(&key) {
            (install.checked_at + expire_after) < OffsetDateTime::now_utc()
        } else {
            true
        }
    }
}

impl Empty for HomebrewOperationUpdateCache {
    fn is_empty(&self) -> bool {
        self.install.is_empty() && self.homebrew.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationUpdateCacheHomebrew {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_path: Option<String>,
}

impl HomebrewOperationUpdateCacheHomebrew {
    pub fn new() -> Self {
        Self {
            updated_at: utils::origin_of_time(),
            bin_path: None,
        }
    }
}

impl Empty for HomebrewOperationUpdateCacheHomebrew {
    fn is_empty(&self) -> bool {
        self.updated_at == utils::origin_of_time()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationUpdateCacheInstall {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub checked_at: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_paths: Option<Vec<String>>,
}

impl HomebrewOperationUpdateCacheInstall {
    pub fn new() -> Self {
        Self {
            updated_at: utils::origin_of_time(),
            checked_at: utils::origin_of_time(),
            bin_paths: None,
        }
    }
}

impl Empty for HomebrewOperationUpdateCacheInstall {
    fn is_empty(&self) -> bool {
        self.updated_at == utils::origin_of_time()
            && self.checked_at == utils::origin_of_time()
            && self.bin_paths.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationUpdateCacheTap {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

impl HomebrewOperationUpdateCacheTap {
    pub fn new() -> Self {
        Self {
            updated_at: utils::origin_of_time(),
        }
    }
}

impl Empty for HomebrewOperationUpdateCacheTap {
    fn is_empty(&self) -> bool {
        self.updated_at == utils::origin_of_time()
    }
}
