use std::collections::BTreeSet;
use std::io;

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

const HOMEBREW_OPERATION_CACHE_NAME: &str = "homebrew_operation";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<HomebrewInstalled>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub tapped: Vec<HomebrewTapped>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl HomebrewOperationCache {
    pub fn add_tap(&mut self, workdir_id: &str, tap_name: &str, tapped: bool) -> bool {
        let inserted = if let Some(tap) = self.tapped.iter_mut().find(|t| t.name == tap_name) {
            tap.tapped = tap.tapped || tapped;
            tap.required_by.insert(workdir_id.to_string())
        } else {
            let tap = HomebrewTapped {
                name: tap_name.to_string(),
                tapped: tapped,
                required_by: [workdir_id.to_string()].iter().cloned().collect(),
            };
            self.tapped.push(tap);
            true
        };

        if inserted {
            self.updated_at = OffsetDateTime::now_utc();
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
                    installed: installed,
                    required_by: [workdir_id.to_string()].iter().cloned().collect(),
                };
                self.installed.push(install);
                true
            };

        if inserted {
            self.updated_at = OffsetDateTime::now_utc();
        }

        inserted
    }
}

impl Empty for HomebrewOperationCache {
    fn is_empty(&self) -> bool {
        self.installed.is_empty() && self.tapped.is_empty()
    }
}

impl CacheObject for HomebrewOperationCache {
    fn new_empty() -> Self {
        Self {
            installed: Vec::new(),
            tapped: Vec::new(),
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
