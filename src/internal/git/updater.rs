use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;
use std::process::Command as StdCommand;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use indicatif::MultiProgress;
use tempfile::NamedTempFile;
use time::format_description::well_known::Rfc3339;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::OmniPathCache;
use crate::internal::commands::base::Command;
use crate::internal::commands::path::global_omnipath_entries;
use crate::internal::config::global_config;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::config::parser::StringFilter;
use crate::internal::config::up::utils::get_command_output;
use crate::internal::config::up::utils::run_command_with_handler;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::current_exe;
use crate::internal::env::running_as_sudo;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::full_git_url_parse;
use crate::internal::git::path_entry_config;
use crate::internal::git_env;
use crate::internal::git_env_flush_cache;
use crate::internal::self_update;
use crate::internal::user_interface::ensure_newline;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
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

    OmniPathCache::get().try_exclusive_update()
}

pub fn auto_update_async(called_command: &Command) {
    let mut options = UpdateOptions::default();

    if called_command.requires_sync_update() && called_command.has_source() {
        let called_command_path_str = called_command.source();
        let called_command_path = Path::new(&called_command_path_str);
        options.add_sync_path(called_command_path);
    }

    update(&options);
}

pub fn auto_update_on_command_not_found() -> bool {
    let config = global_config();
    let should_update = config.path_repo_updates.on_command_not_found;
    if should_update.is_false() {
        return false;
    }

    let mut options = UpdateOptions::default();
    options.disable_background_update();

    if should_update.is_ask() {
        options.add_pre_update_validation_func(move || {
            omni_info!(format!(
                "{}; {}",
                "command not found".light_red(),
                "but it may exist in up-to-date repositories",
            ));
            let question = requestty::Question::confirm("command_not_found_update")
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message(format!(
                    "{} {}",
                    "omni:".light_cyan(),
                    "Do you want to run a sync update? (no = async)".light_yellow(),
                ))
                .default(true)
                .build();

            let answer = match requestty::prompt_one(question) {
                Ok(answer) => match answer {
                    requestty::Answer::Bool(confirmed) => confirmed,
                    _ => unreachable!(),
                },
                Err(err) => {
                    println!("{}", format!("[✘] {:?}", err).red());
                    false
                }
            };

            if !answer {
                // If background updates are authorized, let's trigger one
                if config.path_repo_updates.background_updates {
                    let mut options = UpdateOptions::default();
                    options.force_update();

                    update(&options);
                }
            }

            answer
        });
    }

    let (updated, _errored) = update(&options);
    !updated.is_empty()
}

pub fn exec_update() {
    let mut options = UpdateOptions::default();
    options.force_update();
    options.disable_background_update();

    let (_updated, errored) = update(&options);
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
                    if let Err(err) =
                        OmniPathCache::get().update_error(path.to_string_lossy().to_string())
                    {
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

    if is_user_shell {
        if let Some(error_log) = OmniPathCache::get().try_exclusive_update_error_log() {
            omni_print!(format!(
                "background update failed; log is available at {}",
                error_log,
            )
            .light_red());
        }
    }
}

pub fn trigger_background_update(skip_paths: HashSet<PathBuf>) -> bool {
    let mut command = StdCommand::new(current_exe());
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

pub struct UpdateOptions {
    force_update: bool,
    allow_background_update: bool,
    force_sync: Vec<PathBuf>,
    pre_update_validation_funcs: Vec<Box<dyn Fn() -> bool>>,
}

impl Debug for UpdateOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateOptions")
            .field("force_update", &self.force_update)
            .field("allow_background_update", &self.allow_background_update)
            .field("force_sync", &self.force_sync)
            .field(
                "pre_update_validation_funcs",
                &self.pre_update_validation_funcs.len(),
            )
            .finish()
    }
}

impl Default for UpdateOptions {
    fn default() -> Self {
        Self {
            force_update: false,
            allow_background_update: true,
            force_sync: vec![],
            pre_update_validation_funcs: vec![],
        }
    }
}

impl UpdateOptions {
    pub fn force_update(&mut self) -> &mut Self {
        self.force_update = true;
        self
    }

    pub fn disable_background_update(&mut self) -> &mut Self {
        self.allow_background_update = false;
        self
    }

    pub fn add_sync_path(&mut self, path: &Path) -> &mut Self {
        self.force_sync.push(path.to_path_buf());
        self
    }

    pub fn add_pre_update_validation_func<F>(&mut self, func: F) -> &mut Self
    where
        F: Fn() -> bool + 'static,
    {
        self.pre_update_validation_funcs.push(Box::new(func));
        self
    }

    fn background_update(&self) -> bool {
        self.allow_background_update && global_config().path_repo_updates.background_updates
    }

    fn should_update(&self) -> bool {
        if !self.force_update && !should_update() {
            return false;
        }

        if self.pre_update_validation_funcs.iter().any(|func| !func()) {
            return false;
        }

        true
    }
}

pub fn update(options: &UpdateOptions) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
    // Get the configuration
    let config = global_config();

    // Get the omnipath
    let omnipath_entries = global_omnipath_entries();

    // Check if OMNI_SKIP_UPDATE_PATH is set, in which case we
    // can parse it into a list of paths to skip
    let skip_update_path: HashSet<PathBuf> =
        if let Some(skip_update_path) = std::env::var_os("OMNI_SKIP_UPDATE_PATH") {
            skip_update_path
                .to_str()
                .unwrap()
                .split(':')
                .map(PathBuf::from)
                .collect()
        } else {
            HashSet::new()
        };

    // Nothing to do if nothing is in the omnipath and we don't
    // check for omni updates
    if omnipath_entries.is_empty() && config.path_repo_updates.self_update.do_not_check() {
        return (HashSet::new(), HashSet::new());
    }

    // Nothing to do if we don't need to update
    if !options.should_update() {
        return (HashSet::new(), HashSet::new());
    }

    self_update(false);

    // Nothing more to do if nothing is in the omnipath, or if we are running
    // as sudo since we don't want the repositories to be updated in that case
    if running_as_sudo() || omnipath_entries.is_empty() {
        return (HashSet::new(), HashSet::new());
    }

    // Make sure we run git fetch --dry-run at least once per host
    // to trigger ssh agent authentication if needed
    // TODO: disable that if no agent is setup for the given host
    let mut failed_early_auth = HashSet::new();
    if config.path_repo_updates.pre_auth {
        let mut auth_hosts = HashMap::new();
        for path_entry in &omnipath_entries {
            let git_env = git_env(path_entry.to_string());
            let repo_id = match git_env.id() {
                Some(repo_id) => repo_id,
                None => continue,
            };
            let repo_root = git_env.root().unwrap().to_string();

            if let Ok(git_url) = full_git_url_parse(&repo_id) {
                if let Some(host) = git_url.host {
                    let key = (host.clone(), git_url.scheme.to_string());

                    if let Some(succeeded) = auth_hosts.get(&key) {
                        if !succeeded {
                            failed_early_auth.insert(repo_root.clone());
                        }
                        continue;
                    }

                    // Check using git ls-remote
                    let mut cmd = TokioCommand::new("git");
                    cmd.arg("ls-remote");
                    cmd.current_dir(&repo_root);
                    cmd.stdout(std::process::Stdio::piped());
                    cmd.stderr(std::process::Stdio::piped());

                    let result = run_command_with_handler(
                        &mut cmd,
                        |_stdout, _stderr| {
                            // Do nothing
                        },
                        RunConfig::new().with_timeout(config.path_repo_updates.pre_auth_timeout),
                    );

                    auth_hosts.insert(key, result.is_ok());
                    if result.is_err() {
                        omni_error!(format!("failed to authenticate to {}", host.light_cyan()));
                        failed_early_auth.insert(repo_root);
                    }
                }
            }
        }
    }

    // Add the paths that failed early authentication
    // to the list of paths to skip
    let skip_update_path = skip_update_path
        .iter()
        .cloned()
        .chain(failed_early_auth.iter().map(PathBuf::from))
        .collect::<Vec<_>>();

    let mut count_left_to_update = 0;
    let mut updates_per_path = HashMap::new();
    if options.background_update() && options.force_sync.is_empty() {
        count_left_to_update = omnipath_entries.len();
    } else {
        // Let's do all the git updates in parallel since we don't require
        // switching directory for that
        let multiprogress = MultiProgress::new();
        let mut threads = Vec::new();
        let (sender, receiver) = mpsc::channel();
        let mut seen = HashSet::new();

        for path_entry in omnipath_entries {
            let path = path_entry.to_string();

            let git_env = git_env(&path).clone();
            let repo_id = match git_env.id() {
                Some(repo_id) => repo_id,
                None => continue,
            };
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
            if options.background_update()
                && !options
                    .force_sync
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

            // Get the updater for that repository
            let updater = match GitRepoUpdater::from_path(&path) {
                Some(updater) => updater,
                None => continue,
            };

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
                Box::new(SpinnerProgressHandler::new_with_multi(
                    desc,
                    None,
                    multiprogress.clone(),
                ))
            } else {
                Box::new(PrintProgressHandler::new(desc, None))
            };

            let _multiprogress = multiprogress.clone();
            let sender = sender.clone();
            threads.push(thread::spawn(move || {
                let result = updater.update(Some(progress_handler.as_ref()));

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
                    None => path_entry.to_string().light_cyan(),
                };

                omni_info!(format!(
                    "running {} in {}",
                    "omni up".light_yellow(),
                    location,
                ));

                let mut omni_up_command = StdCommand::new(current_exe.clone());
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

            ensure_newline();
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
        .collect::<HashSet<_>>();

    let errored_paths = updates_per_path
        .iter()
        .filter_map(|(path, updated)| {
            if updated.is_err() {
                Some(PathBuf::from(path))
            } else {
                None
            }
        })
        .chain(failed_early_auth.iter().map(PathBuf::from))
        .collect::<HashSet<_>>();

    // If we need to update in the background, let's do that now
    if options.background_update() && count_left_to_update > 0 {
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

pub enum GitRepoUpdaterRefType {
    Branch,
    Tag,
}

impl GitRepoUpdaterRefType {
    fn from_ref_type(ref_type: &str) -> Self {
        match ref_type {
            "branch" => Self::Branch,
            "tag" => Self::Tag,
            _ => unreachable!("invalid ref type: {}", ref_type),
        }
    }

    fn update(
        &self,
        repo_path: &str,
        ref_match: StringFilter,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<bool, String> {
        match self {
            Self::Branch => update_git_branch(repo_path, ref_match, progress_handler),
            Self::Tag => update_git_tag(repo_path, ref_match, progress_handler),
        }
    }
}

pub struct GitRepoUpdater {
    repo_id: String,
    path: String,
    ref_type: GitRepoUpdaterRefType,
    pattern: StringFilter,
}

impl GitRepoUpdater {
    pub fn from_path<T: AsRef<str>>(path: T) -> Option<Self> {
        let config = global_config();
        let prucfg = config.path_repo_updates;

        let wd = workdir(path);
        let wd_root = wd.root()?;
        if !wd.in_git() {
            return None;
        }

        let (clean_id, typed_id) = match (wd.trust_id(), wd.typed_id()) {
            (Some(clean_id), Some(typed_id)) => (clean_id, typed_id),
            _ => return None,
        };

        for value in &prucfg.per_repo_config {
            if value.workdir_id.matches(&clean_id) || value.workdir_id.matches(&typed_id) {
                if !value.enabled {
                    return None;
                }

                return Some(Self {
                    repo_id: clean_id.to_string(),
                    path: wd_root.to_string(),
                    ref_type: GitRepoUpdaterRefType::from_ref_type(&value.ref_type),
                    pattern: value.ref_match.clone(),
                });
            }
        }

        if !prucfg.enabled {
            return None;
        }

        Some(Self {
            repo_id: clean_id.to_string(),
            path: wd_root.to_string(),
            ref_type: GitRepoUpdaterRefType::from_ref_type(&prucfg.ref_type),
            pattern: prucfg.ref_match.clone(),
        })
    }

    pub fn update(&self, progress_handler: Option<&dyn ProgressHandler>) -> Result<bool, String> {
        let desc = format!("Updating {}:", self.repo_id.italic().light_cyan()).light_blue();
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

        self.ref_type
            .update(&self.path, self.pattern.clone(), *progress_handler)
    }
}

fn update_git_branch(
    repo_path: &str,
    ref_match: StringFilter,
    progress_handler: &dyn ProgressHandler,
) -> Result<bool, String> {
    progress_handler.progress("checking current branch".to_string());

    // Check if the currently checked out branch matches the one we want to update
    let mut local_branch_cmd = StdCommand::new("git");
    local_branch_cmd.arg("branch");
    local_branch_cmd.arg("--show-current");
    local_branch_cmd.current_dir(repo_path);
    local_branch_cmd.stdout(std::process::Stdio::piped());
    local_branch_cmd.stderr(std::process::Stdio::null());

    let output = local_branch_cmd.output().map_err(|err| {
        let msg = format!("git branch failed: {}", err);
        progress_handler.error_with_message(msg.clone());
        msg
    })?;

    if !output.status.success() {
        let msg = "git branch failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    let local_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if local_branch.is_empty() {
        let msg = "not currently checked out on a branch; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    if !ref_match.matches(&local_branch) {
        let msg = format!(
            "current branch {} does not match {}; skipping update",
            local_branch, ref_match,
        );
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    let is_package = PathEntryConfig::from_path(repo_path).is_package();
    if is_package {
        // If we're on a package, we can be aggressive on the update
        // since we are the ones managing the repository

        // Get the remote name we are tracking
        let mut remote_name_cmd = StdCommand::new("git");
        remote_name_cmd.arg("config");
        remote_name_cmd.arg("--get");
        remote_name_cmd.arg(format!("branch.{}.remote", local_branch));
        remote_name_cmd.current_dir(repo_path);
        remote_name_cmd.stdout(std::process::Stdio::piped());
        remote_name_cmd.stderr(std::process::Stdio::null());

        let output = remote_name_cmd.output().map_err(|err| {
            let msg = format!("git config failed: {}", err);
            progress_handler.error_with_message(msg.clone());
            msg
        })?;
        if !output.status.success() {
            let msg = "failed to get remote name".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }

        let remote_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if remote_name.is_empty() {
            let msg = "no remote name configured".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }

        // Get the remote branch we are tracking
        let mut remote_branch_cmd = StdCommand::new("git");
        remote_branch_cmd.arg("rev-parse");
        remote_branch_cmd.arg("--abbrev-ref");
        remote_branch_cmd.arg("@{u}");
        remote_branch_cmd.current_dir(repo_path);
        remote_branch_cmd.stdout(std::process::Stdio::piped());
        remote_branch_cmd.stderr(std::process::Stdio::null());

        let output = local_branch_cmd.output().map_err(|err| {
            let msg = format!("git rev-parse failed: {}", err);
            progress_handler.error_with_message(msg.clone());
            msg
        })?;
        if !output.status.success() {
            let msg = format!(
                "failed to get remote branch for {}: {}",
                local_branch,
                String::from_utf8_lossy(&output.stderr)
            );
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }

        let remote_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if remote_branch.is_empty() {
            let msg = "no remote branch configured".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }

        let remote_branch_full = format!("{}/{}", remote_name, remote_branch);

        // Fetch the updates for the remote branch
        let mut git_fetch_cmd = TokioCommand::new("git");
        git_fetch_cmd.arg("fetch");
        git_fetch_cmd.arg(remote_name);
        git_fetch_cmd.arg(remote_branch);
        git_fetch_cmd.current_dir(repo_path);
        git_fetch_cmd.stdout(std::process::Stdio::piped());
        git_fetch_cmd.stderr(std::process::Stdio::piped());

        let output = get_command_output(&mut git_fetch_cmd, RunConfig::new().with_askpass())
            .map_err(|err| {
                let msg = format!("git fetch failed: {}", err);
                progress_handler.error_with_message(msg.clone());
                msg
            })?;
        if !output.status.success() {
            let msg = format!(
                "failed to fetch updates for {}: {}",
                remote_branch_full,
                String::from_utf8_lossy(&output.stderr)
            );
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }

        // Check if there was any new contents fetched
        let fetched_err = String::from_utf8_lossy(&output.stderr);

        // Check if there is a line containing `-> <remote-branch>` in the error output
        let remote_branch_updated = fetched_err
            .lines()
            .any(|line| line.contains(&format!("-> {}", remote_branch_full)));

        if !remote_branch_updated {
            progress_handler.success_with_message("already up to date".light_black());
            return Ok(false);
        }

        // If there was new contents fetched, we need to reset to the local branch
        // to the remote branch
        let mut git_reset_cmd = StdCommand::new("git");
        git_reset_cmd.arg("reset");
        git_reset_cmd.arg("--hard");
        git_reset_cmd.arg(remote_branch_full);
        git_reset_cmd.current_dir(repo_path);
        git_reset_cmd.stdout(std::process::Stdio::null());
        git_reset_cmd.stderr(std::process::Stdio::piped());

        let output = git_reset_cmd.output().map_err(|err| {
            let msg = format!("git reset failed: {}", err);
            progress_handler.error_with_message(msg.clone());
            msg
        })?;
        if !output.status.success() {
            let msg = format!(
                "failed to reset local branch: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }

        // NOTE: do we want to `git clean -fdx` here?
        //       the main concern is if we have a package generating
        //       local binaries, we would be removing all of that

        // Now we are done
        progress_handler.success_with_message("updated".light_green());
        git_env_flush_cache(repo_path);
        Ok(true)
    } else {
        // If we are not on a package, we need to be more conservative as it could be
        // that someone is working on this repository
        progress_handler.progress("pulling latest changes".to_string());

        let mut git_pull_cmd = TokioCommand::new("git");
        git_pull_cmd.arg("pull");
        git_pull_cmd.arg("--ff-only");
        git_pull_cmd.current_dir(repo_path);
        git_pull_cmd.stdout(std::process::Stdio::piped());
        git_pull_cmd.stderr(std::process::Stdio::piped());

        let output = get_command_output(&mut git_pull_cmd, RunConfig::new().with_askpass())
            .map_err(|err| {
                let msg = format!("git pull failed: {}", err);
                progress_handler.error_with_message(msg.clone());
                msg
            })?;

        if !output.status.success() {
            let msg = format!(
                "git pull failed: {}",
                String::from_utf8_lossy(&output.stderr)
                    .replace('\n', " ")
                    .trim()
            );
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }

        let contents = String::from_utf8_lossy(&output.stdout);
        let lines = contents.lines().collect::<Vec<&str>>();

        if lines.len() == 1 && lines[0].contains("Already up to date.") {
            progress_handler.success_with_message("already up to date".light_black());
            Ok(false)
        } else {
            progress_handler.success_with_message("updated".light_green());
            git_env_flush_cache(repo_path);
            Ok(true)
        }
    }
}

fn update_git_tag(
    repo_path: &str,
    ref_match: StringFilter,
    progress_handler: &dyn ProgressHandler,
) -> Result<bool, String> {
    // Check if we're currently checked out on a branch
    progress_handler.progress("checking current branch".to_string());

    let mut local_branch_cmd = StdCommand::new("git");
    local_branch_cmd.arg("branch");
    local_branch_cmd.arg("--show-current");
    local_branch_cmd.current_dir(repo_path);
    local_branch_cmd.stdout(std::process::Stdio::piped());
    local_branch_cmd.stderr(std::process::Stdio::null());

    let output = local_branch_cmd.output().map_err(|err| {
        let msg = format!("git branch failed: {}", err);
        progress_handler.error_with_message(msg.clone());
        msg
    })?;

    if !output.status.success() {
        let msg = "git branch failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    let local_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !local_branch.is_empty() {
        let msg = "currently checked out on a branch; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Check which tag we are currently checked out on, if any
    progress_handler.progress("checking current tag".to_string());
    let mut git_tag_cmd = StdCommand::new("git");
    git_tag_cmd.arg("tag");
    git_tag_cmd.arg("--points-at");
    git_tag_cmd.arg("HEAD");
    git_tag_cmd.arg("--sort=-creatordate");
    git_tag_cmd.current_dir(repo_path);
    git_tag_cmd.stdout(std::process::Stdio::piped());
    git_tag_cmd.stderr(std::process::Stdio::null());

    let output = git_tag_cmd.output().map_err(|err| {
        let msg = format!("git tag failed: {}", err);
        progress_handler.error_with_message(msg.clone());
        msg
    })?;

    if !output.status.success() {
        let msg = "git tag failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    let git_tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if git_tag.is_empty() {
        let msg = "not currently checked out on a tag; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Consider the latest tag built on this commit to be the current tag
    let current_tag = git_tag.lines().collect::<Vec<&str>>()[0].to_string();

    // Fetch all the tags for the repository
    progress_handler.progress("fetching last tags".to_string());
    let mut git_fetch_tags_cmd = TokioCommand::new("git");
    git_fetch_tags_cmd.arg("fetch");
    git_fetch_tags_cmd.arg("--tags");
    git_fetch_tags_cmd.current_dir(repo_path);
    git_fetch_tags_cmd.stdout(std::process::Stdio::piped());
    git_fetch_tags_cmd.stderr(std::process::Stdio::piped());

    let output = get_command_output(&mut git_fetch_tags_cmd, RunConfig::new().with_askpass())
        .map_err(|err| {
            let msg = format!("git fetch failed: {}", err);
            progress_handler.error_with_message(msg.clone());
            msg
        })?;

    if !output.status.success() {
        let msg = "git fetch failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Check if there was any new tags fetched
    let fetched_out = String::from_utf8_lossy(&output.stdout);
    let fetched_err = String::from_utf8_lossy(&output.stderr);
    if fetched_out.trim().is_empty() && fetched_err.trim().is_empty() {
        // If no new tags, nothing more to do!
        progress_handler.success_with_message("no new tags, nothing to do".light_black());
        return Ok(false);
    }

    // If any new tags, we need to check what is the most recent tag
    // that matches the passed tag parameter (if any)
    progress_handler.progress("checking latest tag".to_string());
    let mut git_tag_cmd = StdCommand::new("git");
    git_tag_cmd.arg("tag");
    git_tag_cmd.arg("--sort=-creatordate");
    git_tag_cmd.current_dir(repo_path);
    git_tag_cmd.stdout(std::process::Stdio::piped());
    git_tag_cmd.stderr(std::process::Stdio::null());

    let output = git_tag_cmd.output().map_err(|err| {
        let msg = format!("git tag failed: {}", err);
        progress_handler.error_with_message(msg.clone());
        msg
    })?;

    if !output.status.success() {
        let msg = "git tag failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    let git_tags = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if git_tags.is_empty() {
        let msg = "no tags found; skipping update".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    // Find the most recent git tag in git_tags that matches
    // the passed tag parameter (if any)
    let target_tag = match git_tags.lines().find(|git_tag| ref_match.matches(git_tag)) {
        Some(target_tag) => target_tag.trim().to_string(),
        None => {
            let msg = "no matching tags found; skipping update".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(msg);
        }
    };

    // If the current tag is the same as the target tag, nothing more to do!
    if current_tag == target_tag {
        progress_handler.success_with_message("already on latest matching tag".light_black());
        return Ok(false);
    }

    // Check out the target tag
    progress_handler.progress(format!("checking out {}", target_tag.light_green()));
    let mut git_checkout_cmd = StdCommand::new("git");
    git_checkout_cmd.arg("checkout");
    git_checkout_cmd.arg("--no-guess");
    git_checkout_cmd.arg(&target_tag);
    git_checkout_cmd.current_dir(repo_path);
    git_checkout_cmd.stdout(std::process::Stdio::null());
    git_checkout_cmd.stderr(std::process::Stdio::null());

    let output = git_checkout_cmd.output().map_err(|err| {
        let msg = format!("git checkout failed: {}", err);
        progress_handler.error_with_message(msg.clone());
        msg
    })?;

    if !output.status.success() {
        let msg = "git checkout failed".to_string();
        progress_handler.error_with_message(msg.clone());
        return Err(msg);
    }

    progress_handler.success_with_message("updated".light_green());
    git_env_flush_cache(repo_path);

    Ok(true)
}
