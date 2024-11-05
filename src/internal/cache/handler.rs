use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::PathBuf;

use fs4::fs_std::FileExt;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::migration::convert_cache;
use crate::internal::cache::CacheObject;
use crate::internal::config::global_config;

pub fn shared<C>(cache_name: &str) -> io::Result<C>
where
    C: CacheObject + Clone + Serialize + for<'a> Deserialize<'a>,
{
    convert_cache()?;

    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
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
    convert_cache()?;

    // Check if the directory of the cache file exists, otherwise create it recursively
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    if !cache_dir_path.exists() {
        std::fs::create_dir_all(&cache_dir_path)?;
    }
    let cache_path = cache_dir_path.join(format!("{}.json", cache_name));

    // Open the cache file
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache_path)?;

    // Take the exclusive lock on the file, it will be release when `_file_lock` goes out of scope
    let _file_lock = file.lock_exclusive();

    // Read the content of the file, and parse it as JSON
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let load_cache: Result<C, _> = serde_json::from_str(&content);
    let mut cache = if let Ok(load_cache) = load_cache {
        load_cache.clone()
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
