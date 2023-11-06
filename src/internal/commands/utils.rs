use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use path_clean::PathClean;
use pathdiff;
use requestty::question::{completions, Completions};

use crate::internal::env::user_home;
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
    } else if path.starts_with("~") {
        let home_dir = std::env::var("HOME").expect("Failed to determine user's home directory");
        let path = path.strip_prefix("~").expect("Failed to strip prefix");
        PathBuf::from(home_dir).join(path)
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

pub fn file_auto_complete(p: String) -> Completions<String> {
    let current: &Path = p.as_ref();
    let (mut dir, last) = if p.ends_with('/') {
        (current, "")
    } else {
        let dir = current.parent().unwrap_or_else(|| "~/".as_ref());
        let last = current
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("");
        (dir, last)
    };

    if dir.to_str().unwrap().is_empty() {
        dir = ".".as_ref();
    }

    let full_path;
    let dir_path = PathBuf::from(dir);
    let used_tilde = if let Ok(suffix) = dir_path.strip_prefix("~") {
        full_path = PathBuf::from(user_home()).join(suffix);
        dir = full_path.as_path();
        true
    } else {
        false
    };

    let files: Completions<_> = match dir.read_dir() {
        Ok(files) => files
            .flatten()
            .flat_map(|file| {
                let mut path = file.path();
                let is_dir = path.is_dir();
                if !path
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap_or("")
                    .to_lowercase()
                    .starts_with(last.to_lowercase().as_str())
                {
                    return None;
                }

                if used_tilde {
                    if let Ok(suffix) = path.strip_prefix(&user_home()) {
                        path = PathBuf::from("~").join(suffix);
                    }
                }

                match path.into_os_string().into_string() {
                    Ok(s) if is_dir => Some(s + "/"),
                    Ok(s) => Some(s),
                    Err(_) => None,
                }
            })
            .collect(),
        Err(_) => {
            return completions![p];
        }
    };

    if files.is_empty() {
        return completions![p];
    }

    files
}
