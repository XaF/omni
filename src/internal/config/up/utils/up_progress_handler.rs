use once_cell::sync::OnceCell;

use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::shell_is_interactive;

pub struct UpProgressHandler<'a> {
    handler: OnceCell<Box<dyn ProgressHandler>>,
    step: Option<(usize, usize)>,
    prefix: String,
    parent: Option<&'a UpProgressHandler<'a>>,
    allow_ending: bool,
}

impl<'a> UpProgressHandler<'a> {
    pub fn new(progress: Option<(usize, usize)>) -> Self {
        UpProgressHandler {
            handler: OnceCell::new(),
            step: progress,
            prefix: "".to_string(),
            parent: None,
            allow_ending: true,
        }
    }

    pub fn init(&self, desc: String) -> bool {
        if self.handler.get().is_some() || self.parent.is_some() {
            return false;
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

    pub fn subhandler(&'a self, prefix: &dyn ToString) -> UpProgressHandler<'a> {
        UpProgressHandler {
            handler: OnceCell::new(),
            step: None,
            prefix: prefix.to_string(),
            parent: Some(self),
            allow_ending: false,
        }
    }

    pub fn step(&self) -> Option<(usize, usize)> {
        if let Some(parent) = self.parent {
            parent.step()
        } else {
            self.step
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
        self.handler().progress(message);
    }

    fn success(&self) {
        self.handler().success();
    }

    fn success_with_message(&self, message: String) {
        let message = self.format_message(message);
        if self.allow_ending {
            self.handler().success_with_message(message);
        } else {
            self.handler().progress(message);
        }
    }

    fn error(&self) {
        if self.allow_ending {
            self.handler().error();
        }
    }

    fn error_with_message(&self, message: String) {
        let message = self.format_message(message);
        if self.allow_ending {
            self.handler().error_with_message(message);
        } else {
            self.handler().progress(message);
        }
    }

    fn hide(&self) {
        self.handler().hide();
    }

    fn show(&self) {
        self.handler().show();
    }

    fn println(&self, message: String) {
        let message = self.format_message(message);
        self.handler().println(message);
    }
}
