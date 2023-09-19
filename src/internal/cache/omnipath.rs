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
}

impl OmniPathCache {
    pub fn updated(&self) -> bool {
        !self.expired() && self.updated
    }

    pub fn update(&mut self) {
        self.updated = true;
        self.updated_at = OffsetDateTime::now_utc();
        self.expires_at =
            self.updated_at + Duration::seconds(config(".").path_repo_updates.interval);
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
