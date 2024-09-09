use git_url_parse::GitUrlParseError;
use thiserror::Error;

use crate::internal::config::up::utils::SyncUpdateInit;

#[derive(Error, Debug)]
pub enum SyncUpdateError {
    #[error("error during file operation: {0}")]
    IO(#[from] std::io::Error),
    #[error("actual init operation `{0}` is different from expected `{1}`")]
    MismatchedInit(Box<SyncUpdateInit>, Box<SyncUpdateInit>),
    #[error("the expected run has more options than the attached run")]
    MissingInitOptions,
    #[error("already initialized, but read another init operation")]
    AlreadyInit,
    #[error("invalid format: {0}")]
    InvalidFormat(#[from] serde_json::Error),
    #[error("progress handler was not initialized")]
    NoProgressHandler,
}

#[derive(Error, Debug)]
pub enum GitUrlError {
    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),
    #[error("missing repository name")]
    MissingRepositoryName,
    #[error("missing repository owner")]
    MissingRepositoryOwner,
    #[error("missing repository host")]
    MissingRepositoryHost,
    #[error("parse timeout")]
    ParseTimeout,
    #[error("normalize timeout")]
    NormalizeTimeout,
    #[error("error during URL parsing: {0}")]
    UrlParseError(#[from] GitUrlParseError),
}
