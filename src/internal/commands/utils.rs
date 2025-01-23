use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use path_clean::PathClean;
use requestty::question::completions;
use requestty::question::Completions;

use crate::internal::env::omni_cmd_file;
use crate::internal::env::user_home;
use crate::internal::env::Shell;
use crate::internal::ORG_LOADER;

pub fn split_name(string: &str, split_on: &str) -> Vec<String> {
    string.split(split_on).map(|s| s.to_string()).collect()
}

pub fn abs_or_rel_path(path: &str) -> String {
    let current_dir = std::env::current_dir().unwrap();
    let path = std::path::PathBuf::from(&path).clean();
    let path = if path.is_absolute() {
        path
    } else {
        current_dir.join(&path)
    };

    let relative_path = pathdiff::diff_paths(path.clone(), current_dir.clone());
    if relative_path.is_none() {
        return path.to_str().unwrap().to_string();
    }
    let relative_path = relative_path.unwrap();
    let relative_path = relative_path.to_str().unwrap();

    // Ignore "./" at the beginning of the path
    let relative_path = relative_path.strip_prefix("./").unwrap_or(relative_path);

    let absolute = path.to_str().unwrap().to_string();
    let relative = relative_path.to_string();

    if absolute.len() < relative.len() {
        absolute
    } else {
        relative
    }
}

pub fn abs_path(path: impl AsRef<Path>) -> PathBuf {
    abs_path_from_path(path, None)
}

pub fn abs_path_from_path<T>(path: T, frompath: Option<T>) -> PathBuf
where
    T: AsRef<Path>,
{
    let path = path.as_ref();

    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else if let Ok(path) = path.strip_prefix("~") {
        PathBuf::from(user_home()).join(path)
    } else {
        match frompath {
            Some(frompath) => frompath.as_ref().join(path),
            None => std::env::current_dir()
                .expect("Failed to determine current directory")
                .join(path),
        }
    }
    .clean();

    absolute_path
}

pub fn omni_cmd(cmd: &str) -> Result<(), io::Error> {
    let cmd_file = omni_cmd_file().expect("shell integration not loaded");

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
                    if let Ok(suffix) = path.strip_prefix(user_home()) {
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

pub fn path_auto_complete(
    value: &str,
    include_repositories: bool,
    include_files: bool,
) -> BTreeSet<String> {
    // Figure out if this is a path, so we can avoid the
    // expensive repository search
    let path_only = value.starts_with('/')
        || value.starts_with('.')
        || value.starts_with("~/")
        || value == "~"
        || value == "-";

    // To store the completions we find
    let mut completions = BTreeSet::new();

    // Print all the completion related to path completion
    let (list_dir, strip_path_prefix, replace_home_prefix) = if value == "~" {
        (user_home(), false, true)
    } else if let Some(value) = value.strip_prefix("~/") {
        if let Some(slash) = value.rfind('/') {
            let abspath = format!("{}/{}", user_home(), &value[..(slash + 1)]);
            (abspath, false, true)
        } else {
            (user_home(), false, true)
        }
    } else if let Some(slash) = value.rfind('/') {
        (value[..(slash + 1)].to_string(), false, false)
    } else {
        (".".to_string(), true, false)
    };

    if let Ok(files) = std::fs::read_dir(&list_dir) {
        for path in files.flatten() {
            let is_dir = path.path().is_dir();
            if !is_dir && !include_files {
                continue;
            }

            let path_buf;
            let path_obj = path.path();
            let path = if strip_path_prefix {
                path_obj.strip_prefix(&list_dir).unwrap()
            } else if replace_home_prefix {
                if let Ok(path_obj) = path_obj.strip_prefix(user_home()) {
                    path_buf = PathBuf::from("~").join(path_obj);
                    path_buf.as_path()
                } else {
                    path_obj.as_path()
                }
            } else {
                path_obj.as_path()
            };

            let path_str = path.to_string_lossy().to_string();
            if !path_str.starts_with(value) {
                continue;
            }

            completions.insert(if is_dir {
                format!("{}/", path_str)
            } else {
                path_str
            });
        }
    }

    // Get all the repositories per org that match the value
    if include_repositories && !path_only {
        let add_space = if Shell::current().is_fish() { " " } else { "" };
        for match_value in ORG_LOADER.complete(value) {
            completions.insert(format!("{}{}", match_value, add_space));
        }
    }

    completions
}

pub fn str_to_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "1" | "on" | "enable" | "enabled" | "yes" | "y" => Some(true),
        "false" | "0" | "off" | "disable" | "disabled" | "no" | "n" => Some(false),
        _ => None,
    }
}

pub struct SplitOnSeparators<'a> {
    remainder: &'a str,
    separators: &'a [char],
    skip_next_char: bool,
}

impl<'a> SplitOnSeparators<'a> {
    pub fn new(s: &'a str, separators: &'a [char]) -> Self {
        SplitOnSeparators {
            remainder: s,
            separators,
            skip_next_char: false,
        }
    }

    pub fn remainder(&mut self) -> &'a str {
        let remainder = self.remainder;
        self.remainder = "";
        remainder
    }
}

impl<'a> Iterator for SplitOnSeparators<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remainder.is_empty() {
            return None;
        }

        if self.skip_next_char {
            self.skip_next_char = false;
            self.remainder = &self.remainder[1..];

            if self.remainder.is_empty() {
                return None;
            }
        }

        match self.remainder.find(|c| self.separators.contains(&c)) {
            Some(index) => {
                let (part, rest) = self.remainder.split_at(index);
                self.remainder = rest;
                self.skip_next_char = true;
                Some(part)
            }
            None => {
                let part = self.remainder;
                self.remainder = "";
                Some(part)
            }
        }
    }
}
