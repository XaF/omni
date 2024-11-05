use std::io;

use time::OffsetDateTime;

pub trait CacheObject {
    fn new_load() -> Self
    where
        Self: Sized,
    {
        if let Ok(cache) = Self::shared() {
            return cache;
        }

        Self::new_empty()
    }
    fn new_empty() -> Self;
    fn get() -> Self;
    fn shared() -> io::Result<Self>
    where
        Self: Sized;
    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
        Self: Sized;
}

pub trait Expires {
    fn expired(&self) -> bool;
}

pub trait Empty {
    fn is_empty(&self) -> bool;
}

pub fn set_true() -> bool {
    true
}

pub fn is_true(value: &bool) -> bool {
    *value
}

pub fn set_false() -> bool {
    false
}

pub fn is_false(value: &bool) -> bool {
    !*value
}

pub fn origin_of_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH
}

pub fn is_origin_of_time(value: &OffsetDateTime) -> bool {
    *value == origin_of_time()
}

// pub fn entry_expired_option<T: Expires>(entry: &Option<T>) -> bool {
// if let Some(entry) = entry {
// entry.expired()
// } else {
// true
// }
// }

// pub fn entry_empty_option<T: Empty>(entry: &Option<T>) -> bool {
// if let Some(entry) = entry {
// entry.is_empty()
// } else {
// true
// }
// }

pub mod optional_rfc3339 {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::OffsetDateTime;

    pub fn serialize<S>(date: &Option<OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(date) => time::serde::rfc3339::serialize(date, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::deserialize(deserializer).and_then(|opt: Option<String>| {
            opt.map(|s| OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339))
                .transpose()
                .map_err(serde::de::Error::custom)
        })
    }
}
