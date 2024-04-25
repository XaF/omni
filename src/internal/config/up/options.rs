use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpOptions {
    pub read_cache: bool,
    pub write_cache: bool,
    pub fail_on_upgrade: bool,
}

impl Default for UpOptions {
    fn default() -> Self {
        Self {
            read_cache: true,
            write_cache: true,
            fail_on_upgrade: false,
        }
    }
}

impl UpOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cache(mut self, read_cache: bool) -> Self {
        self.read_cache = read_cache;
        self
    }

    pub fn cache_disabled(mut self) -> Self {
        self.read_cache = false;
        self.write_cache = false;
        self
    }

    pub fn fail_on_upgrade(mut self, fail_on_upgrade: bool) -> Self {
        self.fail_on_upgrade = fail_on_upgrade;
        self
    }
}
