use rusqlite::Connection;
use rusqlite::Result as SqliteResult;
use rusqlite::Row;

use crate::internal::cache::database::get_conn;
use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::database::SqliteConnection;
use crate::internal::cache::CacheManagerError;

/// The cache manager
#[derive(Debug)]
pub struct CacheManager {
    pub conn: SqliteConnection,
}

impl CacheManager {
    pub fn get() -> Self {
        Self::new().expect("Failed to create cache manager")
    }

    fn new() -> Result<Self, CacheManagerError> {
        let conn = get_conn();
        let manager = CacheManager { conn };

        Ok(manager)
    }

    pub fn transaction<F, T>(&mut self, f: F) -> Result<T, CacheManagerError>
    where
        F: FnOnce(&Connection) -> Result<T, CacheManagerError>,
    {
        let tx = self.conn.transaction()?;
        let result = f(&tx);
        match result {
            Ok(result) => {
                tx.commit()?;
                Ok(result)
            }
            Err(e) => {
                tx.rollback()?;
                Err(e)
            }
        }
    }

    pub fn query_row<T, F>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
        f: F,
    ) -> Result<T, CacheManagerError>
    where
        F: FnOnce(&Row) -> SqliteResult<T>,
    {
        self.conn
            .query_row(query, params, f)
            .map_err(CacheManagerError::from)
    }

    pub fn execute(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<usize, CacheManagerError> {
        self.conn
            .execute(query, params)
            .map_err(CacheManagerError::from)
    }
}

impl RowExt for CacheManager {
    fn query_as<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<T>, CacheManagerError> {
        self.conn.query_as(query, params)
    }

    fn query_one<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<T, CacheManagerError> {
        self.conn.query_one(query, params)
    }

    fn query_one_optional<T: FromRow>(
        &self,
        query: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<Option<T>, CacheManagerError> {
        self.conn.query_one_optional(query, params)
    }
}
