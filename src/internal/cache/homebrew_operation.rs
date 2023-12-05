use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;

use serde::Deserialize;
use serde::Serialize;
use time::Duration;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_homebrew_operation_cache;
use crate::internal::cache::loaders::set_homebrew_operation_cache;
use crate::internal::cache::offsetdatetime_hashmap;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;

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

    pub fn add_tap(&mut self, workdir_id: &str, tap_name: &str, tapped: bool) -> bool {
        let inserted = if let Some(tap) = self.tapped.iter_mut().find(|t| t.name == tap_name) {
            tap.tapped = tap.tapped || tapped;
            tap.required_by.insert(workdir_id.to_string())
        } else {
            let tap = HomebrewTapped {
                name: tap_name.to_string(),
                tapped,
                required_by: [workdir_id.to_string()].iter().cloned().collect(),
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
        workdir_id: &str,
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
                install.required_by.insert(workdir_id.to_string())
            } else {
                let install = HomebrewInstalled {
                    name: install_name.to_string(),
                    version: install_version,
                    cask: is_cask,
                    installed,
                    required_by: [workdir_id.to_string()].iter().cloned().collect(),
                };
                self.installed.push(install);
                true
            };

        if inserted {
            self.updated();
        }

        inserted
    }

    pub fn updated_homebrew(&mut self) {
        self.update_cache.updated_homebrew();
        self.updated();
    }

    pub fn should_update_homebrew(&self) -> bool {
        // TODO: add configuration option for the duration?
        self.update_cache.should_update_homebrew(Duration::days(1))
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
            // TODO: add configuration option for the duration?
            Duration::days(1),
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
            // TODO: add configuration option for the duration?
            Duration::minutes(5),
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewTapped {
    pub name: String,
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub tapped: bool,
    #[serde(default = "BTreeSet::new", skip_serializing_if = "BTreeSet::is_empty")]
    pub required_by: BTreeSet<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationUpdateCache {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub homebrew_updated_at: OffsetDateTime,
    #[serde(
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty",
        with = "offsetdatetime_hashmap"
    )]
    pub install_updated_at: HashMap<String, OffsetDateTime>,
    #[serde(
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty",
        with = "offsetdatetime_hashmap"
    )]
    pub install_checked_at: HashMap<String, OffsetDateTime>,
}

impl HomebrewOperationUpdateCache {
    pub fn new() -> Self {
        Self {
            homebrew_updated_at: utils::origin_of_time(),
            install_updated_at: HashMap::new(),
            install_checked_at: HashMap::new(),
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
        self.homebrew_updated_at = OffsetDateTime::now_utc();
    }

    pub fn should_update_homebrew(&self, expire_after: Duration) -> bool {
        (self.homebrew_updated_at + expire_after) < OffsetDateTime::now_utc()
    }

    pub fn updated_homebrew_install(
        &mut self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
    ) {
        let key = self.install_key(install_name, install_version, is_cask);
        self.install_updated_at
            .insert(key, OffsetDateTime::now_utc());
    }

    pub fn should_update_homebrew_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        expire_after: Duration,
    ) -> bool {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install_updated_at) = self.install_updated_at.get(&key) {
            (*install_updated_at + expire_after) < OffsetDateTime::now_utc()
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
        self.install_checked_at
            .insert(key, OffsetDateTime::now_utc());
    }

    pub fn should_check_homebrew_install(
        &self,
        install_name: &str,
        install_version: Option<String>,
        is_cask: bool,
        expire_after: Duration,
    ) -> bool {
        let key = self.install_key(install_name, install_version, is_cask);
        if let Some(install_checked_at) = self.install_checked_at.get(&key) {
            (*install_checked_at + expire_after) < OffsetDateTime::now_utc()
        } else {
            true
        }
    }
}

impl Empty for HomebrewOperationUpdateCache {
    fn is_empty(&self) -> bool {
        self.install_updated_at.is_empty()
            && self.install_checked_at.is_empty()
            && self.homebrew_updated_at == utils::origin_of_time()
    }
}
