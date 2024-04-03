use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use blake3::Hasher;
use itertools::any;
use normalize_path::NormalizePath;

use crate::internal::config::loader::WORKDIR_CONFIG_FILES;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::utils::base62_encode;
use crate::internal::workdir;

/// Return the name of the directory to use in the data path
/// for the given subdirectory of the work directory.
pub fn data_path_dir_hash(dir: &str) -> String {
    let dir = Path::new(dir).normalize().to_string_lossy().to_string();

    if dir.is_empty() {
        "root".to_string()
    } else {
        let mut hasher = Hasher::new();
        hasher.update(dir.as_bytes());
        let hash_bytes = hasher.finalize();
        let hash_b62 = base62_encode(hash_bytes.as_bytes())[..20].to_string();
        hash_b62
    }
}

/// Remove the given directory, even if it contains read-only files.
/// This will first try to remove the directory normally, and if that
/// fails with a PermissionDenied error, it will make all files and
/// directories in the given path writeable, and then try again.
pub fn force_remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path = path.as_ref();
    if path.exists() {
        match std::fs::remove_dir_all(path) {
            Ok(_) => {}
            Err(err) => {
                if err.kind() == std::io::ErrorKind::PermissionDenied {
                    set_writeable_recursive(path)?;
                    std::fs::remove_dir_all(path)?;
                } else {
                    return Err(err);
                }
            }
        }
    }
    Ok(())
}

/// Set all files and directories in the given path to be writeable.
/// This is useful when we want to remove a directory that contains
/// read-only files, which would otherwise fail.
pub fn set_writeable_recursive<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    for entry in walkdir::WalkDir::new(&path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let metadata = entry.metadata()?;
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_mode(0o775);
            std::fs::set_permissions(entry.path(), permissions)?;
        }
    }
    Ok(())
}

/// Return the modification time of the configuration files
/// for the work directory at the given path.
pub fn get_config_mod_times<T: AsRef<str>>(path: T) -> HashMap<String, u64> {
    let mut mod_times = HashMap::new();

    if let Some(wdroot) = workdir(path.as_ref()).root() {
        for config_file in WORKDIR_CONFIG_FILES {
            let wd_config_path = PathBuf::from(wdroot).join(config_file);
            if let Ok(metadata) = std::fs::metadata(&wd_config_path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(modified) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let modified = modified.as_secs();
                        mod_times.insert(config_file.to_string(), modified);
                    }
                }
            }
        }
    }

    mod_times
}

/// cleanup_path is a function that removes all files and directories
/// in the given path that are not expected. It will return the number
/// of files and directories removed, and a list of the paths that were
/// removed.
pub fn cleanup_path(
    path: impl AsRef<Path>,
    expected_paths: Vec<impl AsRef<Path>>,
    progress_handler: &dyn ProgressHandler,
    remove_root: bool,
) -> Result<(bool, usize, Vec<PathBuf>), UpError> {
    // Convert the root path to a path buffer
    let path = PathBuf::from(path.as_ref());

    // Exit early if the root path does not exist
    if !path.exists() {
        return Ok((false, 0, vec![]));
    }

    // Exit early on error if the root path is not a directory
    if !path.is_dir() {
        return Err(UpError::Exec(format!(
            "expected directory, got: {}",
            path.display()
        )));
    }

    // Convert the expected paths to path buffers and filter out the
    // paths that are not in the root path
    let expected_paths = expected_paths
        .into_iter()
        .map(|p| PathBuf::from(p.as_ref()))
        .filter(|p| p.starts_with(&path))
        .collect::<Vec<_>>();

    // If there are no expected data paths, we can remove the workdir
    // data path entirely
    if expected_paths.is_empty() {
        if remove_root {
            progress_handler.progress(format!("removing {}", path.display()));
            force_remove_dir_all(path.clone()).map_err(|err| {
                UpError::Exec(format!("failed to remove {}: {}", path.display(), err))
            })?;
        }
        return Ok((remove_root, 0, vec![]));
    }

    // If there are expected paths, we want to do a breadth-first search
    // so that we can remove paths fast when they are not expected; we
    // can stop in depth when we find a path that is expected (since it
    // means that any deeper path is also expected)
    let mut known_unknown_paths = vec![];
    let mut removed_paths = vec![];
    for entry in walkdir::WalkDir::new(path.clone())
        .into_iter()
        .filter_entry(|e| {
            // If the path is the root, we want to keep it
            if e.path() == path {
                return true;
            }

            // Check if the path is known, in which case we can skip it
            // and its children
            if any(expected_paths.iter(), |expected_path| {
                e.path() == *expected_path
            }) {
                return false;
            }

            // If we're here, the path is not known, but we want to keep
            // digging if it is the beginning of a known path; we will need
            // to filter those paths out after
            if any(expected_paths.iter(), |expected_path| {
                expected_path.starts_with(e.path())
            }) {
                return true;
            }

            // If we're here, the path is not known and is not the beginning
            // of a known path, so we want to keep it as it will need to get
            // removed; however, we don't want to dig indefinitely, so we will
            // keep track of paths that we already marked as unknown, so we
            // can skip their children
            if any(known_unknown_paths.iter(), |unknown_path| {
                e.path().starts_with(unknown_path)
            }) {
                return false;
            }

            // If we're here, the path is not known and is not the beginning
            // of a known path, so we want to keep it as it will need to get
            known_unknown_paths.push(e.path().to_path_buf());
            true
        })
        .filter_map(|e| e.ok())
        // Filter the parents of known paths since we don't want to remove them
        .filter(|e| {
            !any(expected_paths.iter(), |expected_path| {
                expected_path.starts_with(e.path())
            })
        })
    {
        let path = entry.path();

        progress_handler.progress(format!("removing {}", path.display()));

        if path.is_file() {
            if let Err(error) = std::fs::remove_file(path) {
                return Err(UpError::Exec(format!(
                    "failed to remove {}: {}",
                    path.display(),
                    error
                )));
            }
            removed_paths.push(path.to_path_buf());
        } else if path.is_dir() {
            force_remove_dir_all(path).map_err(|err| {
                UpError::Exec(format!("failed to remove{}: {}", path.display(), err))
            })?;
            removed_paths.push(path.to_path_buf());
        } else {
            return Err(UpError::Exec(format!(
                "unexpected path type: {}",
                path.display()
            )));
        }
    }

    let num_removed = removed_paths.len();
    Ok((false, num_removed, removed_paths))
}
