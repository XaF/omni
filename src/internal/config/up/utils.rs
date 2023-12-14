use std::io::Write;
use std::path::Path;

use blake3::Hasher;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressDrawTarget;
use indicatif::ProgressStyle;
use normalize_path::NormalizePath;
use regex::Regex;
use std::os::unix::fs::PermissionsExt;
use tempfile::NamedTempFile;
use time::format_description::well_known::Rfc3339;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::process::Command as TokioCommand;
use tokio::runtime::Runtime;
use tokio::time::Duration;

use crate::internal::config::up::UpError;
use crate::internal::user_interface::StringColor;

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub timeout: Option<Duration>,
    pub strip_ctrl_chars: bool,
}

impl RunConfig {
    pub fn new() -> Self {
        RunConfig {
            timeout: None,
            strip_ctrl_chars: false,
        }
    }

    pub fn default() -> Self {
        RunConfig {
            timeout: None,
            strip_ctrl_chars: true,
        }
    }

    pub fn with_timeout(&mut self, timeout: u64) -> Self {
        self.timeout = Some(Duration::from_secs(timeout));
        self.clone()
    }

    // pub fn with_ctrl_chars(&mut self) -> Self {
    // self.strip_ctrl_chars = false;
    // self.clone()
    // }

    pub fn without_ctrl_chars(&mut self) -> Self {
        self.strip_ctrl_chars = true;
        self.clone()
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }
}

pub fn run_progress(
    process_command: &mut TokioCommand,
    progress_handler: Option<&dyn ProgressHandler>,
    run_config: RunConfig,
) -> Result<(), UpError> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async_run_progress_readblocks(
        process_command,
        |stdout, stderr| {
            if let Some(progress_handler) = &progress_handler {
                if let Some(stdout) = stdout {
                    progress_handler.progress(stdout);
                } else if let Some(stderr) = stderr {
                    progress_handler.progress(stderr);
                }
            }
        },
        run_config,
    ))
}

pub fn run_command_with_handler<F>(
    command: &mut TokioCommand,
    handler_fn: F,
    run_config: RunConfig,
) -> Result<(), UpError>
where
    F: FnMut(Option<String>, Option<String>),
{
    let rt = Runtime::new().unwrap();
    rt.block_on(async_run_progress_readlines(
        command, handler_fn, run_config,
    ))
}

// pub fn run_command_with_handler_blocks<F>(
// command: &mut TokioCommand,
// handler_fn: F,
// run_config: RunConfig,
// ) -> Result<(), UpError>
// where
// F: Fn(Option<String>, Option<String>) -> (),
// {
// let rt = Runtime::new().unwrap();
// rt.block_on(async_run_progress_readblocks(
// command, handler_fn, run_config,
// ))
// }

async fn async_run_progress_readblocks<F>(
    process_command: &mut TokioCommand,
    handler_fn: F,
    run_config: RunConfig,
) -> Result<(), UpError>
where
    F: Fn(Option<String>, Option<String>),
{
    if let Ok(mut command) = process_command.spawn() {
        // Create a temporary file to store the output
        let log_file_prefix = format!(
            "omni-exec.{}.",
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
                return Err(UpError::Exec(err.to_string()));
            }
        };

        if let (Some(mut stdout), Some(mut stderr)) = (command.stdout.take(), command.stderr.take())
        {
            let mut stdout_buffer = [0; 1024];
            let mut stderr_buffer = [0; 1024];
            let mut last_read = std::time::Instant::now();

            loop {
                tokio::select! {
                    stdout_result = stdout.read(&mut stdout_buffer) => {
                        match stdout_result {
                            Ok(0) => break,  // End of stdout stream
                            Ok(n) => {
                                last_read = std::time::Instant::now();
                                let stdout_output = &stdout_buffer[..n];
                                log_file.write_all(stdout_output).unwrap();
                                if let Ok(stdout_str) = std::str::from_utf8(stdout_output) {
                                    let stdout_str = stdout_str.trim_end();
                                    let stdout_str = if let Some(index) = stdout_str.rfind('\n') {
                                        &stdout_str[index+1..]
                                    } else {
                                        stdout_str
                                    };

                                    handler_fn(Some(if run_config.strip_ctrl_chars {
                                        filter_control_characters(stdout_str)
                                    } else { stdout_str.to_string() }), None);
                                }
                            }
                            Err(_err) => break,
                        }
                    }
                    stderr_result = stderr.read(&mut stderr_buffer) => {
                        match stderr_result {
                            Ok(0) => break,  // End of stderr stream
                            Ok(n) => {
                                last_read = std::time::Instant::now();
                                let stderr_output = &stderr_buffer[..n];
                                log_file.write_all(stderr_output).unwrap();
                                if let Ok(stderr_str) = std::str::from_utf8(stderr_output) {
                                    let stderr_str = stderr_str.trim_end();
                                    let stderr_str = if let Some(index) = stderr_str.rfind('\n') {
                                        &stderr_str[index+1..]
                                    } else {
                                        stderr_str
                                    };
                                    handler_fn(None, Some(if run_config.strip_ctrl_chars {
                                        filter_control_characters(stderr_str)
                                    } else { stderr_str.to_string() }));
                                }
                            }
                            Err(_err) => break,
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if let Some(timeout) = run_config.timeout() {
                            if last_read.elapsed() > timeout {
                                if (command.kill().await).is_err() {
                                    // Nothing special to do, we're returning an error anyway
                                }
                                return Err(UpError::Timeout(format!("{:?}", process_command.as_std())));
                            }
                        }
                    }
                }
            }
        }

        match command.wait().await {
            Err(err) => Err(UpError::Exec(err.to_string())),
            Ok(exit_status) if !exit_status.success() => {
                let exit_code = exit_status.code().unwrap_or(-42);
                match log_file.keep() {
                    Ok((_file, path)) => Err(UpError::Exec(format!(
                        "process exited with status {}; log is available at {}",
                        exit_code,
                        path.to_string_lossy().underline(),
                    ))),
                    Err(err) => Err(UpError::Exec(format!(
                        "process exited with status {}; failed to keep log file: {}",
                        exit_code, err,
                    ))),
                }
            }
            Ok(_exit_status) => Ok(()),
        }
    } else {
        Err(UpError::Exec(format!("{:?}", process_command.as_std())))
    }
}

async fn async_run_progress_readlines<F>(
    process_command: &mut TokioCommand,
    mut handler_fn: F,
    run_config: RunConfig,
) -> Result<(), UpError>
where
    F: FnMut(Option<String>, Option<String>),
{
    if let Ok(mut command) = process_command.spawn() {
        if let (Some(stdout), Some(stderr)) = (command.stdout.take(), command.stderr.take()) {
            let mut last_read = std::time::Instant::now();
            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut stderr_reader = BufReader::new(stderr).lines();

            loop {
                tokio::select! {
                    stdout_line = stdout_reader.next_line() => {
                        match stdout_line {
                            Ok(Some(line)) => {
                                last_read = std::time::Instant::now();
                                handler_fn(Some(if run_config.strip_ctrl_chars {
                                    filter_control_characters(&line)
                                } else { line }), None);

                            }
                            Ok(None) => break,  // End of stdout stream
                            Err(err) => return Err(UpError::Exec(err.to_string())),
                        }
                    }
                    stderr_line = stderr_reader.next_line() => {
                        match stderr_line {
                            Ok(Some(line)) => {
                                last_read = std::time::Instant::now();
                                handler_fn(None, Some(if run_config.strip_ctrl_chars {
                                    filter_control_characters(&line)
                                } else { line }));
                            }
                            Ok(None) => break,  // End of stderr stream
                            Err(err) => return Err(UpError::Exec(err.to_string())),
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if let Some(timeout) = run_config.timeout() {
                            if last_read.elapsed() > timeout {
                                if (command.kill().await).is_err() {
                                    // Nothing special to do, we're returning an error anyway
                                }
                                return Err(UpError::Timeout(format!("{:?}", process_command.as_std())));
                            }
                        }
                    }
                }
            }
        }

        let exit_status = command.wait().await;
        if exit_status.is_err() || !exit_status.unwrap().success() {
            return Err(UpError::Exec(format!("{:?}", process_command.as_std())));
        }
    } else {
        return Err(UpError::Exec(format!("{:?}", process_command.as_std())));
    }

    Ok(())
}

fn filter_control_characters(input: &str) -> String {
    let control_chars_regex = Regex::new(r"(\x1B\[[0-9;]*[ABCDK]|\x0D)").unwrap();
    control_chars_regex.replace_all(input, "").to_string()
    // let regex = Regex::new(r"[[:cntrl:]]").unwrap();
    // let cleaned_text = regex.replace_all(input, |caps: &regex::Captures<'_>| {
    // let control_char = caps.get(0).unwrap().as_str();
    // format!("\\x{:02X}", control_char.chars().next().unwrap() as u8)
    // });
    // cleaned_text.to_string()
}

pub trait ProgressHandler {
    fn println(&self, message: String);
    fn progress(&self, message: String);
    fn success(&self);
    fn success_with_message(&self, message: String);
    fn error(&self);
    fn error_with_message(&self, message: String);
    fn hide(&self);
    fn show(&self);
}

#[derive(Debug, Clone)]
pub struct SpinnerProgressHandler {
    spinner: ProgressBar,
    template: String,
    newline_on_error: bool,
}

impl SpinnerProgressHandler {
    pub fn new(desc: String, progress: Option<(usize, usize)>) -> Self {
        Self::new_with_params(desc, progress, None)
    }

    pub fn new_with_multi(
        desc: String,
        progress: Option<(usize, usize)>,
        multiprogress: MultiProgress,
    ) -> Self {
        Self::new_with_params(desc, progress, Some(multiprogress))
    }

    pub fn new_with_params(
        desc: String,
        progress: Option<(usize, usize)>,
        multiprogress: Option<MultiProgress>,
    ) -> Self {
        let template = format!("{{prefix}}{} {} {{msg}}", "{spinner}".yellow(), desc,);

        let spinner = if let Some(multiprogress) = multiprogress {
            multiprogress.add(ProgressBar::new_spinner())
        } else {
            ProgressBar::new_spinner()
        };
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template(template.as_str())
                .unwrap(),
        );
        spinner.set_message("-");
        spinner.enable_steady_tick(Duration::from_millis(50));

        if let Some((current, total)) = progress {
            let padding = format!("{}", total).len();
            spinner.set_prefix(
                format!(
                    "[{:padding$}/{:padding$}] ",
                    current,
                    total,
                    padding = padding
                )
                .bold()
                .light_black(),
            );
        }

        SpinnerProgressHandler {
            spinner,
            template,
            newline_on_error: true,
        }
    }

    pub fn no_newline_on_error(&mut self) {
        self.newline_on_error = false;
    }

    // pub fn get_spinner(&self) -> &ProgressBar {
    // &self.spinner
    // }

    fn replace_spinner(&self, replace_by: String) {
        let template = self.template.replace("{spinner}", replace_by.as_str());
        self.spinner.set_style(
            ProgressStyle::default_spinner()
                .template(template.as_str())
                .unwrap(),
        );
    }
}

impl ProgressHandler for SpinnerProgressHandler {
    fn println(&self, message: String) {
        self.spinner.println(message);
    }

    fn progress(&self, message: String) {
        self.spinner.set_message(message);
    }

    fn success(&self) {
        // self.replace_spinner("✔".green());
        // self.spinner.finish();
        self.success_with_message("done".to_string());
    }

    fn success_with_message(&self, message: String) {
        self.replace_spinner("✔".green());
        self.spinner.finish_with_message(message);
    }

    fn error(&self) {
        self.replace_spinner("✖".red());
        self.spinner
            .finish_with_message(self.spinner.message().red());
        if self.newline_on_error {
            println!();
        }
    }

    fn error_with_message(&self, message: String) {
        self.replace_spinner("✖".red());
        self.spinner.finish_with_message(message.red());
        if self.newline_on_error {
            println!();
        }
    }

    fn hide(&self) {
        self.spinner.set_draw_target(ProgressDrawTarget::hidden());
    }

    fn show(&self) {
        self.spinner.set_draw_target(ProgressDrawTarget::stderr());
    }
}

#[derive(Debug, Clone)]
pub struct PrintProgressHandler {
    template: String,
}

impl PrintProgressHandler {
    pub fn new(desc: String, progress: Option<(usize, usize)>) -> Self {
        let prefix = if let Some((current, total)) = progress {
            let padding = format!("{}", total).len();
            format!(
                "[{:padding$}/{:padding$}] ",
                current,
                total,
                padding = padding
            )
            .bold()
            .light_black()
        } else {
            "".to_string()
        };

        let template = format!("{}{{}} {} {{}}", prefix, desc);

        PrintProgressHandler { template }
    }
}

impl ProgressHandler for PrintProgressHandler {
    fn println(&self, message: String) {
        eprintln!("{}", message);
    }

    fn progress(&self, message: String) {
        eprintln!(
            "{}",
            self.template
                .replacen("{}", "-".light_black().as_str(), 1)
                .replacen("{}", message.as_str(), 1)
        );
    }

    fn success(&self) {
        self.success_with_message("done".to_string());
    }

    fn success_with_message(&self, message: String) {
        eprintln!(
            "{}",
            self.template
                .replacen("{}", "✔".green().as_str(), 1)
                .replacen("{}", message.as_str(), 1)
        );
    }

    fn error(&self) {
        self.error_with_message("error".to_string());
    }

    fn error_with_message(&self, message: String) {
        eprintln!(
            "{}",
            self.template
                .replacen("{}", "✖".red().as_str(), 1)
                .replacen("{}", message.red().as_str(), 1)
        );
    }

    fn hide(&self) {
        // do nothing
    }

    fn show(&self) {
        // do nothing
    }
}

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
        let hash_b62 = base_62::encode(hash_bytes.as_bytes())[..20].to_string();
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
