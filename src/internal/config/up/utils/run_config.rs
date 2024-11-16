use tokio::process::Command as TokioCommand;
use tokio::time::Duration;

use crate::internal::config::up::utils::AskPassListener;
use crate::internal::config::up::utils::ListenerManager;

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub timeout: Option<Duration>,
    pub strip_ctrl_chars: bool,
    pub askpass: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        RunConfig {
            timeout: None,
            strip_ctrl_chars: true,
            askpass: false,
        }
    }
}

impl RunConfig {
    pub fn new() -> Self {
        Self::default().without_ctrl_chars()
    }

    pub fn with_timeout(&mut self, timeout: u64) -> Self {
        self.timeout = Some(Duration::from_secs(timeout));
        self.clone()
    }

    pub fn without_ctrl_chars(&mut self) -> Self {
        self.strip_ctrl_chars = true;
        self.clone()
    }

    pub fn with_askpass(&mut self) -> Self {
        self.askpass = true;
        self.clone()
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    pub async fn listener_manager_for_command(
        &self,
        process_command: &mut TokioCommand,
    ) -> Result<ListenerManager, String> {
        let mut listener_manager = ListenerManager::new();
        let command_string = &command_str(process_command);

        if self.askpass {
            match AskPassListener::new(command_string).await {
                Ok(Some(listener)) => {
                    listener_manager.add_listener(Box::new(listener));
                }
                Ok(None) => {}
                Err(err) => {
                    return Err(err.to_string());
                }
            }
        }

        listener_manager.set_process_env(process_command).await?;

        Ok(listener_manager)
    }
}

/// Convert a `TokioCommand` to a string
fn command_str(command: &TokioCommand) -> String {
    let command = command.as_std();
    let mut command_arr = vec![];
    command_arr.push(command.get_program().to_string_lossy().to_string());
    for arg in command.get_args() {
        let mut arg = arg.to_string_lossy().to_string();
        if arg.contains(' ') {
            arg = format!("\"{}\"", arg.replace('"', "\\\""));
        }
        command_arr.push(arg);
    }
    command_arr.join(" ")
}
