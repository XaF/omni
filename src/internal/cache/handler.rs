use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::PathBuf;

use fs4::FileExt;
use serde::Deserialize;
use serde::Serialize;
use serde_json;

use crate::internal::cache::CacheObject;
use crate::internal::config;

// Read the content of the cache file and parse it as structured JSON;
// we will want to deprecate and remove that at some point, but it will
// allow for moving from omni <=0.0.15 to any newer version that will
// now expect a separate file per cache category
#[derive(Debug, Serialize, Deserialize)]
struct OldCache {
    asdf_operation: serde_json::Value,
    homebrew_operation: serde_json::Value,
    omni_path_updates: serde_json::Value,
    trusted_repositories: OldCacheTrustedRepositories,
    up_environments: serde_json::Value,
}
#[derive(Debug, Serialize, Deserialize)]
struct OldCacheTrustedRepositories {
    repositories: serde_json::Value,
    updated_at: serde_json::Value,
}
#[derive(Debug, Serialize, Deserialize)]
struct NewCacheRepositories {
    trusted: serde_json::Value,
    updated_at: serde_json::Value,
}

fn convert_cache_to_dir() -> io::Result<()> {
    let cache_path = PathBuf::from(config(".").cache.path.clone());

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

    // Read the contents of the cache file into a OldCache struct
    let old_cache_file = File::open(&cache_path)?;
    let old_cache: OldCache = serde_json::from_reader(old_cache_file)?;

    // Write each cache category to a separate file in our temporary directory
    if old_cache.asdf_operation != serde_json::Value::Null {
        let asdf_operation_file_path = tmp_dir_path.join("asdf_operation.json");
        let mut asdf_operation_file = File::create(&asdf_operation_file_path)?;
        asdf_operation_file
            .write_all(serde_json::to_string(&old_cache.asdf_operation)?.as_bytes())?;
    }
    if old_cache.homebrew_operation != serde_json::Value::Null {
        let homebrew_operation_file_path = tmp_dir_path.join("homebrew_operation.json");
        let mut homebrew_operation_file = File::create(&homebrew_operation_file_path)?;
        homebrew_operation_file
            .write_all(serde_json::to_string(&old_cache.homebrew_operation)?.as_bytes())?;
    }
    if old_cache.omni_path_updates != serde_json::Value::Null {
        let omni_path_updates_file_path = tmp_dir_path.join("omnipath.json");
        let mut omni_path_updates_file = File::create(&omni_path_updates_file_path)?;
        omni_path_updates_file
            .write_all(serde_json::to_string(&old_cache.omni_path_updates)?.as_bytes())?;
    }
    if old_cache.trusted_repositories.repositories != serde_json::Value::Null {
        let repositories_file_path = tmp_dir_path.join("repositories.json");
        let mut repositories_file = File::create(&repositories_file_path)?;
        // Since we're changing the format, we're rewriting this into a new struct
        let new_repositories = NewCacheRepositories {
            trusted: old_cache.trusted_repositories.repositories,
            updated_at: old_cache.trusted_repositories.updated_at,
        };
        repositories_file.write_all(serde_json::to_string(&new_repositories)?.as_bytes())?;
    }
    if old_cache.up_environments != serde_json::Value::Null {
        let up_environments_file_path = tmp_dir_path.join("up_environments.json");
        let mut up_environments_file = File::create(&up_environments_file_path)?;
        up_environments_file
            .write_all(serde_json::to_string(&old_cache.up_environments)?.as_bytes())?;
    }

    // Rename the current cache file to a backup file, just in case
    let backup_file_path = cache_path.with_extension("json.bak");
    std::fs::rename(&cache_path, &backup_file_path)?;

    // Move the temporary directory to the cache path
    std::fs::rename(&tmp_dir_path, &cache_path)?;

    Ok(())
}

pub fn shared<C>(cache_name: &str) -> io::Result<C>
where
    C: CacheObject + Clone + Serialize + for<'a> Deserialize<'a>,
{
    convert_cache_to_dir()?;

    let cache_dir_path = PathBuf::from(config(".").cache.path.clone());
    let cache_path = cache_dir_path.join(format!("{}.json", cache_name));

    let file = File::open(cache_path)?;
    // TODO: re-evaluate, but shared lock does not seem necessary
    // let _file_lock = file.lock_shared();

    let cache: C = serde_json::from_reader(file)?;
    Ok(cache)
}

pub fn exclusive<C, F1, F2>(cache_name: &str, processing_fn: F1, set_cache_fn: F2) -> io::Result<C>
where
    C: CacheObject + Clone + Serialize + for<'a> Deserialize<'a>,
    F1: FnOnce(&mut C) -> bool,
    F2: FnOnce(C),
{
    convert_cache_to_dir()?;

    // Check if the directory of the cache file exists, otherwise create it recursively
    let cache_dir_path = PathBuf::from(config(".").cache.path.clone());
    if !cache_dir_path.exists() {
        std::fs::create_dir_all(&cache_dir_path)?;
    }
    let cache_path = cache_dir_path.join(format!("{}.json", cache_name));

    // Open the cache file
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(cache_path)?;

    // Take the exclusive lock on the file, it will be release when `_file_lock` goes out of scope
    let _file_lock = file.lock_exclusive();

    // Read the content of the file, and parse it as JSON
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let load_cache: Result<C, _> = serde_json::from_str(&content);
    let mut cache = if let Ok(_) = load_cache {
        load_cache.unwrap().clone()
    } else {
        C::new_empty()
    };

    // Call the provided closure, passing the cache reference, and check if there is a request
    // to update the cache with the new data
    if processing_fn(&mut cache) {
        let serialized = serde_json::to_string(&cache).unwrap();

        // Replace entirely the content of the file with the new JSON
        file.set_len(0)?;
        file.seek(io::SeekFrom::Start(0))?;
        file.write_all(serialized.as_bytes())?;

        // Update the global cache variable with the new data
        set_cache_fn(cache.clone());
    }

    // Return the cache as modified by the closure, no matter if the file was updated or not
    Ok(cache)
}
