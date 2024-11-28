use crate::internal::config::up::utils::ProgressHandler;

#[derive(Debug, Clone)]
pub struct VoidProgressHandler {}

impl VoidProgressHandler {
    pub fn new() -> Self {
        VoidProgressHandler {}
    }
}

impl ProgressHandler for VoidProgressHandler {
    fn println(&self, _message: String) {
        // do nothing
    }

    fn progress(&self, _message: String) {
        // do nothing
    }

    fn success(&self) {
        // do nothing
    }

    fn success_with_message(&self, _message: String) {
        // do nothing
    }

    fn error(&self) {
        // do nothing
    }

    fn error_with_message(&self, _message: String) {
        // do nothing
    }

    fn hide(&self) {
        // do nothing
    }

    fn show(&self) {
        // do nothing
    }
}
