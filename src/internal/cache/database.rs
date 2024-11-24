use std::path::PathBuf;

use lazy_static::lazy_static;
use r2d2::Pool as R2d2Pool;
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use rusqlite::Result as SqliteResult;
use rusqlite::Row;
use thiserror::Error;

use crate::internal::cache::migration::convert_cache;
use crate::internal::cache::migration::migrate_json_to_database;
use crate::internal::config::global_config;

/// Type alias for a SQLite connection pool
type SqlitePool = R2d2Pool<SqliteConnectionManager>;

/// Type alias for a pooled SQLite connection
type SqliteConnection = PooledConnection<SqliteConnectionManager>;

lazy_static! {
    /// The SQLite connection pool
    static ref SQLITE_POOL: SqlitePool = {
        let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
        let db_path = cache_dir_path.join("cache.db");

        let manager = SqliteConnectionManager::file(db_path);
        let pool = R2d2Pool::builder()
            .max_size(10)
            .build(manager)
            .expect("Failed to create pool");

        // Apply upgrades if necessary
        let conn = pool.get().expect("Couldn't get connection from pool");
        upgrade_database(&conn).expect("Failed to upgrade database");

        pool
    };
}

/// Get a pooled SQLite connection
fn get_conn() -> SqliteConnection {
    SQLITE_POOL
        .get()
        .expect("Couldn't get connection from pool")
}

/// Upgrade the database schema
fn upgrade_database(conn: &Connection) -> Result<(), CacheManagerError> {
    // Get the current version of the database
    let current_version: usize = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    // Set up for getting to version 1 and migrating
    // from the JSON files
    if current_version == 0 {
        // Make sure that we have the latest version of the JSON cache
        convert_cache()?;

        // Create the tables
        conn.execute_batch(include_str!("sql/create_tables.sql"))?;

        // Migrate the JSON files to the database
        migrate_json_to_database(&conn)?;
    }

    // Run the migration SQL, which should handle any migrations
    // of schemas from any version of the db to the latest
    // self.conn.execute_batch(MIGRATE_SQL)?;

    Ok(())
}

/// Error type for the cache manager
#[derive(Error, Debug)]
pub enum CacheManagerError {
    #[error("SQL error: {0}")]
    SqlError(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Time parse error: {0}")]
    TimeParseError(#[from] time::error::Parse),
    #[error("{0}")]
    Other(String),
}

impl From<CacheManagerError> for rusqlite::Error {
    fn from(err: CacheManagerError) -> rusqlite::Error {
        match err {
            CacheManagerError::SqlError(e) => e,
            CacheManagerError::IoError(e) => rusqlite::Error::ToSqlConversionFailure(Box::new(e)),
            CacheManagerError::SerdeError(e) => {
                rusqlite::Error::ToSqlConversionFailure(Box::new(e))
            }
            CacheManagerError::TimeParseError(e) => {
                rusqlite::Error::ToSqlConversionFailure(Box::new(e))
            }
            CacheManagerError::Other(e) => rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(std::io::ErrorKind::Other, e),
            )),
        }
    }
}

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

    // /// Executes a SQL file containing multiple statements with numbered parameters
    // ///
    // /// The SQL file can contain multiple statements separated by semicolons.
    // /// Parameters in the SQL should use numbered placeholders like ?1, ?2, etc.
    // /// The same parameter number can be used multiple times in the SQL.
    // ///
    // /// # Arguments
    // /// * `sql_content` - The content of the SQL file
    // /// * `parameters` - Vec of parameters that will be mapped to ?1, ?2, etc.
    // ///
    // /// # Returns
    // /// * `Result<()>` - Success or error
    // pub fn execute_batch_with_params(
    // &mut self,
    // sql_content: &str,
    // parameters: &[Box<dyn rusqlite::ToSql>],
    // ) -> Result<(), CacheManagerError> {
    // // Split the content into individual statements
    // let statements: Vec<&str> = sql_content
    // .split(';')
    // .map(|s| s.trim())
    // .filter(|s| !s.is_empty())
    // .collect();

    // // Find the highest parameter number used in the SQL
    // let max_param = statements
    // .iter()
    // .flat_map(|stmt| {
    // stmt.match_indices("?").filter_map(|(i, _)| {
    // if let Some(num_str) = stmt[i + 1..]
    // .chars()
    // .take_while(|c| c.is_ascii_digit())
    // .collect::<String>()
    // .parse::<usize>()
    // .ok()
    // {
    // Some(num_str)
    // } else {
    // None
    // }
    // })
    // })
    // .max()
    // .unwrap_or(0);

    // // Verify we have enough parameters
    // if max_param > parameters.len() {
    // return Err(rusqlite::Error::InvalidQuery(format!(
    // "SQL uses parameter ?{} but only {} parameters provided",
    // max_param,
    // parameters.len()
    // )));
    // }

    // // Start a transaction
    // let tx = self.conn.transaction()?;

    // // Execute each statement
    // for stmt in statements {
    // if stmt.contains('?') {
    // tx.execute(stmt, rusqlite::params_from_iter(parameters.iter()))?;
    // } else {
    // tx.execute(stmt, [])?;
    // }
    // }

    // // Commit the transaction
    // tx.commit()?;
    // Ok(())
    // }
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
}

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

// impl rusqlite::types::FromSql for PathBuf {
// fn column_result(value: rusqlite::types::ValueRef) -> rusqlite::types::FromSqlResult<Self> {
// let s: String = rusqlite::types::FromSql::column_result(value)?;
// Ok(PathBuf::from(s))
// }
// }

// impl rusqlite::types::FromSql for OffsetDateTime {
// fn column_result(value: rusqlite::types::ValueRef) -> rusqlite::types::FromSqlResult<Self> {
// let s: String = rusqlite::types::FromSql::column_result(value)?;
// Ok(OffsetDateTime::parse(&s, &Rfc3339)
// .map_err(|e| rusqlite::types::FromSqlError::Other(Box::new(e)))?)
// }
// }

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
}
