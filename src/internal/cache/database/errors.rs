use thiserror::Error;

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
