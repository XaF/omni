mod errors;
mod from_row;
mod manager;
mod pool;
mod row_ext;
mod upgrade;

use pool::SqliteConnection;
use upgrade::upgrade_database;

pub(super) use from_row::FromRow;
pub(super) use row_ext::RowExt;

pub(crate) use errors::CacheManagerError;
pub(crate) use manager::CacheManager;

cfg_if::cfg_if! {
    if #[cfg(test)] {
        pub(super) use pool::get_conn;
        pub(crate) use pool::cleanup_test_pool;
    } else {
        use pool::get_conn;
    }
}
