use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::StringColor;
use crate::internal::ENV;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigCustom {
    pub meet: String,
    pub met: Option<String>,
    pub unmeet: Option<String>,
    pub name: Option<String>,
}

impl UpConfigCustom {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut meet = None;
        let mut met = None;
        let mut unmeet = None;
        let mut name = None;

        if let Some(config_value) = config_value {
            if let Some(value) = config_value.get("meet") {
                meet = Some(value.as_str().unwrap().to_string());
            }
            if let Some(value) = config_value.get("met?") {
                met = Some(value.as_str().unwrap().to_string());
            }
            if let Some(value) = config_value.get("unmeet") {
                unmeet = Some(value.as_str().unwrap().to_string());
            }
            if let Some(value) = config_value.get("name") {
                name = Some(value.as_str().unwrap().to_string());
            }
        }

        if meet == None {
            meet = Some("".to_string());
        }

        UpConfigCustom {
            meet: meet.unwrap(),
            met: met,
            unmeet: unmeet,
            name: name,
        }
    }

    pub fn up(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        let name = if let Some(name) = &self.name {
            name.to_string()
        } else {
            self.meet
                .split_whitespace()
                .next()
                .unwrap_or("custom")
                .to_string()
        };
        let desc = format!("{}:", name).light_blue();

        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, progress))
        };
        let progress_handler: Option<Box<&dyn ProgressHandler>> =
            Some(Box::new(progress_handler.as_ref()));

        if self.met().unwrap_or(false) {
            progress_handler.clone().map(|progress_handler| {
                progress_handler
                    .success_with_message("skipping (already met)".to_string().light_black())
            });
            return Ok(());
        }

        if let Err(err) = self.meet(progress_handler.clone()) {
            progress_handler.clone().map(|progress_handler| {
                progress_handler.error_with_message(format!("{}", err).light_red())
            });
            return Err(err);
        }

        progress_handler
            .clone()
            .map(|progress_handler| progress_handler.success());

        Ok(())
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        let name = if let Some(name) = &self.name {
            name.to_string()
        } else {
            self.unmeet
                .clone()
                .unwrap_or("custom".to_string())
                .split_whitespace()
                .next()
                .unwrap_or("custom")
                .to_string()
        };

        let mut spinner_progress_handler = None;
        let mut progress_handler: Option<Box<&dyn ProgressHandler>> = None;
        if ENV.interactive_shell {
            spinner_progress_handler = Some(SpinnerProgressHandler::new(
                format!("{}:", name).light_blue(),
                progress,
            ));
            progress_handler = Some(Box::new(spinner_progress_handler.as_ref().unwrap()));
        }

        if let Some(_unmeet) = &self.unmeet {
            if !self.met().unwrap_or(true) {
                progress_handler.clone().map(|progress_handler| {
                    progress_handler
                        .success_with_message("skipping (not met)".to_string().light_black())
                });
                return Ok(());
            }

            progress_handler.clone().map(|progress_handler| {
                progress_handler.progress("reverting".to_string().light_black())
            });

            if let Err(err) = self.unmeet(progress_handler.clone()) {
                progress_handler.clone().map(|progress_handler| {
                    progress_handler.error_with_message(format!("{}", err).light_red())
                });
                return Err(err);
            }
        }

        progress_handler
            .clone()
            .map(|progress_handler| progress_handler.success());

        Ok(())
    }

    fn met(&self) -> Option<bool> {
        if let Some(met) = &self.met {
            let mut command = std::process::Command::new("bash");
            command.arg("-c");
            command.arg(met);
            command.stdout(std::process::Stdio::null());
            command.stderr(std::process::Stdio::null());

            let output = command.output().unwrap();
            Some(output.status.success())
        } else {
            None
        }
    }

    fn meet(&self, progress_handler: Option<Box<&dyn ProgressHandler>>) -> Result<(), UpError> {
        if self.meet != "" {
            // eprintln!("{}", format!("$ {}", self.meet).light_black());
            progress_handler.clone().map(|progress_handler| {
                progress_handler.progress("running (meet) command".to_string())
            });

            let mut command = TokioCommand::new("bash");
            command.arg("-c");
            command.arg(&self.meet);
            command.stdout(std::process::Stdio::piped());
            command.stderr(std::process::Stdio::piped());

            run_progress(&mut command, progress_handler.clone(), RunConfig::default())?;
        }

        Ok(())
    }

    fn unmeet(&self, progress_handler: Option<Box<&dyn ProgressHandler>>) -> Result<(), UpError> {
        if let Some(unmeet) = &self.unmeet {
            // eprintln!("{}", format!("$ {}", unmeet).light_black());
            progress_handler.clone().map(|progress_handler| {
                progress_handler.progress("running (unmeet) command".to_string())
            });

            let mut command = TokioCommand::new("bash");
            command.arg("-c");
            command.arg(unmeet);
            command.stdout(std::process::Stdio::piped());
            command.stderr(std::process::Stdio::piped());

            run_progress(&mut command, progress_handler.clone(), RunConfig::default())?;
        }

        Ok(())
    }
}
