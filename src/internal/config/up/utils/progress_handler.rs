use std::io::Write;

use regex::Regex;
use tempfile::NamedTempFile;
use time::format_description::well_known::Rfc3339;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::process::Command as TokioCommand;
use tokio::runtime::Runtime;
use tokio::time::Duration;

use crate::internal::config::up::utils::AskPassListener;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::UpError;
use crate::internal::user_interface::StringColor;
use crate::omni_warning;

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

pub fn run_progress(
    process_command: &mut TokioCommand,
    progress_handler: Option<&dyn ProgressHandler>,
    run_config: RunConfig,
) -> Result<(), UpError> {
    let rt = Runtime::new().map_err(|err| UpError::Exec(err.to_string()))?;
    rt.block_on(async_run_progress_readblocks(
        process_command,
        |stdout, stderr, hide| {
            if let Some(progress_handler) = &progress_handler {
                match hide {
                    Some(true) => progress_handler.hide(),
                    Some(false) => progress_handler.show(),
                    None => {}
                }
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

pub fn get_command_output(
    process_command: &mut TokioCommand,
    run_config: RunConfig,
) -> std::io::Result<std::process::Output> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async_get_output(process_command, run_config))
}

async fn async_get_output(
    process_command: &mut TokioCommand,
    run_config: RunConfig,
) -> std::io::Result<std::process::Output> {
    let mut listener = match AskPassListener::new(&run_config).await {
        Ok(listener) => listener,
        Err(err) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                err.to_string(),
            ))
        }
    };
    listener.set_process_env(process_command).await;

    // let timeout_sleep = async || -> Option<()> {
    // if let Some(timeout) = run_config.timeout() {
    // tokio::time::sleep(timeout).await;
    // Some(())
    // } else {
    // None
    // }
    // };

    process_command.kill_on_drop(true);
    let mut command = match process_command.spawn() {
        Ok(command) => command,
        Err(err) => {
            let _ = listener.close().await;
            return Err(err);
        }
    };

    let mut result = None;
    let mut stdout_vec = Vec::new();
    let mut stderr_vec = Vec::new();

    let (mut stdout_reader, mut stderr_reader) =
        match (command.stdout.take(), command.stderr.take()) {
            (Some(stdout), Some(stderr)) => (
                BufReader::new(stdout).lines(),
                BufReader::new(stderr).lines(),
            ),
            _ => {
                let _ = listener.close().await;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "stdout or stderr missing",
                ));
            }
        };

    loop {
        tokio::select! {
            stdout_line = stdout_reader.next_line() => {
                match stdout_line {
                    Ok(Some(line)) => {
                        stdout_vec.extend_from_slice(line.as_bytes());
                    }
                    Ok(None) => break,  // End of stdout stream
                    Err(err) => {
                        result = Some(Err(err));
                        break;
                    }
                }
            }
            stderr_line = stderr_reader.next_line() => {
                match stderr_line {
                    Ok(Some(line)) => {
                        stderr_vec.extend_from_slice(line.as_bytes());
                    }
                    Ok(None) => break,  // End of stderr stream
                    Err(err) => {
                        result = Some(Err(err));
                        break;
                    }
                }
            }
            Some(connection) = listener.accept() => {
                // We received a connection on the askpass socket,
                // which means that the user will need to provide
                // a password to the process.
                match connection {
                    Ok((mut stream, _addr)) => {
                        if let Err(err) = AskPassListener::handle_request(&mut stream).await {
                            omni_warning!("{}", err);
                        }
                    }
                    Err(err) => {
                        omni_warning!("{}", err);
                    }
                }
            }
            Some(_) = async_timeout(&run_config) => {
                result = Some(Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout")));
                break;
            }
        }
    }

    // Close the listener
    if let Err(err) = listener.close().await {
        omni_warning!("{}", err);
    }

    if let Some(result) = result {
        return result;
    }

    match command.wait_with_output().await {
        Ok(output) => {
            let mut output = output;
            output.stdout = stdout_vec;
            output.stderr = stderr_vec;
            Ok(output)
        }
        Err(err) => Err(err),
    }
}

async fn async_timeout(run_config: &RunConfig) -> Option<()> {
    if let Some(timeout) = run_config.timeout() {
        tokio::time::sleep(timeout).await;
        Some(())
    } else {
        None
    }
}

async fn async_run_progress_readblocks<F>(
    process_command: &mut TokioCommand,
    handler_fn: F,
    run_config: RunConfig,
) -> Result<(), UpError>
where
    F: Fn(Option<String>, Option<String>, Option<bool>),
{
    let mut listener = AskPassListener::new(&run_config).await?;
    listener.set_process_env(process_command).await;

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
                                    for line in stdout_str.lines() {
                                        if line.is_empty() {
                                            continue;
                                        }
                                        handler_fn(Some(if run_config.strip_ctrl_chars {
                                            filter_control_characters(line)
                                        } else { line.to_string() }), None, None);
                                    }
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
                                    for line in stderr_str.lines() {
                                        if line.is_empty() {
                                            continue;
                                        }
                                        handler_fn(None, Some(if run_config.strip_ctrl_chars {
                                            filter_control_characters(line)
                                        } else { line.to_string() }), None);
                                    }
                                }
                            }
                            Err(_err) => break,
                        }
                    }
                    Some(connection) = listener.accept() => {
                        // We received a connection on the askpass socket,
                        // which means that the user will need to provide
                        // a password to the process.
                        match connection {
                            Ok((mut stream, _addr)) => {
                                handler_fn(None, None, Some(true));
                                if let Err(err) = AskPassListener::handle_request(&mut stream).await {
                                    handler_fn(None, Some(err.to_string()), None);
                                }
                                handler_fn(None, None, Some(false));
                            }
                            Err(err) => {
                                handler_fn(None, Some(err.to_string()), None);
                            }
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

        // Close the listener
        if let Err(err) = listener.close().await {
            handler_fn(None, Some(err.to_string()), None);
        }

        match command.wait().await {
            Err(err) => Err(UpError::Exec(err.to_string())),
            Ok(exit_status) if !exit_status.success() => {
                let exit_code = exit_status.code().unwrap_or(-42);
                // TODO: the log file should be prefixed by the tmpdir_cleanup_prefix
                // by default and renamed when deciding to keep it
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
}
