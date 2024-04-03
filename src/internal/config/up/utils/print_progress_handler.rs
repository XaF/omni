use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::user_interface::StringColor;

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
