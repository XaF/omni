pub(crate) mod base62;

pub(crate) use base62::encode as base62_encode;
use std::path::Path;
use std::{fs, io};

#[cfg(target_os = "linux")]
mod libc;
#[cfg(target_os = "linux")]
pub(crate) use libc::detect_libc;

pub(crate) fn safe_rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    if fs::rename(from.as_ref(), to.as_ref()).is_ok() {
        return Ok(());
    }
    // Fall back to copy-and-delete
    if from.as_ref().is_dir() {
        copy_dir_all(from.as_ref(), to.as_ref())?;
        fs::remove_dir_all(from)?;
    } else {
        fs::copy(from.as_ref(), to)?;
        fs::remove_file(from)?;
    }
    Ok(())
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
