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
