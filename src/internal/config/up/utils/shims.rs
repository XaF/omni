use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::exit;

use crate::internal::commands::utils::abs_path;
use crate::internal::config::up::asdf_base::asdf_path;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::UpError;
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

    // Load the current path environment variable
    let path_env = env::var("PATH").unwrap_or_else(|_| "".to_string());

    // Check if argv0 is a path, or if it is just a binary name called from the PATH
    let path = if argv0.contains('/') {
        abs_path(&argv0)
    } else {
        path_env
            .split(':')
            .map(|path| PathBuf::from(path).join(&argv0))
            .find(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from(&argv0))
    };

    // Check if argv0 is a shim, i.e. if its path is in the
    // shims directory
    if !path.starts_with(&shims_dir()) {
        return;
    }

    // Since argv0 is a shim, let's extract the binary name from it
    let binary = path
        .file_name()
        .map(|file_name| file_name.to_string_lossy().to_string())
        .unwrap_or_else(|| argv0);

    // Load the dynamic environment for the current directory
    update_dynamic_env_for_command(".");

    // Make sure that the PATH does not contain the shims directory
    let path_env = path_env
        .split(':')
        .filter(|path| !PathBuf::from(path).starts_with(&shims_dir()))
        .collect::<Vec<_>>()
        .join(":");
    env::set_var("PATH", &path_env);

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
    let err = std::process::Command::new(&binary_path).args(&args).exec();

    // If we reach this point, it means that the exec failed
    // so we'll print the error and exit with 126
    eprintln!("{}: {}", binary, err);
    exit(126);
}

pub fn reshim(progress_handler: &dyn ProgressHandler) -> Result<Option<String>, UpError> {
    // Get all the directories that we need to build shims for
    let mut shims_sources = vec![];

    // The default asdf shims
    shims_sources.push(PathBuf::from(asdf_path()).join("shims"));

    // Use a glob to get the shims from the different tools bin
    // directories in the isolated workdir environments
    let venv_glob = format!("{}/wd/*/*/*/*/bin", data_home());
    for entry in glob::glob(&venv_glob).unwrap() {
        if let Ok(path) = entry {
            shims_sources.push(path);
        }
    }

    // Use a glob to get the shims from the github release tools
    // bin directories
    let gh_glob = format!("{}/ghreleases/*/*/*", data_home());
    for entry in glob::glob(&gh_glob).unwrap() {
        if let Ok(path) = entry {
            shims_sources.push(path);
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

            let metadata = match path.metadata() {
                Ok(metadata) => metadata,
                Err(_err) => continue,
            };

            let is_executable = metadata.permissions().mode() & 0o111 != 0;
            if !is_executable {
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

    if !shims_to_create.is_empty() {
        // Create the shims directory
        if !shims_dir().exists() {
            std::fs::create_dir_all(&shims_dir()).map_err(|err| {
                UpError::Exec(format!(
                    "failed to create shims directory {}: {}",
                    shims_dir().display(),
                    err
                ))
            })?;
        }

        // Create the shims as a symlink to the current executable
        let target = current_exe();
        for shim in &shims_to_create {
            if shim.is_file() || shim.is_symlink() {
                fs::remove_file(shim).map_err(|err| {
                    UpError::Exec(format!(
                        "failed to remove existing file {}: {}",
                        shim.display(),
                        err
                    ))
                })?;
            }
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
        cleanup_path(&shims_dir(), expected_shims, progress_handler, false)?;

    let num_created = shims_to_create.len();
    let msg = if root_removed {
        Some(format!("removed shims directory"))
    } else if num_created > 0 || num_removed > 0 {
        let mut msg = String::new();
        if num_created > 0 {
            msg.push_str(&format!("+{}", num_created).light_green());
        }
        if num_removed > 0 {
            if num_created > 0 {
                msg.push_str(", ");
            }
            msg.push_str(&format!("-{}", num_removed).light_red());
        }
        msg.push_str(&format!(
            " shim{}",
            if num_created + num_removed > 1 {
                "s"
            } else {
                ""
            }
        ));
        Some(msg)
    } else {
        None
    };

    Ok(msg)
}
