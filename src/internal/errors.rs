use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncUpdateError {
    #[error("error during file operation: {0}")]
    IO(#[from] std::io::Error),
    #[error("actual init operation `{0}` is different from expected `{1}`")]
    MismatchedInit(String, String),
    #[error("already initialized, but read another init operation")]
    AlreadyInit,
    #[error("invalid format: {0}")]
    InvalidFormat(#[from] serde_json::Error),
    #[error("progress handler was not initialized")]
    NoProgressHandler,
}
