use tokio::time::Duration;

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

    pub fn askpass(&self) -> bool {
        self.askpass
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }
}
