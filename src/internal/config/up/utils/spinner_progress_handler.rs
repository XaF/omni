use std::cell::Cell;

use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressDrawTarget;
use indicatif::ProgressStyle;
use tokio::time::Duration;

use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::user_interface::ensure_newline_from_len;
use crate::internal::user_interface::print::strip_ansi_codes;
use crate::internal::user_interface::StringColor;

#[derive(Debug, Clone)]
pub struct SpinnerProgressHandler {
    spinner: ProgressBar,
    template: String,
    ensure_newline: bool,
    base_len: usize,
    last_message_len: Cell<usize>,
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

        // Base len = length of the description + 3 (1 for the spinner and 2 for the spaces)
        let mut base_len = desc.len() + 4;

        let mut ensure_newline = true;
        let spinner = if let Some(multiprogress) = multiprogress {
            ensure_newline = false;
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
            // Increase the base length to account for the prefix, which is the padding
            // length times two (current and total) + the 2 brackets + the slash + the space
            base_len += padding * 2 + 4;
        }

        SpinnerProgressHandler {
            spinner,
            template,
            ensure_newline,
            base_len,
            last_message_len: Cell::new(0),
        }
    }

    fn replace_spinner(&self, replace_by: String) {
        let template = self.template.replace("{spinner}", replace_by.as_str());
        self.spinner.set_style(
            ProgressStyle::default_spinner()
                .template(template.as_str())
                .unwrap(),
        );
    }

    fn update_last_message(&self, message: &str) {
        let message = strip_ansi_codes(message);
        let len = message.len();
        self.last_message_len.set(len);
    }

    fn cur_len(&self) -> usize {
        self.base_len + self.last_message_len.get()
    }

    fn ensure_newline(&self) {
        if self.ensure_newline {
            ensure_newline_from_len(self.cur_len());
        }
    }
}

impl ProgressHandler for SpinnerProgressHandler {
    fn println(&self, message: String) {
        self.spinner.println(message);
    }

    fn progress(&self, message: String) {
        self.update_last_message(&message);
        self.spinner.set_message(message);
    }

    fn success(&self) {
        let message = "done".to_string();
        self.update_last_message(&message);
        // self.replace_spinner("✔".green());
        // self.spinner.finish();
        self.success_with_message(message);
        self.ensure_newline();
    }

    fn success_with_message(&self, message: String) {
        self.update_last_message(&message);
        self.replace_spinner("✔".green());
        self.spinner.finish_with_message(message);
        self.ensure_newline();
    }

    fn error(&self) {
        let message = self.spinner.message().to_string();
        self.update_last_message(&message);
        self.replace_spinner("✖".red());
        self.spinner.finish_with_message(message.red());
        self.ensure_newline();
    }

    fn error_with_message(&self, message: String) {
        self.update_last_message(&message);
        self.replace_spinner("✖".red());
        self.spinner.finish_with_message(message.red());
        self.ensure_newline();
    }

    fn hide(&self) {
        self.spinner.set_draw_target(ProgressDrawTarget::hidden());
    }

    fn show(&self) {
        self.spinner.set_draw_target(ProgressDrawTarget::stderr());
    }
}
