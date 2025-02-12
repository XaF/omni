use std::env;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::global_config;
use crate::internal::utils::safe_rename;

// Read the content of the cache file and parse it as structured JSON;
// we will want to deprecate and remove that at some point, but it will
// allow for moving from omni <=0.0.15 to any newer version that will
// now expect a separate file per cache category
#[derive(Debug, Serialize, Deserialize)]
struct Pre0015Cache {
    #[serde(default)]
    asdf_operation: serde_json::Value,
    #[serde(default)]
    homebrew_operation: serde_json::Value,
    #[serde(default)]
    omni_path_updates: serde_json::Value,
    #[serde(default)]
    trusted_repositories: Pre0015CacheTrustedRepositories,
    #[serde(default)]
    up_environments: serde_json::Value,
}
#[derive(Debug, Serialize, Deserialize, Default)]
struct Pre0015CacheTrustedRepositories {
    #[serde(default)]
    repositories: serde_json::Value,
    #[serde(default)]
    updated_at: serde_json::Value,
}
#[derive(Debug, Serialize, Deserialize)]
struct Post0015CacheRepositories {
    trusted: serde_json::Value,
    updated_at: serde_json::Value,
}

pub fn convert_cache_pre_0_0_15() -> io::Result<()> {
    let cache_path = PathBuf::from(global_config().cache.path.clone());

    // If the cache path does not exist, there is nothing to do
    if !cache_path.exists() {
        return Ok(());
    }

    // If the cache path exists and is a directory, there is nothing to do
    if cache_path.is_dir() {
        return Ok(());
    }

    // If the cache path exists and is a file, we need to prepare the directory
    let tmp_dir = env::temp_dir();
    let tmp_dir_path = loop {
        let tmp_dir_path = tmp_dir.join(format!("omni-cache.d-{}", uuid::Uuid::new_v4()));
        if let Ok(()) = std::fs::create_dir_all(&tmp_dir_path) {
            break tmp_dir_path;
        }
    };

    // Read the contents of the cache file into a Pre0015Cache struct
    let pre0015_cache_file = File::open(&cache_path)?;
    let pre0015_cache: Pre0015Cache = serde_json::from_reader(pre0015_cache_file)?;

    // Write each cache category to a separate file in our temporary directory
    if pre0015_cache.asdf_operation != serde_json::Value::Null {
        let asdf_operation_file_path = tmp_dir_path.join("asdf_operation.json");
        let mut asdf_operation_file = File::create(asdf_operation_file_path)?;
        asdf_operation_file
            .write_all(serde_json::to_string(&pre0015_cache.asdf_operation)?.as_bytes())?;
    }
    if pre0015_cache.homebrew_operation != serde_json::Value::Null {
        let homebrew_operation_file_path = tmp_dir_path.join("homebrew_operation.json");
        let mut homebrew_operation_file = File::create(homebrew_operation_file_path)?;
        homebrew_operation_file
            .write_all(serde_json::to_string(&pre0015_cache.homebrew_operation)?.as_bytes())?;
    }
    if pre0015_cache.omni_path_updates != serde_json::Value::Null {
        let omni_path_updates_file_path = tmp_dir_path.join("omnipath.json");
        let mut omni_path_updates_file = File::create(omni_path_updates_file_path)?;
        omni_path_updates_file
            .write_all(serde_json::to_string(&pre0015_cache.omni_path_updates)?.as_bytes())?;
    }
    if pre0015_cache.trusted_repositories.repositories != serde_json::Value::Null {
        let repositories_file_path = tmp_dir_path.join("repositories.json");
        let mut repositories_file = File::create(repositories_file_path)?;
        // Since we're changing the format, we're rewriting this into a new struct
        let post0015_repositories = Post0015CacheRepositories {
            trusted: pre0015_cache.trusted_repositories.repositories,
            updated_at: pre0015_cache.trusted_repositories.updated_at,
        };
        repositories_file.write_all(serde_json::to_string(&post0015_repositories)?.as_bytes())?;
    }
    if pre0015_cache.up_environments != serde_json::Value::Null {
        let up_environments_file_path = tmp_dir_path.join("up_environments.json");
        let mut up_environments_file = File::create(up_environments_file_path)?;
        up_environments_file
            .write_all(serde_json::to_string(&pre0015_cache.up_environments)?.as_bytes())?;
    }

    // Rename the current cache file to a backup file, just in case
    let backup_file_path = cache_path.with_extension("json.pre0015");
    safe_rename(&cache_path, backup_file_path)?;

    // Move the temporary directory to the cache path
    safe_rename(&tmp_dir_path, &cache_path)?;

    Ok(())
}
