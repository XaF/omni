use std::collections::HashMap;
use std::collections::HashSet;
use std::process::exit;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use indicatif::MultiProgress;

use crate::internal::cache::CacheObject;
use crate::internal::cache::OmniPathCache;
use crate::internal::commands::path::global_omnipath_entries;
use crate::internal::config::config;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::path_entry_config;
use crate::internal::git_env;
use crate::internal::self_update;
use crate::internal::user_interface::StringColor;
use crate::omni_error;
use crate::omni_info;

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

pub fn auto_path_update() {
    // Get the configuration
    let config = config(".");

    // Get the omnipath
    let omnipath_entries = global_omnipath_entries();
    if omnipath_entries.is_empty() && config.path_repo_updates.self_update.do_not_check() {
        // Nothing to do if nothing is in the omnipath and we
        // don't check for omni updates
        return;
    }

    if !should_update() {
        return;
    }

    self_update();

    if omnipath_entries.is_empty() {
        return;
    }

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
        if updated {
            results.insert(path, updated);
        }
    }

    // multiprogress.clear().unwrap();

    if results.is_empty() {
        return;
    }

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
            Some(ref package) => format!("{}:{}", "package".underline(), package.light_cyan(),),
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
                    Ok(_status) => {}
                    Err(err) => {
                        omni_error!(format!("failed to wait on process: {}", err));
                    }
                }
            }
            Err(err) => {
                omni_error!(format!("failed to spawn process: {}", err));
            }
        }
    }

    omni_info!("done!".light_green());
}

pub fn update_git_repo(
    repo_id: &str,
    ref_type: String,
    ref_match: Option<String>,
    repo_path: Option<&str>,
    progress_handler: Option<&dyn ProgressHandler>,
) -> bool {
    match ref_type.as_str() {
        "branch" => update_git_branch(repo_id, ref_match, repo_path, progress_handler),
        "tag" => update_git_tag(repo_id, ref_match, repo_path, progress_handler),
        _ => {
            omni_error!("invalid ref type: {}", ref_type.light_red());
            false
        }
    }
}

fn update_git_branch(
    repo_id: &str,
    ref_match: Option<String>,
    repo_path: Option<&str>,
    progress_handler: Option<&dyn ProgressHandler>,
) -> bool {
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
        progress_handler.error_with_message("git branch failed".to_string());
        return false;
    }
    let git_branch = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if git_branch.is_empty() {
        progress_handler.error_with_message(
            "not currently checked out on a branch; skipping update".to_string(),
        );
        return false;
    }

    let regex = match ref_match {
        Some(ref ref_match) => regex::Regex::new(ref_match),
        None => regex::Regex::new(".*"),
    };
    if regex.is_err() {
        progress_handler.error_with_message(format!(
            "invalid ref match: {}",
            ref_match.unwrap().light_red()
        ));
        return false;
    }

    if !regex.unwrap().is_match(&git_branch) {
        progress_handler.error_with_message(format!(
            "current branch {} does not match {}; skipping update",
            git_branch.light_red(),
            ref_match.unwrap().light_red()
        ));
        return false;
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
            progress_handler.error_with_message(format!("git pull failed: {}", err));
            false
        }
        Ok(output) if !output.status.success() => {
            progress_handler.error_with_message(format!(
                "git pull failed: {}",
                String::from_utf8(output.stderr)
                    .unwrap()
                    .replace('\n', " ")
                    .trim(),
            ));
            false
        }
        Ok(output) => {
            let output = String::from_utf8(output.stdout).unwrap().trim().to_string();
            let output_lines = output.lines().collect::<Vec<&str>>();

            if output_lines.len() == 1 && output_lines[0].contains("Already up to date.") {
                progress_handler.success_with_message("already up to date".light_black());
                false
            } else {
                progress_handler.success_with_message("updated".light_green());
                true
            }
        }
    }
}

fn update_git_tag(
    repo_id: &str,
    ref_match: Option<String>,
    repo_path: Option<&str>,
    progress_handler: Option<&dyn ProgressHandler>,
) -> bool {
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
        progress_handler.error_with_message("git branch failed".to_string());
        return false;
    }
    let git_branch = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if !git_branch.is_empty() {
        progress_handler
            .error_with_message("currently checked out on a branch; skipping update".to_string());
        return false;
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
        progress_handler.error_with_message("git tag failed".to_string());
        return false;
    }
    let git_tag = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if git_tag.is_empty() {
        progress_handler
            .error_with_message("not currently checked out on a tag; skipping update".to_string());
        return false;
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
        progress_handler.error_with_message("git fetch failed".to_string());
        return false;
    }

    // Check if there was any new tags fetched
    let fetched = output.unwrap();
    let fetched_out = String::from_utf8(fetched.stdout).unwrap();
    let fetched_err = String::from_utf8(fetched.stderr).unwrap();
    if fetched_out.trim().is_empty() && fetched_err.trim().is_empty() {
        // If no new tags, nothing more to do!
        progress_handler.success_with_message("no new tags, nothing to do".light_black());
        return false;
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
        progress_handler.error_with_message("git tag failed".to_string());
        return false;
    }
    let git_tags = String::from_utf8(output.unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    if git_tags.is_empty() {
        progress_handler.error_with_message("no tags found; skipping update".to_string());
        return false;
    }

    // Find the most recent git tag in git_tags that matches
    // the passed tag parameter (if any)
    let regex = match ref_match {
        Some(ref ref_match) => regex::Regex::new(ref_match),
        None => regex::Regex::new(".*"),
    };
    if regex.is_err() {
        progress_handler.error_with_message("invalid tag regex".to_string());
        return false;
    }
    let regex = regex.unwrap();

    let target_tag = git_tags.lines().find(|git_tag| regex.is_match(git_tag));
    if target_tag.is_none() {
        progress_handler.error_with_message("no matching tags found; skipping update".to_string());
        return false;
    }
    let target_tag = target_tag.unwrap().trim().to_string();

    // If the current tag is the same as the target tag, nothing more to do!
    if current_tag == target_tag {
        progress_handler.success_with_message("already on latest matching tag".light_black());
        return false;
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
        progress_handler.error_with_message("git checkout failed".to_string());
        return false;
    }

    progress_handler.success_with_message("updated".light_green());

    true
}
