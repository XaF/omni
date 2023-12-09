use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpOptions {
    pub read_cache: bool,
    pub write_cache: bool,
}

impl UpOptions {
    pub fn new() -> Self {
        Self {
            read_cache: true,
            write_cache: false,
        }
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
}

impl Default for UpOptions {
    fn default() -> Self {
        Self::new()
    }
}
