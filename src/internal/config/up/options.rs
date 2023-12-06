use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpOptions {
    pub cache_enabled: bool,
}

impl UpOptions {
    pub fn new() -> Self {
        Self {
            cache_enabled: true,
        }
    }

    pub fn cache(mut self, cache_enabled: bool) -> Self {
        self.cache_enabled = cache_enabled;
        self
    }
}

impl Default for UpOptions {
    fn default() -> Self {
        Self::new()
    }
}
