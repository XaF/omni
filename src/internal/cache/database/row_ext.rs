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

use crate::internal::cache::database::FromRow;
use crate::internal::cache::migration::convert_cache;
use crate::internal::cache::migration::migrate_json_to_database;
use crate::internal::cache::CacheManagerError;
#[cfg(not(test))]
use crate::internal::config::global_config;

pub trait RowExt {
    fn query_as<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<T>, CacheManagerError>;
    fn query_one<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<T, CacheManagerError>;
    fn query_one_optional<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<Option<T>, CacheManagerError>;
}

impl RowExt for rusqlite::Connection {
    fn query_as<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<T>, CacheManagerError> {
        let mut stmt = self.prepare(query)?;
        let rows = stmt.query_map(params, |row| {
            T::from_row(row).map_err(rusqlite::Error::from)
        })?;
        Ok(rows.collect::<Result<Vec<T>, rusqlite::Error>>()?)
    }

    fn query_one<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<T, CacheManagerError> {
        Ok(self.query_row(query, params, |row| {
            T::from_row(row).map_err(rusqlite::Error::from)
        })?)
    }

    fn query_one_optional<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<Option<T>, CacheManagerError> {
        Ok(self
            .query_row(query, params, |row| {
                T::from_row(row).map_err(rusqlite::Error::from)
            })
            .optional()?)
    }
}
