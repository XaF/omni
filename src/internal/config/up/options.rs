use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpOptions<'a> {
    pub read_cache: bool,
    pub write_cache: bool,
    pub fail_on_upgrade: bool,
    pub upgrade: bool,
    #[serde(skip)]
    pub lock_file: Option<&'a std::fs::File>,
}

impl Default for UpOptions<'_> {
    fn default() -> Self {
        Self {
            read_cache: true,
            write_cache: true,
            fail_on_upgrade: false,
            upgrade: false,
            lock_file: None,
        }
    }
}

impl<'a> UpOptions<'a> {
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

    pub fn upgrade(mut self, upgrade: bool) -> Self {
        self.upgrade = upgrade;
        self
    }

    pub fn lock_file(mut self, lock_file: &'a std::fs::File) -> Self {
        self.lock_file = Some(lock_file);
        self
    }
}
