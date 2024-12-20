use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::exit;

use crate::internal::commands::utils::abs_path;
use crate::internal::config::up::mise::mise_path;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::directory::force_remove_all;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::utils::is_executable;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::env::current_exe;
use crate::internal::env::data_home;
use crate::internal::env::shims_dir;
use crate::internal::user_interface::StringColor;

pub fn handle_shims() {
    let argv0 = match env::args().next() {
        Some(argv0) => argv0,
        None => return,
    };

    // Check if argv0 is a path, or if it is just a binary name called from the PATH
    let path = if argv0.contains('/') {
        abs_path(&argv0)
    } else {
        env::var("PATH")
            .unwrap_or_else(|_| "".to_string())
            .split(':')
            .map(|path| PathBuf::from(path).join(&argv0))
            .find(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from(&argv0))
    };

    // Check if argv0 is a shim, i.e. if its path is in the
    // shims directory
    if !path.starts_with(shims_dir()) {
        return;
    }

    // Since argv0 is a shim, let's extract the binary name from it
    let binary = path
        .file_name()
        .map(|file_name| file_name.to_string_lossy().to_string())
        .unwrap_or_else(|| argv0);

    // Load the dynamic environment for the current directory
    update_dynamic_env_for_command(".");

    // Resolve the binary full path
    let binary_path = match which::which(&binary) {
        Ok(binary_path) => binary_path,
        Err(err) => {
            // Exit with 127 if the binary was not found
            eprintln!("{}: {}", binary, err);
            exit(127);
        }
    };

    // Get all the other arguments we received
    let args = env::args().skip(1).collect::<Vec<_>>();

    // Now replace the current process with the binary
    let err = std::process::Command::new(binary_path).args(args).exec();

    // If we reach this point, it means that the exec failed
    // so we'll print the error and exit with 126
    eprintln!("{}: {}", binary, err);
    exit(126);
}

pub fn reshim(progress_handler: &dyn ProgressHandler) -> Result<Option<String>, UpError> {
    // Get all the directories that we need to build shims for
    let mut shims_sources = vec![];

    // The default mise shims
    shims_sources.push(PathBuf::from(mise_path()).join("shims"));

    // Use a glob to get the shims from the different tools bin
    // directories in the isolated workdir environments
    let venv_glob = format!("{}/wd/*/*/*/*/bin", data_home());
    if let Ok(entries) = glob::glob(&venv_glob) {
        for entry in entries.flatten() {
            shims_sources.push(entry);
        }
    }

    // Use a glob to get the shims from the github release tools
    // bin directories
    let gh_glob = format!("{}/ghreleases/*/*/*", data_home());
    if let Ok(entries) = glob::glob(&gh_glob) {
        for entry in entries.flatten() {
            shims_sources.push(entry);
        }
    }

    // Use a glob to get the shims from the go-install tools
    // bin directories
    let go_glob = format!("{}/go-install/**/bin", data_home());
    if let Ok(entries) = glob::glob(&go_glob) {
        for entry in entries.flatten() {
            shims_sources.push(entry);
        }
    }

    // Use a glob to get the shims from the cargo-install tools
    // bin directories
    let cargo_glob = format!("{}/cargo-install/**/bin", data_home());
    if let Ok(entries) = glob::glob(&cargo_glob) {
        for entry in entries.flatten() {
            shims_sources.push(entry);
        }
    }

    // Figure out the required shims
    let mut expected_shims = BTreeSet::new();

    // Find all binaries in the source directories and add them
    // to the list of expected shims
    for shims_source in &shims_sources {
        if !shims_source.exists() {
            continue;
        }

        let read_dir = match shims_source.read_dir() {
            Ok(read_dir) => read_dir,
            Err(_err) => continue,
        };

        for entry in read_dir {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_err) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            if !is_executable(&path) {
                continue;
            }

            let filename = match path.file_name() {
                Some(filename) => filename.to_string_lossy().to_string(),
                None => continue,
            };

            expected_shims.insert(shims_dir().join(&filename));
        }
    }

    let shims_to_create = expected_shims
        .iter()
        .filter(|shim| !shim.exists())
        .collect::<Vec<_>>();

    let shims_to_update = expected_shims
        .iter()
        .filter(|shim| {
            // We can skip if the shim does not exist
            if !shim.exists() {
                return false;
            }

            // We need to update if it is a directory
            if shim.is_dir() {
                return true;
            }

            // We need to update if it is not a symlink
            if !shim.is_symlink() {
                return true;
            }

            // We need to update if the symlink target is not
            // the current executable
            let target = match fs::read_link(shim) {
                Ok(target) => target,
                Err(_) => return true,
            };

            target != current_exe()
        })
        .collect::<Vec<_>>();

    if !shims_to_update.is_empty() {
        for shim in &shims_to_update {
            progress_handler.progress(format!("removing {}", shim.display()));
            force_remove_all(shim).map_err(|err| {
                UpError::Exec(format!(
                    "failed to remove existing path {}: {}",
                    shim.display(),
                    err
                ))
            })?;
        }
    }

    // Create the shims directory if it does not exist and is needed
    if !shims_to_create.is_empty() && !shims_dir().exists() {
        std::fs::create_dir_all(shims_dir()).map_err(|err| {
            UpError::Exec(format!(
                "failed to create shims directory {}: {}",
                shims_dir().display(),
                err
            ))
        })?;
    }

    if !shims_to_create.is_empty() || !shims_to_update.is_empty() {
        // Create the shims as a symlink to the current executable
        let target = current_exe();
        for shim in shims_to_create.iter().chain(shims_to_update.iter()) {
            progress_handler.progress(format!("creating {}", shim.display()));
            symlink(&target, shim).map_err(|err| {
                UpError::Exec(format!(
                    "failed to create symlink for {}: {}",
                    shim.display(),
                    err
                ))
            })?;
        }
    }

    // Find all shims that are not needed anymore and clean them up
    let expected_shims: Vec<_> = expected_shims.iter().collect();
    let (root_removed, num_removed, _) =
        cleanup_path(shims_dir(), expected_shims, progress_handler, false)?;

    let num_created = shims_to_create.len();
    let num_updated = shims_to_update.len();
    let msg = if root_removed {
        Some("removed shims directory".to_string())
    } else if num_created > 0 || num_updated > 0 || num_removed > 0 {
        let mut counts = vec![];
        if num_created > 0 {
            counts.push(format!("+{}", num_created).light_green());
        }
        if num_updated > 0 {
            counts.push(format!("~{}", num_updated).light_yellow());
        }
        if num_removed > 0 {
            counts.push(format!("-{}", num_removed).light_red());
        }
        let msg = format!(
            "{} shim{}",
            counts.join(", "),
            if num_created + num_updated + num_removed > 1 {
                "s"
            } else {
                ""
            }
        );
        Some(msg)
    } else {
        None
    };

    Ok(msg)
}
