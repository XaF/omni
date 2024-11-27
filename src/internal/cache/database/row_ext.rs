use rusqlite::OptionalExtension;

use crate::internal::cache::database::FromRow;
use crate::internal::cache::CacheManagerError;

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
