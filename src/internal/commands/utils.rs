use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use path_clean::PathClean;
use pathdiff;

use crate::internal::ENV;

pub fn split_name(string: &str, split_on: &str) -> Vec<String> {
    string.split(split_on).map(|s| s.to_string()).collect()
}

pub fn abs_or_rel_path(path: &str) -> String {
    let current_dir = std::env::current_dir().unwrap();
    let path = std::path::PathBuf::from(&path);
    let path = if path.is_absolute() {
        path
    } else {
        let joined_path = current_dir.join(&path);
        joined_path
    };

    let relative_path = pathdiff::diff_paths(path.clone(), current_dir.clone());
    if relative_path.is_none() {
        return path.to_str().unwrap().to_string();
    }
    let relative_path = relative_path.unwrap();
    let relative_path = relative_path.to_str().unwrap();

    // Ignore "./" at the beginning of the path
    let relative_path = if relative_path.starts_with("./") {
        &relative_path[2..]
    } else {
        relative_path
    };

    let absolute = path.to_str().unwrap().to_string();
    let relative = relative_path.to_string();

    if absolute.len() < relative.len() {
        absolute
    } else {
        relative
    }
}

pub fn abs_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();

    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap().join(path)
    }
    .clean();

    absolute_path
}

pub fn omni_cmd(cmd: &str) -> Result<(), io::Error> {
    let cmd_file = ENV
        .omni_cmd_file
        .clone()
        .expect("shell integration not loaded");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(cmd_file.clone())
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Unable to open omni command file: {}", e),
            )
        })?;

    writeln!(file, "{}", cmd).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Unable to write to omni command file: {}", e),
        )
    })?;

    drop(file);

    Ok(())
}
