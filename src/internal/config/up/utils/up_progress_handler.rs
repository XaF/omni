use std::collections::BTreeMap;
use std::io::BufRead;
use std::io::Write;
use std::process::exit;

use fs4::FileExt;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::shell_is_interactive;
use crate::internal::errors::SyncUpdateError;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;
use crate::omni_info;
use crate::omni_warning;

pub struct UpProgressHandler<'a> {
    handler: OnceCell<Box<dyn ProgressHandler>>,
    handler_id: Option<String>,
    step: Option<(usize, usize)>,
    prefix: String,
    parent: Option<&'a UpProgressHandler<'a>>,
    allow_ending: bool,
    sync_file: Option<&'a std::fs::File>,
    desc: OnceCell<String>,
}

impl<'a> UpProgressHandler<'a> {
    pub fn new(progress: Option<(usize, usize)>) -> Self {
        // Generate a random handler ID
        let handler_id = uuid::Uuid::new_v4().to_string();

        UpProgressHandler {
            handler: OnceCell::new(),
            handler_id: Some(handler_id),
            step: progress,
            prefix: "".to_string(),
            parent: None,
            allow_ending: true,
            sync_file: None,
            desc: OnceCell::new(),
        }
    }

    pub fn desc(&self) -> &str {
        if let Some(parent) = self.parent {
            return parent.desc();
        }

        self.desc
            .get_or_init(|| {
                let desc = "".to_string();
                desc
            })
            .as_str()
    }

    pub fn init(&self, desc: String) -> bool {
        if self.handler.get().is_some() || self.parent.is_some() {
            return false;
        }

        if self.desc.set(desc.clone()).is_err() {
            panic!("failed to set progress description");
        }

        #[cfg(not(test))]
        let boxed_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
            Box::new(SpinnerProgressHandler::new(desc, self.step))
        } else {
            Box::new(PrintProgressHandler::new(desc, self.step))
        };

        #[cfg(test)]
        let boxed_handler: Box<dyn ProgressHandler> =
            Box::new(PrintProgressHandler::new(desc, self.step));

        if self.handler.set(boxed_handler).is_err() {
            panic!("failed to set progress handler");
        }
        true
    }

    fn handler(&self) -> &dyn ProgressHandler {
        if let Some(parent) = self.parent {
            return parent.handler();
        }

        self.handler
            .get_or_init(|| {
                let desc = "".to_string();
                let boxed_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
                    Box::new(SpinnerProgressHandler::new(desc, self.step))
                } else {
                    Box::new(PrintProgressHandler::new(desc, self.step))
                };
                boxed_handler
            })
            .as_ref()
    }

    fn handler_id(&self) -> String {
        if let Some(handler_id) = &self.handler_id {
            return handler_id.clone();
        }

        if let Some(parent) = self.parent {
            return parent.handler_id();
        }

        "".to_string()
    }

    pub fn subhandler(&'a self, prefix: &dyn ToString) -> UpProgressHandler<'a> {
        UpProgressHandler {
            handler: OnceCell::new(),
            handler_id: None,
            step: None,
            prefix: prefix.to_string(),
            parent: Some(self),
            allow_ending: false,
            sync_file: None,
            desc: OnceCell::new(),
        }
    }

    pub fn step(&self) -> Option<(usize, usize)> {
        if let Some(parent) = self.parent {
            parent.step()
        } else {
            self.step
        }
    }

    pub fn set_sync_file(&mut self, sync_file: &'a std::fs::File) {
        self.sync_file = Some(sync_file);
    }

    fn update_sync_file(&self, action: SyncUpdateProgressAction) {
        if let Some(mut sync_file) = self.sync_file {
            // Overwrite the handler id and description with the current ones
            let update = SyncUpdateOperation::Progress(SyncUpdateProgress {
                handler_id: self.handler_id(),
                desc: self.desc().to_string(),
                step: self.step(),
                action,
            });

            if let Err(err) = update.dump_to_file(&mut sync_file) {
                panic!("failed to write progress update to file: {}", err);
            }
        } else if let Some(parent) = self.parent {
            parent.update_sync_file(action);
        }
    }

    pub fn perform_sync_action(&self, action: &SyncUpdateProgressAction) {
        match action {
            SyncUpdateProgressAction::Progress(message) => self.handler().progress(message.clone()),
            SyncUpdateProgressAction::Success(message) => {
                if let Some(message) = message {
                    self.handler().success_with_message(message.clone());
                } else {
                    self.handler().success();
                }
            }
            SyncUpdateProgressAction::Error(message) => {
                if let Some(message) = message {
                    self.handler().error_with_message(message.clone());
                } else {
                    self.handler().error();
                }
            }
            SyncUpdateProgressAction::Hide => self.handler().hide(),
            SyncUpdateProgressAction::Show => self.handler().show(),
            SyncUpdateProgressAction::Println(message) => self.handler().println(message.clone()),
        }
    }

    fn format_message(&self, message: String) -> String {
        let message = format!("{}{}", self.prefix, message);
        match self.parent {
            Some(parent) => parent.format_message(message),
            None => message,
        }
    }
}

impl ProgressHandler for UpProgressHandler<'_> {
    fn progress(&self, message: String) {
        let message = self.format_message(message);
        self.update_sync_file(SyncUpdateProgressAction::Progress(message.clone()));
        self.handler().progress(message);
    }

    fn success(&self) {
        self.update_sync_file(SyncUpdateProgressAction::Success(None));
        self.handler().success();
    }

    fn success_with_message(&self, message: String) {
        let message = self.format_message(message);
        if self.allow_ending {
            self.update_sync_file(SyncUpdateProgressAction::Success(Some(message.clone())));
            self.handler().success_with_message(message);
        } else {
            self.update_sync_file(SyncUpdateProgressAction::Progress(message.clone()));
            self.handler().progress(message);
        }
    }

    fn error(&self) {
        if self.allow_ending {
            self.update_sync_file(SyncUpdateProgressAction::Error(None));
            self.handler().error();
        }
    }

    fn error_with_message(&self, message: String) {
        let message = self.format_message(message);
        if self.allow_ending {
            self.update_sync_file(SyncUpdateProgressAction::Error(Some(message.clone())));
            self.handler().error_with_message(message);
        } else {
            self.update_sync_file(SyncUpdateProgressAction::Progress(message.clone()));
            self.handler().progress(message);
        }
    }

    fn hide(&self) {
        self.update_sync_file(SyncUpdateProgressAction::Hide);
        self.handler().hide();
    }

    fn show(&self) {
        self.update_sync_file(SyncUpdateProgressAction::Show);
        self.handler().show();
    }

    fn println(&self, message: String) {
        let message = self.format_message(message);
        self.update_sync_file(SyncUpdateProgressAction::Println(message.clone()));
        self.handler().println(message);
    }
}

pub struct SyncUpdateListener<'a> {
    expected_init: Option<String>,
    current_handler: Option<UpProgressHandler<'a>>,
    current_handler_id: Option<String>,
    seen_init: bool,
}

impl SyncUpdateListener<'_> {
    pub fn new() -> Self {
        Self {
            expected_init: None,
            current_handler: None,
            current_handler_id: None,
            seen_init: false,
        }
    }

    pub fn expect_init(&mut self, init: &str) -> &mut Self {
        self.expected_init = Some(init.to_string());
        self
    }

    pub fn follow(&mut self, file: &std::fs::File) -> Result<(), SyncUpdateError> {
        let mut lines = std::io::BufReader::new(file).lines();

        self.current_handler = None;
        self.current_handler_id = None;

        loop {
            while let Some(line) = lines.next() {
                if let Ok(line) = line {
                    if let Err(err) = self.handle_line(&line) {
                        match err {
                            SyncUpdateError::MismatchedInit(..) => {
                                return Err(err);
                            }
                            _ => {
                                omni_warning!(format!("{}", err));
                            }
                        }
                    }
                }
            }
            if file.try_lock_exclusive().is_ok() {
                file.unlock()?;
                break Ok(());
            } else {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    fn handle_line(&mut self, line: &str) -> Result<(), SyncUpdateError> {
        // JSON deserialize the line into a SyncUpdateOperation object
        // If the line is not valid JSON, return an error
        let sync_update = serde_json::from_str::<SyncUpdateOperation>(&line)?;
        match sync_update {
            SyncUpdateOperation::Init(init) => {
                if self.seen_init {
                    return Err(SyncUpdateError::AlreadyInit);
                }

                self.seen_init = true;

                if let Some(ref expected_init) = self.expected_init {
                    if expected_init != init.as_str() {
                        return Err(SyncUpdateError::MismatchedInit(
                            init.to_string(),
                            expected_init.to_string(),
                        ));
                    }
                }
                omni_info!("attaching to running operation".to_string());
            }
            SyncUpdateOperation::Exit(exit_code) => {
                exit(exit_code);
            }
            SyncUpdateOperation::OmniError(error) => {
                omni_error!(error);
            }
            SyncUpdateOperation::OmniWarning(warning) => {
                omni_warning!(warning);
            }
            SyncUpdateOperation::OmniInfo(info) => {
                omni_info!(info);
            }
            SyncUpdateOperation::Progress(progress) => {
                let need_new_handler = match self.current_handler_id {
                    Some(ref current_handler_id) => current_handler_id != progress.handler_id(),
                    _ => true,
                };

                if need_new_handler {
                    // Create a new handler for the new handler id
                    let new_handler = UpProgressHandler::new(progress.step());
                    new_handler.init(progress.desc().to_string());

                    self.current_handler = Some(new_handler);
                    self.current_handler_id = Some(progress.handler_id().to_string());
                }

                if let Some(ref mut handler) = self.current_handler {
                    handler.perform_sync_action(progress.action());
                } else {
                    return Err(SyncUpdateError::NoProgressHandler);
                }
            }
        }

        return Ok(());
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncUpdateOperation {
    Init(SyncUpdateInit),
    Exit(i32),
    Progress(SyncUpdateProgress),
    #[serde(rename = "error")]
    OmniError(String),
    #[serde(rename = "warning")]
    OmniWarning(String),
    #[serde(rename = "info")]
    OmniInfo(String),
}

impl SyncUpdateOperation {
    pub fn dump_to_file(&self, mut file: &std::fs::File) -> Result<(), std::io::Error> {
        // Serialize the update to JSON in a single line
        let update_json = serde_json::to_string(self)?;

        // Add a line return at the end of the JSON
        let update_json = format!("{}\n", update_json);

        // Write the JSON to the file
        file.write_all(update_json.as_bytes())?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncUpdateInit {
    Up,
    Down,
}

impl SyncUpdateInit {
    pub fn as_str(&self) -> &str {
        match self {
            SyncUpdateInit::Up => "up",
            SyncUpdateInit::Down => "down",
        }
    }
}

impl ToString for SyncUpdateInit {
    fn to_string(&self) -> String {
        self.as_str().to_string()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncUpdateProgress {
    #[serde(rename = "id", skip_serializing_if = "str::is_empty")]
    handler_id: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    desc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<(usize, usize)>,
    #[serde(flatten)]
    action: SyncUpdateProgressAction,
}

impl SyncUpdateProgress {
    pub fn handler_id(&self) -> &str {
        &self.handler_id
    }

    pub fn desc(&self) -> &str {
        &self.desc
    }

    pub fn step(&self) -> Option<(usize, usize)> {
        self.step
    }

    pub fn action(&self) -> &SyncUpdateProgressAction {
        &self.action
    }
}

#[derive(Debug)]
pub enum SyncUpdateProgressAction {
    Progress(String),
    Success(Option<String>),
    Error(Option<String>),
    Hide,
    Show,
    Println(String),
}

impl SyncUpdateProgressAction {
    fn from_map(map: BTreeMap<String, String>) -> Option<SyncUpdateProgressAction> {
        let action = match map.get("action") {
            Some(action) => action,
            None => return None,
        };

        match action.as_str() {
            "progress" => {
                if let Some(message) = map.get("message") {
                    Some(SyncUpdateProgressAction::Progress(message.clone()))
                } else {
                    None
                }
            }
            "success" => {
                let message = map.get("message").cloned();
                Some(SyncUpdateProgressAction::Success(message))
            }
            "error" => {
                let message = map.get("message").cloned();
                Some(SyncUpdateProgressAction::Error(message))
            }
            "hide" => Some(SyncUpdateProgressAction::Hide),
            "show" => Some(SyncUpdateProgressAction::Show),
            "println" => {
                if let Some(message) = map.get("message") {
                    Some(SyncUpdateProgressAction::Println(message.clone()))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn as_map(&self) -> BTreeMap<String, String> {
        let mut as_map = BTreeMap::new();
        match self {
            SyncUpdateProgressAction::Progress(message) => {
                as_map.insert("action".to_string(), "progress".to_string());
                as_map.insert("message".to_string(), message.clone());
            }
            SyncUpdateProgressAction::Success(message) => {
                as_map.insert("action".to_string(), "success".to_string());
                if let Some(message) = message {
                    as_map.insert("message".to_string(), message.clone());
                }
            }
            SyncUpdateProgressAction::Error(message) => {
                as_map.insert("action".to_string(), "error".to_string());
                if let Some(message) = message {
                    as_map.insert("message".to_string(), message.clone());
                }
            }
            SyncUpdateProgressAction::Hide => {
                as_map.insert("action".to_string(), "hide".to_string());
            }
            SyncUpdateProgressAction::Show => {
                as_map.insert("action".to_string(), "show".to_string());
            }
            SyncUpdateProgressAction::Println(message) => {
                as_map.insert("action".to_string(), "println".to_string());
                as_map.insert("message".to_string(), message.clone());
            }
        }
        as_map
    }
}

impl Serialize for SyncUpdateProgressAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.as_map().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SyncUpdateProgressAction {
    fn deserialize<D>(deserializer: D) -> Result<SyncUpdateProgressAction, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        // Deserialize the JSON value into a BTreeMap<String, String>
        let map = BTreeMap::<String, String>::deserialize(deserializer)?;

        // Convert the map into a SyncUpdateProgressAction using the from_map method
        match SyncUpdateProgressAction::from_map(map) {
            Some(action) => Ok(action),
            None => Err(serde::de::Error::custom("invalid SyncUpdateProgressAction")),
        }
    }
}
