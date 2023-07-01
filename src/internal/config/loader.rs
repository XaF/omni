use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Mutex;

use fs4::FileExt;
use lazy_static::lazy_static;
use serde_yaml;

use crate::internal::config::ConfigExtendStrategy;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;
use crate::internal::env::git_env;
use crate::internal::env::ENV;
use crate::internal::env::HOME;
use crate::internal::user_interface::StringColor;
use crate::omni_error;

lazy_static! {
    #[derive(Debug)]
    static ref CONFIG_LOADER_PER_PATH: Mutex<ConfigLoaderPerPath> = Mutex::new(ConfigLoaderPerPath::new());

    #[derive(Debug)]
    static ref CONFIG_LOADER_GLOBAL: ConfigLoader = ConfigLoader::new();
}

pub fn config_loader(path: &str) -> ConfigLoader {
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut config_loader_per_path = CONFIG_LOADER_PER_PATH.lock().unwrap();
    config_loader_per_path.get(&path).clone()
}

pub fn global_config_loader() -> ConfigLoader {
    CONFIG_LOADER_GLOBAL.clone()
}

#[derive(Debug)]
pub struct ConfigLoaderPerPath {
    loaders: HashMap<String, ConfigLoader>,
}

impl ConfigLoaderPerPath {
    fn new() -> Self {
        Self {
            loaders: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &ConfigLoader {
        if !self.loaders.contains_key(path) {
            self.loaders
                .insert(path.to_owned(), CONFIG_LOADER_GLOBAL.get_local(path));
        }

        self.loaders.get(path).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct ConfigLoader {
    pub loaded_config_files: Vec<String>,
    pub raw_config: ConfigValue,
}

impl ConfigLoader {
    fn new() -> Self {
        Self::new_global()
    }

    fn user_config_files() -> Vec<String> {
        vec![
            format!("{}/.omni", *HOME),
            format!("{}/.omni.yaml", *HOME),
            format!("{}/omni", ENV.xdg_config_home),
            format!("{}/omni.yaml", ENV.xdg_config_home),
            format!("{}/config", ENV.config_home),
            format!("{}/config.yaml", ENV.config_home),
            std::env::var("OMNI_CONFIG").unwrap_or("".to_owned()),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<String>>()
    }

    fn new_global() -> Self {
        let mut new_config_loader = Self {
            loaded_config_files: vec![],
            raw_config: ConfigValue::default(),
        };

        new_config_loader.import_config_files(Self::user_config_files(), vec!["user".to_owned()]);

        new_config_loader
    }

    fn new_empty() -> Self {
        Self {
            loaded_config_files: vec![],
            raw_config: ConfigValue::new_null(ConfigSource::Null, vec![]),
        }
    }

    pub fn edit_main_user_config_file<F>(edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut ConfigValue) -> bool,
    {
        // We will use the list of user config files, but in reverse, to
        // look for the first user configuration file that we can write
        // into, and that would be the LAST loaded file when reading
        // the configuration
        let config_files = Self::user_config_files()
            .into_iter()
            .rev()
            .map(PathBuf::from)
            .collect::<Vec<PathBuf>>();

        // We first try and look for a file that already exists and that
        // we can simply edit
        let mut found_file: Option<PathBuf> = None;
        for config_file in &config_files {
            if let Ok(metadata) = config_file.metadata() {
                if config_file.is_file() {
                    let permissions = metadata.permissions();
                    let mode = permissions.mode();
                    let has_read_access = mode & 0o400 == 0o400;
                    let has_write_access = mode & 0o200 == 0o200;

                    if has_read_access && has_write_access {
                        found_file = Some(config_file.clone());
                        break;
                    }
                }
            }
        }

        // But if we don't find any, we want to find the first path that exists
        // and is writeable, by looking at the same file list, but checking their
        // parents until finding one that exists. If the parent exists but does
        // not have write permissions, we will not be able to write to the file
        // so we can skip to the next file path
        if found_file.is_none() {
            'outer: for config_file in &config_files {
                let mut parent = config_file.clone();
                parent.pop();

                while !parent.exists() {
                    parent.pop();
                }

                if parent.is_dir() {
                    if let Ok(metadata) = parent.metadata() {
                        let permissions = metadata.permissions();
                        let mode = permissions.mode();
                        let has_write_access = mode & 0o200 == 0o200;

                        if has_write_access {
                            found_file = Some(config_file.clone());
                            break 'outer;
                        }
                    }
                }
            }
        }

        // If we get here and we still have no file, we can raise an error
        if found_file.is_none() {
            omni_error!("unable to find a writeable user config file");
            exit(1);
        }
        let found_file = found_file.unwrap();
        let file_path = format!("{}", found_file.display());

        Self::edit_user_config_file(file_path, edit_fn)
    }

    pub fn edit_user_config_file<F>(file_path: String, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut ConfigValue) -> bool,
    {
        // Check if the directory of the config file exists, otherwise create it recursively
        let file_pathbuf = PathBuf::from(file_path.clone());
        if let Some(parent) = file_pathbuf.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Open the file and take the lock
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file_path.clone())?;

        // Take the exclusive lock on the file, it will be release when `_file_lock` goes out of scope
        let _file_lock = file.lock_exclusive();

        // Now we'll want to open the file in question, and load its
        // configuration into a clean ConfigLoader.
        let mut config_loader = Self::new_empty();
        config_loader.import_config_file_with_strategy(
            &file_path,
            vec![],
            ConfigExtendStrategy::Raw,
        );

        // We can now call the edit function
        if edit_fn(&mut config_loader.raw_config) {
            let serialized = config_loader.raw_config.as_yaml();

            // Replace entirely the content of the file with the new JSON
            file.set_len(0)?;
            file.seek(io::SeekFrom::Start(0))?;
            file.write_all(serialized.as_bytes())?;
        }

        Ok(())
    }

    // fn new_local(path: &str) -> Self {
    // ConfigLoader::new_global().get_local(path)
    // }

    pub fn get_local(&self, path: &str) -> Self {
        let mut new_config_loader = Self {
            loaded_config_files: self.loaded_config_files.clone(),
            raw_config: self.raw_config.clone(),
        };

        let git_repo = git_env(path);
        if !git_repo.in_repo() {
            return new_config_loader;
        }

        let git_repo_root = git_repo.root().clone().unwrap();
        let mut git_config_files = vec![];
        git_config_files.push(format!("{}/.omni", git_repo_root));
        git_config_files.push(format!("{}/.omni.yaml", git_repo_root));
        git_config_files.push(format!("{}/.omni/config", git_repo_root));
        git_config_files.push(format!("{}/.omni/config.yaml", git_repo_root));

        new_config_loader.import_config_files(git_config_files, vec!["git_repo".to_owned()]);

        new_config_loader
    }

    pub fn import_config_files(&mut self, config_files: Vec<String>, labels: Vec<String>) {
        for config_file in &config_files.clone() {
            self.import_config_file(config_file, labels.clone());
        }
    }

    pub fn import_config_file(&mut self, config_file: &String, labels: Vec<String>) {
        self.import_config_file_with_strategy(config_file, labels, ConfigExtendStrategy::Default)
    }

    pub fn import_config_file_with_strategy(
        &mut self,
        config_file: &String,
        labels: Vec<String>,
        strategy: ConfigExtendStrategy,
    ) {
        let file = File::open(config_file);
        if !file.is_ok() {
            return;
        }

        let mut file = file.unwrap();
        let mut contents = String::new();
        if let Err(_err) = file.read_to_string(&mut contents) {
            return;
        }

        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&contents) {
            self.loaded_config_files.push(config_file.to_string());
            let config_value = ConfigValue::from_value(
                ConfigSource::File(config_file.to_string()),
                labels.clone(),
                value,
            );
            self.raw_config.extend(config_value, strategy, vec![]);
        }
    }
}
