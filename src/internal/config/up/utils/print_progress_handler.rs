use std::cell::RefCell;

use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::user_interface::StringColor;

#[derive(Debug, Clone)]
pub struct PrintProgressHandler {
    template: String,
    message: RefCell<String>,
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

        PrintProgressHandler {
            template,
            message: RefCell::new("".to_string()),
        }
    }

    fn set_message(&self, message: impl ToString) {
        self.message.replace(message.to_string());
    }

    fn get_message(&self) -> String {
        self.message.borrow().clone()
    }
}

impl ProgressHandler for PrintProgressHandler {
    fn println(&self, message: String) {
        eprintln!("{}", message);
    }

    fn progress(&self, message: String) {
        self.set_message(&message);
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
        self.set_message(&message);
        eprintln!(
            "{}",
            self.template
                .replacen("{}", "✔".green().as_str(), 1)
                .replacen("{}", message.as_str(), 1)
        );
    }

    fn error(&self) {
        self.error_with_message(self.get_message());
    }

    fn error_with_message(&self, message: String) {
        self.set_message(&message);
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
