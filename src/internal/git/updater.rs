use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use indicatif::MultiProgress;
use tempfile::NamedTempFile;
use time::format_description::well_known::Rfc3339;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::CacheObject;
use crate::internal::cache::OmniPathCache;
use crate::internal::commands::path::global_omnipath_entries;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::run_command_with_handler;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::current_exe;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::path_entry_config;
use crate::internal::git_env;
use crate::internal::self_update;
use crate::internal::user_interface::ensure_newline;
use crate::internal::user_interface::StringColor;
use crate::omni_error;
use crate::omni_info;
use crate::omni_print;
use crate::omni_warning;

fn should_update() -> bool {
    // Check if OMNI_SKIP_UPDATE is set
    if let Some(skip_update) = std::env::var_os("OMNI_SKIP_UPDATE") {
        if !skip_update.to_str().unwrap().is_empty() {
            return false;
        }
    }
    // Check if OMNI_FORCE_UPDATE is set
    if let Some(force_update) = std::env::var_os("OMNI_FORCE_UPDATE") {
        if !force_update.to_str().unwrap().is_empty() {
            // Unset that environment variable so that we don't
            // propagate it to any child process
            std::env::remove_var("OMNI_FORCE_UPDATE");

            return true;
        }
    }

    // Check if interactive shell, or skip update
    if !shell_is_interactive() {
        return false;
    }

    // Check first without exclusive lock (less costly)
    let mut require_update = false;
    if !OmniPathCache::get().updated() {
        // If the update is due, let's take the lock and check again
        if let Err(err) = OmniPathCache::exclusive(|omnipath| {
            if !omnipath.updated() {
                omnipath.update();
                require_update = true;
            }
            require_update
        }) {
            omni_error!(format!("Failed to update cache (update skipped): {}", err));
            return false;
        }
    }

    require_update
}

pub fn auto_update_async(current_command_path: Option<PathBuf>) {
    update(
        false,
        true,
        if let Some(current_command_path) = current_command_path {
            vec![current_command_path]
        } else {
            vec![]
        },
    );
}

pub fn auto_update_sync() -> bool {
    let (updated, _errored) = update(false, false, vec![]);
    !updated.is_empty()
}

pub fn exec_update() {
    let (_updated, errored) = update(true, false, vec![]);
    exit(if !errored.is_empty() { 1 } else { 0 });
}

pub fn exec_update_and_log_on_error() {
    let mut cmd = TokioCommand::new(current_exe());
    cmd.arg("--update");
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let log_file_prefix = format!(
        "omni-update.{}.",
        time::OffsetDateTime::now_utc()
            .replace_nanosecond(0)
            .unwrap()
            .format(&Rfc3339)
            .expect("failed to format date")
            .replace(['-', ':'], ""), // Remove the dashes in the date and the colons in the time
    );
    let mut log_file = match NamedTempFile::with_prefix(log_file_prefix.as_str()) {
        Ok(file) => file,
        Err(err) => {
            omni_error!(format!("failed to create temporary file: {}", err));
            exit(1);
        }
    };

    omni_info!("running update in the background");

    // Make a simple handler to append to the log file
    let handler_fn = |stdout: Option<String>, stderr: Option<String>| {
        if let Some(s) = stdout {
            log_file.write_all(s.as_bytes()).unwrap();
            log_file.write_all(b"\n").unwrap();
        }
        if let Some(s) = stderr {
            log_file.write_all(s.as_bytes()).unwrap();
            log_file.write_all(b"\n").unwrap();
        }
    };

    let config = global_config();
    let result = run_command_with_handler(
        &mut cmd,
        handler_fn,
        RunConfig::new().with_timeout(config.path_repo_updates.background_updates_timeout),
    );

    match result {
        Ok(_) => {
            // Get rid of the log file
            match log_file.close() {
                Ok(_) => {
                    omni_info!("update successful");
                }
                Err(err) => {
                    omni_warning!(format!("failed to close log file: {}", err));
                }
            };
        }
        Err(err) => {
            omni_error!(format!("update failed: {}", err));
            match log_file.keep() {
                Ok((_file, path)) => {
                    omni_info!(format!("log file kept at {}", path.display()));
                    if let Err(err) = OmniPathCache::exclusive(|omnipath| {
                        omnipath.update_error(path.to_string_lossy().to_string());
                        true
                    }) {
                        omni_error!(format!("failed to update cache: {}", err));
                    }
                }
                Err(err) => {
                    omni_error!(format!("failed to keep log file: {}", err));
                }
            };
            exit(1);
        }
    }

    exit(0);
}

pub fn report_update_error() {
    // The omni hook sets the SHELL_PPID environment variable to the parent process id
    // when being called from the shell prompt hook. If that variable is set, we know
    // that we are being called from the shell prompt hook and we can check if the
    // previous update errored and report it to the user.
    let is_user_shell = if let Some(shell_ppid) = std::env::var_os("OMNI_SHELL_PPID") {
        !shell_ppid.is_empty()
    } else {
        false
    };

    if is_user_shell && OmniPathCache::get().update_errored() {
        if let Err(err) = OmniPathCache::exclusive(|omnipath| {
            if omnipath.update_errored() {
                omni_print!(format!(
                    "background update failed; log is available at {}",
                    omnipath.update_error_log()
                )
                .light_red());
                omnipath.clear_update_error();
                true
            } else {
                false
            }
        }) {
            omni_error!(format!("failed to update cache: {}", err));
        }
    }
}

pub fn trigger_background_update(skip_paths: Vec<PathBuf>) -> bool {
    let mut command = Command::new(current_exe());
    command.arg("--update-and-log-on-error");
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::null());

    // Skip self-updates in the background
    command.env("OMNI_SKIP_SELF_UPDATE", "1");

    // Skip updates for the paths that were passed as arguments
    if skip_paths.is_empty() {
        command.env_remove("OMNI_SKIP_UPDATE_PATH");
    } else {
        let skip_path_env_var = skip_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(":");

        command.env("OMNI_SKIP_UPDATE_PATH", skip_path_env_var);
    }

    match command.spawn() {
        Ok(_child) => true,
        Err(_err) => false,
    }
}

pub fn update(
    force_update: bool,
    allow_background_update: bool,
    force_sync: Vec<PathBuf>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    // Get the configuration
    let config = global_config();

    // Get the omnipath
    let omnipath_entries = global_omnipath_entries();

    // Check if OMNI_SKIP_UPDATE_PATH is set, in which case we
    // can parse it into a list of paths to skip
    let skip_update_path: Vec<PathBuf> =
        if let Some(skip_update_path) = std::env::var_os("OMNI_SKIP_UPDATE_PATH") {
            skip_update_path
                .to_str()
                .unwrap()
                .split(':')
                .map(PathBuf::from)
                .collect()
        } else {
            vec![]
        };

    // Nothing to do if nothing is in the omnipath and we don't
    // check for omni updates
    if omnipath_entries.is_empty() && config.path_repo_updates.self_update.do_not_check() {
        return (vec![], vec![]);
    }

    if !force_update && !should_update() {
        return (vec![], vec![]);
    }

    self_update();

    if omnipath_entries.is_empty() {
        return (vec![], vec![]);
    }

    // Override allow_background_update if the configuration does not allow it
    let allow_background_update = if !config.path_repo_updates.background_updates {
        false
    } else {
        allow_background_update
    };

    let mut count_left_to_update = 0;
    let mut updates_per_path = HashMap::new();
    if allow_background_update && force_sync.is_empty() {
        count_left_to_update = omnipath_entries.len();
    } else {
        // Let's do all the git updates in parallel since we don't require
        // switching directory for that
        let multiprogress = MultiProgress::new();
        let mut threads = Vec::new();
        let (sender, receiver) = mpsc::channel();
        let mut seen = HashSet::new();

        for path_entry in omnipath_entries {
            let path = path_entry.as_string();

            let git_env = git_env(&path).clone();
            let repo_id = git_env.id();
            if repo_id.is_none() {
                continue;
            }
            let repo_id = repo_id.unwrap();
            let repo_root = git_env.root().unwrap().to_string();

            // Skip if the path is in the list of paths to skip
            let repo_root_path = PathBuf::from(repo_root.clone());
            if skip_update_path
                .iter()
                .any(|skip_update_path| skip_update_path == &repo_root_path)
            {
                continue;
            }

            // Check if we want to update this repository synchronously
            // or if we can delegate it to the background update
            if allow_background_update
                && !force_sync
                    .iter()
                    .any(|force_sync_path| path_entry.includes_path(force_sync_path.clone()))
            {
                count_left_to_update += 1;
                continue;
            }

            // Avoid updating the same repository multiple times
            if !seen.insert(repo_root.clone()) {
                continue;
            }

            // Get the configuration for that repository
            let (enabled, ref_type, ref_match) = config.path_repo_updates.update_config(&repo_id);

            if !enabled {
                // Skipping repository if updates are not enabled for it
                continue;
            }

            let desc = format!(
                "Updating {} {}:",
                if path_entry.is_package() {
                    "📦"
                } else {
                    "🌳"
                },
                repo_id.italic().light_cyan()
            )
            .light_blue();
            let progress_handler: Box<dyn ProgressHandler + Send> = if shell_is_interactive() {
                let mut spinner =
                    SpinnerProgressHandler::new_with_multi(desc, None, multiprogress.clone());
                spinner.no_newline_on_error();
                Box::new(spinner)
            } else {
                Box::new(PrintProgressHandler::new(desc, None))
            };

            let _multiprogress = multiprogress.clone();
            let sender = sender.clone();
            threads.push(thread::spawn(move || {
                let result = update_git_repo(
                    &repo_id,
                    ref_type,
                    ref_match,
                    Some(&repo_root.clone()),
                    Some(progress_handler.as_ref()),
                );

                sender.send((repo_root, result)).unwrap();
            }));
        }

        for thread in threads {
            let _ = thread.join();
        }

        let mut results = HashMap::new();
        while let Ok((path, updated)) = receiver.recv_timeout(Duration::from_millis(10)) {
            updates_per_path.insert(path.clone(), updated.clone());
            if let Ok(true) = updated {
                results.insert(path, true);
            }
        }

        // multiprogress.clear().unwrap();

        if !results.is_empty() {
            let current_exe = std::env::current_exe();
            if current_exe.is_err() {
                omni_error!("failed to get current executable path", "updater");
                exit(1);
            }
            let current_exe = current_exe.unwrap();

            for (repo_path, _updated) in results.iter() {
                let path_entry = path_entry_config(repo_path);
                if !path_entry.is_valid() {
                    continue;
                }

                let location = match path_entry.package {
                    Some(ref package) => {
                        format!("{}:{}", "package".underline(), package.light_cyan(),)
                    }
                    None => path_entry.as_string().light_cyan(),
                };

                omni_info!(format!(
                    "running {} in {}",
                    "omni up".light_yellow(),
                    location,
                ));

                let mut omni_up_command = std::process::Command::new(current_exe.clone());
                omni_up_command.arg("up");
                omni_up_command.current_dir(repo_path);
                omni_up_command.env_remove("OMNI_FORCE_UPDATE");
                omni_up_command.env("OMNI_SKIP_UPDATE", "1");

                let child = omni_up_command.spawn();
                match child {
                    Ok(mut child) => {
                        let status = child.wait();
                        match status {
                            Ok(status) => {
                                if !status.success() {
                                    updates_per_path.insert(
                                        repo_path.clone(),
                                        Err("omni up failed".to_string()),
                                    );
                                }
                            }
                            Err(err) => {
                                let msg = format!("failed to wait on process: {}", err);
                                omni_error!(msg.clone());
                                updates_per_path.insert(
                                    repo_path.clone(),
                                    Err(format!("omni up failed: {}", msg)),
                                );
                            }
                        }
                    }
                    Err(err) => {
                        let msg = format!("failed to spawn process: {}", err);
                        omni_error!(msg.clone());
                        updates_per_path
                            .insert(repo_path.clone(), Err(format!("omni up failed: {}", msg)));
                    }
                }
            }
        }
    }

    let updated_paths = updates_per_path
        .iter()
        .filter_map(|(path, updated)| {
            if let Ok(true) = updated {
                Some(PathBuf::from(path))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let errored_paths = updates_per_path
        .iter()
        .filter_map(|(path, updated)| {
            if updated.is_err() {
                Some(PathBuf::from(path))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // If we need to update in the background, let's do that now
    ensure_newline();
    if allow_background_update && count_left_to_update > 0 {
        trigger_background_update(
            skip_update_path
                .iter()
                .cloned()
                .chain(
                    updates_per_path
                        .keys()
                        .map(PathBuf::from)
                        .collect::<Vec<_>>(),
                )
                .collect(),
        );
        omni_info!(format!(
            "{}{}",
            if !errored_paths.is_empty() {
                "error! ".light_red()
            } else if updated_paths.is_empty() {
                "".to_string()
            } else {
                "done! ".light_green()
            },
            format!(
                "{} {} {}",
                "updating".light_black(),
                count_left_to_update.to_string().light_yellow(),
                format!(
                    "path{} in the background",
                    if count_left_to_update > 1 { "s" } else { "" }
                )
                .light_black(),
            )
            .italic(),
        ));
    } else if !errored_paths.is_empty() {
        omni_info!("error!".light_red());
    } else {
        omni_info!("done!".light_green());
    }

    // Return the list of updated paths
    (updated_paths, errored_paths)
}

pub fn update_git_repo(
    repo_id: &str,
    ref_type: String,
    ref_match: Option<String>,
    repo_path: Option<&str>,
    progress_handler: Option<&dyn ProgressHandler>,
) -> Result<bool, String> {
    let desc = format!("Updating {}:", repo_id.italic().light_cyan()).light_blue();
    let spinner;
    let printer;

    let progress_handler: Box<&dyn ProgressHandler> =
        if let Some(progress_handler) = progress_handler {
            Box::new(progress_handler)
        } else if shell_is_interactive() {
            spinner = SpinnerProgressHandler::new(desc, None);
            Box::new(&spinner)
        } else {
            printer = PrintProgressHandler::new(desc, None);
            Box::new(&printer)
        };

    match ref_type.as_str() {
        "branch" => update_git_branch(repo_id, ref_match, repo_path, Some(*progress_handler)),
        "tag" => update_git_tag(repo_id, ref_match, repo_path, Some(*progress_handler)),
        _ => {
            let msg = format!("invalid ref type: {}", ref_type);
            progress_handler.error_with_message(msg.clone());
            Err(msg)
        }
    }
}

fn update_git_branch(
    repo_id: &str,
    ref_match: Option<String>,
    repo_path: Option<&str>,
    progress_handler: Option<&dyn ProgressHandler>,
) -> Result<bool, String> {
    let desc = format!("Updating {}:", repo_id.italic().light_cyan()).light_blue();
    let spinner;
    let printer;

    let progress_handler: Box<&dyn ProgressHandler> =
        if let Some(progress_handler) = progress_handler {
            Box::new(progress_handler)
        } else if shell_is_interactive() {
            spinner = SpinnerProgressHandler::new(desc, None);
            Box::new(&spinner)
        } else {
            printer = PrintProgressHandler::new(desc, None);
            Box::new(&printer)
        };

    progress_handler.progress("checking current branch".to_string());

    // Check if the currently checked out branch matches the one we want to update
    let mut git_branch_cmd = std::process::Command::new("git");
    if let Some(repo_path) = repo_path {
        git_branch_cmd.current_dir(repo_path);
    }
    git_branch_cmd.arg("branch");
    git_branch_cmd.arg("--show-current");
    git_branch_cmd.stdout(std::process::Stdio::piped());
    git_branch_cmd.stderr(std::process::Stdio::null());

    let output = git_branch_cmd.output();
    if output.is_err() || !output.as_ref().unwrap().status.success() {
        let msg = "git branch failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }
    let git_branch = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if git_branch.is_empty() {
        let msg = "not currently checked out on a branch; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    let regex = match ref_match {
        Some(ref ref_match) => regex::Regex::new(ref_match),
        None => regex::Regex::new(".*"),
    };
    if regex.is_err() {
        let msg = format!("invalid ref match: {}", ref_match.unwrap());
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    if !regex.unwrap().is_match(&git_branch) {
        let msg = format!(
            "current branch {} does not match {}; skipping update",
            git_branch,
            ref_match.unwrap()
        );
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    progress_handler.progress("pulling latest changes".to_string());

    let mut git_pull_cmd = std::process::Command::new("git");
    if let Some(repo_path) = repo_path {
        git_pull_cmd.current_dir(repo_path);
    }
    git_pull_cmd.arg("pull");
    git_pull_cmd.arg("--ff-only");
    git_pull_cmd.stdout(std::process::Stdio::piped());
    git_pull_cmd.stderr(std::process::Stdio::piped());

    match git_pull_cmd.output() {
        Err(err) => {
            let msg = format!("git pull failed: {}", err);
            progress_handler.error_with_message(msg.clone());
            Err(msg)
        }
        Ok(output) if !output.status.success() => {
            let msg = format!(
                "git pull failed: {}",
                String::from_utf8(output.stderr)
                    .unwrap()
                    .replace('\n', " ")
                    .trim()
            );
            progress_handler.error_with_message(msg.clone());
            Err(msg)
        }
        Ok(output) => {
            let output = String::from_utf8(output.stdout).unwrap().trim().to_string();
            let output_lines = output.lines().collect::<Vec<&str>>();

            if output_lines.len() == 1 && output_lines[0].contains("Already up to date.") {
                progress_handler.success_with_message("already up to date".light_black());
                Ok(false)
            } else {
                progress_handler.success_with_message("updated".light_green());
                Ok(true)
            }
        }
    }
}

fn update_git_tag(
    repo_id: &str,
    ref_match: Option<String>,
    repo_path: Option<&str>,
    progress_handler: Option<&dyn ProgressHandler>,
) -> Result<bool, String> {
    let desc = format!("Updating {}:", repo_id.italic().light_cyan()).light_blue();
    let spinner;
    let printer;

    let progress_handler: Box<&dyn ProgressHandler> =
        if let Some(progress_handler) = progress_handler {
            Box::new(progress_handler)
        } else if shell_is_interactive() {
            spinner = SpinnerProgressHandler::new(desc, None);
            Box::new(&spinner)
        } else {
            printer = PrintProgressHandler::new(desc, None);
            Box::new(&printer)
        };

    // Check if we're currently checked out on a branch
    progress_handler.progress("checking current branch".to_string());
    let mut git_branch_cmd = std::process::Command::new("git");
    if let Some(repo_path) = repo_path {
        git_branch_cmd.current_dir(repo_path);
    }
    git_branch_cmd.arg("branch");
    git_branch_cmd.arg("--show-current");
    git_branch_cmd.stdout(std::process::Stdio::piped());
    git_branch_cmd.stderr(std::process::Stdio::null());

    let output = git_branch_cmd.output();
    if output.is_err() || !output.as_ref().unwrap().status.success() {
        let msg = "git branch failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }
    let git_branch = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if !git_branch.is_empty() {
        let msg = "currently checked out on a branch; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Check which tag we are currently checked out on, if any
    progress_handler.progress("checking current tag".to_string());
    let mut git_tag_cmd = std::process::Command::new("git");
    if let Some(repo_path) = repo_path {
        git_tag_cmd.current_dir(repo_path);
    }
    git_tag_cmd.arg("tag");
    git_tag_cmd.arg("--points-at");
    git_tag_cmd.arg("HEAD");
    git_tag_cmd.arg("--sort=-creatordate");
    git_tag_cmd.stdout(std::process::Stdio::piped());
    git_tag_cmd.stderr(std::process::Stdio::null());

    let output = git_tag_cmd.output();
    if output.is_err() || !output.as_ref().unwrap().status.success() {
        let msg = "git tag failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }
    let git_tag = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if git_tag.is_empty() {
        let msg = "not currently checked out on a tag; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Consider the latest tag built on this commit to be the current tag
    let current_tag = git_tag.lines().collect::<Vec<&str>>()[0].to_string();

    // Fetch all the tags for the repository
    progress_handler.progress("fetching last tags".to_string());
    let mut git_fetch_tags_cmd = std::process::Command::new("git");
    if let Some(repo_path) = repo_path {
        git_fetch_tags_cmd.current_dir(repo_path);
    }
    git_fetch_tags_cmd.arg("fetch");
    git_fetch_tags_cmd.arg("--tags");
    git_fetch_tags_cmd.stdout(std::process::Stdio::piped());
    git_fetch_tags_cmd.stderr(std::process::Stdio::piped());

    let output = git_fetch_tags_cmd.output();
    if output.is_err() || !output.as_ref().unwrap().status.success() {
        let msg = "git fetch failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Check if there was any new tags fetched
    let fetched = output.unwrap();
    let fetched_out = String::from_utf8(fetched.stdout).unwrap();
    let fetched_err = String::from_utf8(fetched.stderr).unwrap();
    if fetched_out.trim().is_empty() && fetched_err.trim().is_empty() {
        // If no new tags, nothing more to do!
        progress_handler.success_with_message("no new tags, nothing to do".light_black());
        return Ok(false);
    }

    // If any new tags, we need to check what is the most recent tag
    // that matches the passed tag parameter (if any)
    progress_handler.progress("checking latest tag".to_string());
    let mut git_tag_cmd = std::process::Command::new("git");
    if let Some(repo_path) = repo_path {
        git_tag_cmd.current_dir(repo_path);
    }
    git_tag_cmd.arg("tag");
    git_tag_cmd.arg("--sort=-creatordate");
    git_tag_cmd.stdout(std::process::Stdio::piped());
    git_tag_cmd.stderr(std::process::Stdio::null());

    let output = git_tag_cmd.output();
    if output.is_err() || !output.as_ref().unwrap().status.success() {
        let msg = "git tag failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }
    let git_tags = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if git_tags.is_empty() {
        let msg = "no tags found; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Find the most recent git tag in git_tags that matches
    // the passed tag parameter (if any)
    let regex = match ref_match {
        Some(ref ref_match) => regex::Regex::new(ref_match),
        None => regex::Regex::new(".*"),
    };
    if regex.is_err() {
        let msg = "invalid tag regex".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }
    let regex = regex.unwrap();

    let target_tag = git_tags.lines().find(|git_tag| regex.is_match(git_tag));
    if target_tag.is_none() {
        let msg = "no matching tags found; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }
    let target_tag = target_tag.unwrap().trim().to_string();

    // If the current tag is the same as the target tag, nothing more to do!
    if current_tag == target_tag {
        progress_handler.success_with_message("already on latest matching tag".light_black());
        return Ok(false);
    }

    // Check out the target tag
    progress_handler.progress(format!("checking out {}", target_tag.light_green()));
    let mut git_checkout_cmd = std::process::Command::new("git");
    if let Some(repo_path) = repo_path {
        git_checkout_cmd.current_dir(repo_path);
    }
    git_checkout_cmd.arg("checkout");
    git_checkout_cmd.arg("--no-guess");
    git_checkout_cmd.arg(&target_tag);
    git_checkout_cmd.stdout(std::process::Stdio::null());
    git_checkout_cmd.stderr(std::process::Stdio::null());

    let output = git_checkout_cmd.output();
    if output.is_err() || !output.as_ref().unwrap().status.success() {
        let msg = "git checkout failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    progress_handler.success_with_message("updated".light_green());

    Ok(true)
}
