#[cfg(test)]
use std::collections::HashMap;
#[cfg(not(test))]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Mutex;

use lazy_static::lazy_static;
use r2d2::Pool as R2d2Pool;
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Result as SqliteResult;
use rusqlite::Row;
use thiserror::Error;

use crate::internal::cache::migration::convert_cache;
use crate::internal::cache::migration::migrate_json_to_database;
use crate::internal::cache::CacheManagerError;
#[cfg(not(test))]
use crate::internal::config::global_config;

pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError>;
}

impl<T: FromRow> FromRow for Option<T> {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        T::from_row(row).map(Some)
    }
}

impl FromRow for String {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        row.get(0).map_err(CacheManagerError::from)
    }
}

impl FromRow for i64 {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        row.get(0).map_err(CacheManagerError::from)
    }
}

impl FromRow for i32 {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        row.get(0).map_err(CacheManagerError::from)
    }
}

impl FromRow for f64 {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        row.get(0).map_err(CacheManagerError::from)
    }
}

impl FromRow for bool {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        row.get(0).map_err(CacheManagerError::from)
    }
}

// Implement an approach to handle any tuple by calling the `FromRow` trait on each element
macro_rules! impl_from_row_tuple {
    ($($idx:tt : $t:ident),+) => {
        impl<$($t,)+> FromRow for ($($t,)+)
        where
            $($t: rusqlite::types::FromSql,)+
        {
            fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
                Ok(($(
                    row.get($idx).map_err(|e| CacheManagerError::SqlError(e))?,
                )+))
            }
        }
    }
}

impl_from_row_tuple!(0: T1);
impl_from_row_tuple!(0: T1, 1: T2);
impl_from_row_tuple!(0: T1, 1: T2, 2: T3);
impl_from_row_tuple!(0: T1, 1: T2, 2: T3, 3: T4);
impl_from_row_tuple!(0: T1, 1: T2, 2: T3, 3: T4, 4: T5);
