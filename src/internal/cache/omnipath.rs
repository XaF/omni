use std::io;

use serde::Deserialize;
use serde::Serialize;
use time::Duration;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_omnipath_cache;
use crate::internal::cache::loaders::set_omnipath_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Expires;
use crate::internal::cache::CacheObject;
use crate::internal::config;

const OMNIPATH_CACHE_NAME: &str = "omnipath";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniPathCache {
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub updated: bool,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    #[serde(default = "String::new", skip_serializing_if = "String::is_empty")]
    pub update_error_log: String,
}

impl OmniPathCache {
    pub fn updated(&self) -> bool {
        !self.expired() && self.updated
    }

    pub fn update(&mut self) {
        self.updated = true;
        self.updated_at = OffsetDateTime::now_utc();
        self.expires_at = self.updated_at
            + Duration::seconds(
                config(".")
                    .path_repo_updates
                    .interval
                    .try_into()
                    .unwrap_or(43200),
            );
    }

    pub fn update_errored(&self) -> bool {
        !self.update_error_log.is_empty()
    }

    pub fn update_error(&mut self, update_error_log: String) {
        self.update_error_log = update_error_log;
    }

    pub fn clear_update_error(&mut self) {
        self.update_error_log = "".to_string();
    }

    pub fn update_error_log(&self) -> String {
        self.update_error_log.clone()
    }
}

impl Expires for OmniPathCache {
    fn expired(&self) -> bool {
        self.expires_at < OffsetDateTime::now_utc()
    }
}

impl CacheObject for OmniPathCache {
    fn new_empty() -> Self {
        Self {
            updated: false,
            updated_at: utils::origin_of_time(),
            expires_at: utils::origin_of_time(),
            update_error_log: "".to_string(),
        }
    }

    fn get() -> Self {
        get_omnipath_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(OMNIPATH_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(OMNIPATH_CACHE_NAME, processing_fn, set_omnipath_cache)
    }
}
