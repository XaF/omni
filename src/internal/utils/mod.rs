pub(crate) mod base62;
pub(crate) use base62::encode as base62_encode;

#[cfg(target_os = "linux")]
mod libc;
#[cfg(target_os = "linux")]
pub(crate) use libc::detect_libc;
