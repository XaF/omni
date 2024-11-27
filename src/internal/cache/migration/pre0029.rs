use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::offsetdatetime_hashmap;
use crate::internal::cache::utils;
use crate::internal::config::global_config;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::git_env_fresh;
use crate::internal::ORG_LOADER;

// In 0.0.29, we are changing the format of the up environments cache to handle
// versioned environments. This means that instead of having a list of workdir
// environments, that list will target a specific version of the environment,
// which will be stored in a separate list.
// This allows to build a new environment without breaking the current one in
// case of any issue, and to keep traces of previous environments and when they
// were used. However, this requires the following changes:
// - up_environments.json
//     - need to generate version names for the entries and convert them to
//       the new format, from { "env": { "repo" => env } } to
//       { "workdir_env": { "repo" => "version" },
//         "versioned_env": { "version" => env },
//         "history": [ { "wd": "repo",
//                        "sha": "head_sha",
//                        "env": "version",
//                        "from": "date" } ],
// - github_release_operation.json
//    - Replace the references to the repository by references to the versions
// - asdf_operation.json
//    - Replace the references to the repository by references to the versions
// - homebrew_operation.json
//    - Replace the references to the repository by references to the versions

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029UpEnvironment {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<Pre0029UpVersion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<Pre0029UpEnvVar>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config_modtimes: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub config_hash: String,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub last_assigned_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct Pre0029UpVersion {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_real_name: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct Pre0029UpEnvVar {
    #[serde(
        rename = "n",
        alias = "name",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(
        rename = "v",
        alias = "value",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<String>,
    #[serde(
        rename = "o",
        alias = "operation",
        default,
        skip_serializing_if = "EnvOperationEnum::is_default"
    )]
    pub operation: EnvOperationEnum,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pre0029UpEnvironmentsCache {
    env: HashMap<String, Pre0029UpEnvironment>,
    updated_at: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Post0029UpEnvironmentsCache {
    workdir_env: HashMap<String, String>,
    versioned_env: HashMap<String, Pre0029UpEnvironment>,
    history: Vec<Post0029UpEnvironmentHistoryEntry>,
    updated_at: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Post0029UpEnvironmentHistoryEntry {
    #[serde(rename = "wd")]
    workdir_id: String,
    #[serde(rename = "sha")]
    head_sha: String,
    #[serde(rename = "env")]
    env_version_id: String,
    #[serde(rename = "from")]
    used_from_date: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029AsdfOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<Pre0029AsdfInstalled>,
    #[serde(
        default = "Pre0029AsdfOperationUpdateCache::new",
        skip_serializing_if = "Pre0029AsdfOperationUpdateCache::is_empty"
    )]
    pub update_cache: Pre0029AsdfOperationUpdateCache,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029AsdfInstalled {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_real_name: Option<String>,
    pub version: String,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029AsdfOperationUpdateCache {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub asdf_updated_at: OffsetDateTime,
    #[serde(
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty",
        with = "offsetdatetime_hashmap"
    )]
    pub plugins_updated_at: HashMap<String, OffsetDateTime>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub plugins_versions: HashMap<String, Pre0029AsdfOperationUpdateCachePluginVersions>,
}

impl Pre0029AsdfOperationUpdateCache {
    pub fn new() -> Self {
        Self {
            asdf_updated_at: utils::origin_of_time(),
            plugins_updated_at: HashMap::new(),
            plugins_versions: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.asdf_updated_at == utils::origin_of_time()
            && self.plugins_updated_at.is_empty()
            && self.plugins_versions.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029AsdfOperationUpdateCachePluginVersions {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029HomebrewOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<Pre0029HomebrewInstalled>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub tapped: Vec<Pre0029HomebrewTapped>,
    #[serde(
        default = "Pre0029HomebrewOperationUpdateCache::new",
        skip_serializing_if = "Pre0029HomebrewOperationUpdateCache::is_empty"
    )]
    pub update_cache: Pre0029HomebrewOperationUpdateCache,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029HomebrewInstalled {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub cask: bool,
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub installed: bool,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029HomebrewTapped {
    pub name: String,
    #[serde(default = "utils::set_false", skip_serializing_if = "utils::is_false")]
    pub tapped: bool,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029HomebrewOperationUpdateCache {
    #[serde(
        default = "Pre0029HomebrewOperationUpdateCacheHomebrew::new",
        skip_serializing_if = "Pre0029HomebrewOperationUpdateCacheHomebrew::is_empty"
    )]
    pub homebrew: Pre0029HomebrewOperationUpdateCacheHomebrew,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub install: HashMap<String, Pre0029HomebrewOperationUpdateCacheInstall>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub tap: HashMap<String, Pre0029HomebrewOperationUpdateCacheTap>,
}

impl Pre0029HomebrewOperationUpdateCache {
    pub fn new() -> Self {
        Self {
            homebrew: Pre0029HomebrewOperationUpdateCacheHomebrew::new(),
            install: HashMap::new(),
            tap: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.homebrew.is_empty() && self.install.is_empty() && self.tap.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029HomebrewOperationUpdateCacheHomebrew {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_path: Option<String>,
}

impl Pre0029HomebrewOperationUpdateCacheHomebrew {
    pub fn new() -> Self {
        Self {
            updated_at: utils::origin_of_time(),
            bin_path: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bin_path.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029HomebrewOperationUpdateCacheInstall {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub checked_at: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_paths: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029HomebrewOperationUpdateCacheTap {
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029GithubReleaseOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<Pre0029GithubReleaseInstalled>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub releases: HashMap<String, Pre0029GithubReleases>,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029GithubReleaseInstalled {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default = "utils::origin_of_time", with = "time::serde::rfc3339")]
    pub last_required_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029GithubReleases {
    pub releases: Vec<Pre0029GithubReleaseVersion>,
    #[serde(default = "OffsetDateTime::now_utc", with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029GithubReleaseVersion {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Pre0029GithubReleaseAsset>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pre0029GithubReleaseAsset {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub browser_download_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_type: String,
    #[serde(default)]
    pub size: u64,
}

pub fn convert_cache_pre_0_0_29() -> io::Result<()> {
    let cache_path = PathBuf::from(global_config().cache.path.clone());

    // If the cache path does not exist, there is nothing to do
    if !cache_path.exists() {
        return Ok(());
    }

    // If the up_enviroments.json file does not exist, there is nothing to do
    let up_environments_path = cache_path.join("up_environments.json");
    if !up_environments_path.exists() || up_environments_path.metadata()?.len() == 0 {
        return Ok(());
    }

    // Read the contents of the UpEnvironments cache file into a Pre0029UpEnvironmentsCache object
    let pre0029_cache_file = File::open(&up_environments_path)?;
    let pre0029_cache: Pre0029UpEnvironmentsCache =
        match serde_json::from_reader(pre0029_cache_file) {
            Ok(cache) => cache,
            Err(err) => {
                // If the error is that we are missing the field `env`, we can assume
                // that the cache is already in the new format, so we can just return
                // without doing anything
                if err.to_string().contains("missing field `env`") {
                    return Ok(());
                }
                return Err(err.into());
            }
        };

    // Nothing to do if the cache is empty
    if pre0029_cache.env.is_empty() {
        return Ok(());
    }

    // Prepare a directory to dump the updated cache files
    let tmp_dir = env::temp_dir();
    let tmp_dir_path = loop {
        let tmp_dir_path = tmp_dir.join(format!("omni-cache.d-{}", uuid::Uuid::new_v4()));
        if let Ok(()) = std::fs::create_dir_all(&tmp_dir_path) {
            break tmp_dir_path;
        }
    };

    // First, go over the UpEnvironment to convert in the new format
    let mut post0029_cache = Post0029UpEnvironmentsCache {
        workdir_env: HashMap::new(),
        versioned_env: HashMap::new(),
        history: Vec::new(),
        updated_at: pre0029_cache.updated_at,
    };
    for (wd_id, env) in pre0029_cache.env {
        let uuid = uuid::Uuid::new_v4();
        let short_uuid = uuid.to_string()[..8].to_string();
        let version = format!("{}%{}", wd_id, short_uuid);

        post0029_cache
            .workdir_env
            .insert(wd_id.clone(), version.clone());
        post0029_cache.versioned_env.insert(version.clone(), env);

        // Use the workdir_id to resolve the head sha of the repository, if possible
        // otherwise skip the entry entirely
        let repo_path = ORG_LOADER.find_repo(&wd_id, false, false, false);
        let repo_path = match repo_path {
            Some(path) => path.to_string_lossy().to_string(),
            None => continue,
        };
        let head_sha = match git_env_fresh(&repo_path).commit() {
            Some(sha) => sha.to_string(),
            None => continue,
        };
        post0029_cache
            .history
            .push(Post0029UpEnvironmentHistoryEntry {
                workdir_id: wd_id,
                head_sha,
                env_version_id: version,
                used_from_date: post0029_cache.updated_at.clone(),
            });
    }

    // Write the new cache to the temporary directory
    let post0029_cache_path = tmp_dir_path.join("up_environments.json");
    let mut post0029_cache_file = File::create(&post0029_cache_path)?;
    post0029_cache_file.write_all(serde_json::to_string(&post0029_cache)?.as_bytes())?;

    // Keep track of the files that were modified
    let mut modified_files = vec![post0029_cache_path];

    // Now we need to go over the other resources, and replace the references to the
    // repositories by references to the versions; we can use the cache handlers to get
    // the objects, and then we can dump the new version in the temporary directory

    // First the AsdfOperationCache
    let asdf_operation_path = cache_path.join("asdf_operation.json");
    let asdf_operation: Option<Pre0029AsdfOperationCache> = match File::open(&asdf_operation_path) {
        Ok(file) => serde_json::from_reader(file).ok(),
        Err(_) => None,
    };
    if let Some(asdf_operation) = asdf_operation {
        // Create a copy
        let mut asdfop = asdf_operation.clone();

        // Go over the .installed objects, and modify any reference in the .required_by
        // parameter if one of the wd_id appears there
        let mut updated = false;
        for install in asdfop.installed.iter_mut() {
            let mut required_by = install.required_by.clone();
            for wd_id in install.required_by.iter() {
                if let Some(version) = post0029_cache.workdir_env.get(wd_id) {
                    required_by.remove(wd_id);
                    required_by.insert(version.clone());
                }
            }
            if required_by != install.required_by {
                install.required_by = required_by;
                updated = true;
            }
        }

        if updated {
            // Write the new cache to the temporary directory
            let asdf_operation_path = tmp_dir_path.join("asdf_operation.json");
            let mut asdf_operation_file = File::create(&asdf_operation_path)?;
            asdf_operation_file.write_all(serde_json::to_string(&asdfop)?.as_bytes())?;

            modified_files.push(asdf_operation_path);
        }
    }

    // The HomebrewOperationCache
    let homebrew_operation_path = cache_path.join("homebrew_operation.json");
    let homebrew_operation: Option<Pre0029HomebrewOperationCache> =
        match File::open(&homebrew_operation_path) {
            Ok(file) => serde_json::from_reader(file).ok(),
            Err(_) => None,
        };
    if let Some(homebrew_operation) = homebrew_operation {
        // Create a copy
        let mut homebrewop = homebrew_operation.clone();

        // Go over the .installed and .tapped objects, and modify any reference
        // in the .required_by parameter if one of the wd_id appears there
        let mut updated = false;

        for install in homebrewop.installed.iter_mut() {
            let mut required_by = install.required_by.clone();
            for wd_id in install.required_by.iter() {
                if let Some(version) = post0029_cache.workdir_env.get(wd_id) {
                    required_by.remove(wd_id);
                    required_by.insert(version.clone());
                }
            }
            if required_by != install.required_by {
                install.required_by = required_by;
                updated = true;
            }
        }

        for tap in homebrewop.tapped.iter_mut() {
            let mut required_by = tap.required_by.clone();
            for wd_id in tap.required_by.iter() {
                if let Some(version) = post0029_cache.workdir_env.get(wd_id) {
                    required_by.remove(wd_id);
                    required_by.insert(version.clone());
                }
            }
            if required_by != tap.required_by {
                tap.required_by = required_by;
                updated = true;
            }
        }

        if updated {
            // Write the new cache to the temporary directory
            let homebrew_operation_path = tmp_dir_path.join("homebrew_operation.json");
            let mut homebrew_operation_file = File::create(&homebrew_operation_path)?;
            homebrew_operation_file.write_all(serde_json::to_string(&homebrewop)?.as_bytes())?;

            modified_files.push(homebrew_operation_path);
        }
    }

    // The GithubReleaseOperationCache
    let github_release_operation_path = cache_path.join("github_release_operation.json");
    let github_release_operation: Option<Pre0029GithubReleaseOperationCache> =
        match File::open(&github_release_operation_path) {
            Ok(file) => serde_json::from_reader(file).ok(),
            Err(_) => None,
        };
    if let Some(github_release_operation) = github_release_operation {
        // Create a copy
        let mut ghrop = github_release_operation.clone();

        // Go over the .installed objects, and modify any reference in the .required_by
        // parameter if one of the wd_id appears there
        let mut updated = false;
        for install in ghrop.installed.iter_mut() {
            let mut required_by = install.required_by.clone();
            for wd_id in install.required_by.iter() {
                if let Some(version) = post0029_cache.workdir_env.get(wd_id) {
                    required_by.remove(wd_id);
                    required_by.insert(version.clone());
                }
            }
            if required_by != install.required_by {
                install.required_by = required_by;
                updated = true;
            }
        }

        if updated {
            // Write the new cache to the temporary directory
            let github_release_operation_path = tmp_dir_path.join("github_release_operation.json");
            let mut github_release_operation_file = File::create(&github_release_operation_path)?;
            github_release_operation_file.write_all(serde_json::to_string(&ghrop)?.as_bytes())?;

            modified_files.push(github_release_operation_path);
        }
    }

    // Finally, go over the files that were modified, rename the original files and move
    // the new files to the original location
    for file in modified_files {
        let original_file = cache_path.join(file.file_name().unwrap());
        std::fs::rename(&original_file, original_file.with_extension("json.pre0029"))?;
        std::fs::rename(file, original_file)?;
    }

    // And remove the temporary directory
    std::fs::remove_dir_all(tmp_dir_path)?;

    Ok(())
}
