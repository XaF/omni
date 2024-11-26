use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::params;
use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::CacheManagerError;
use crate::internal::config::global_config;

pub fn migrate_json_to_database(conn: &Connection) -> Result<(), CacheManagerError> {
    migrate_up_environments(conn)?;
    migrate_omnipath(conn)?;
    migrate_asdf_operation(conn)?;
    migrate_github_release_operation(conn)?;
    migrate_homebrew_operation(conn)?;
    migrate_prompts(conn)?;
    migrate_repositories(conn)?;

    Ok(())
}

fn handle_optional_date_string<T>(date: &Option<T>) -> Option<String>
where
    T: AsRef<str>,
{
    match date {
        Some(date) if date.as_ref().is_empty() => None,
        Some(date) => Some(date.as_ref().to_string()),
        None => None,
    }
}

fn handle_date_string<T>(date: T) -> String
where
    T: AsRef<str>,
{
    if date.as_ref().is_empty() {
        "1970-01-01T00:00:00Z".to_string()
    } else {
        date.as_ref().to_string()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PreDatabaseUpEnvironmentsCache {
    workdir_env: HashMap<String, String>,
    versioned_env: HashMap<String, PreDatabaseUpEnvironment>,
    history: Vec<PreDatabaseUpEnvironmentHistoryEntry>,
    updated_at: serde_json::Value, // We don't care about this value, it won't be ported to the database
}

#[derive(Debug, Serialize, Deserialize)]
struct PreDatabaseUpEnvironmentHistoryEntry {
    #[serde(rename = "wd")]
    workdir_id: String,
    #[serde(rename = "sha")]
    head_sha: String,
    #[serde(rename = "env")]
    env_version_id: String,
    #[serde(rename = "from")]
    used_from_date: String, // This is an RFC3339 date, but we will store it as a TEXT in the database
    #[serde(rename = "until", default, skip_serializing_if = "Option::is_none")]
    used_until_date: Option<String>, // This is an RFC3339 date, but we will store it as a TEXT in the database
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseUpEnvironment {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config_modtimes: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub config_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_assigned_at: String, // This is an RFC3339 date, but we will store it as a TEXT in the database
}

fn migrate_up_environments(conn: &Connection) -> Result<(), CacheManagerError> {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    let up_environments_path = cache_dir_path.join("up_environments.json");
    if !up_environments_path.exists() || up_environments_path.metadata()?.len() == 0 {
        return Ok(());
    }

    let file = std::fs::File::open(up_environments_path)?;
    let cache: PreDatabaseUpEnvironmentsCache = serde_json::from_reader(file)?;

    // Add to the env_versions table all entries from the versioned_env hash map
    for (env_id, env) in cache.versioned_env.iter() {
        // Table format:
        //  env_version_id TEXT PRIMARY KEY,
        //  versions TEXT NOT NULL,  -- JSON array of UpVersion
        //  paths TEXT NOT NULL,     -- JSON array of PathBuf
        //  env_vars TEXT NOT NULL,  -- JSON array of UpEnvVar
        //  config_modtimes TEXT NOT NULL, -- JSON object
        //  config_hash TEXT NOT NULL,
        //  last_assigned_at TEXT NOT NULL

        conn.execute(
            concat!(
                "INSERT INTO env_versions ",
                "(env_version_id, versions, paths, env_vars, config_modtimes, config_hash, last_assigned_at) ",
                "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            ),
            params![
                env_id,
                serde_json::to_string(&env.versions)?,
                serde_json::to_string(&env.paths)?,
                serde_json::to_string(&env.env_vars)?,
                serde_json::to_string(&env.config_modtimes)?,
                env.config_hash,
                handle_date_string(&env.last_assigned_at),
            ],
        )?;
    }

    // Add to the workdir_env table all entries from the workdir_env hash map
    for (workdir_id, workdir_env) in cache.workdir_env.iter() {
        // Table format:
        //  workdir_id TEXT PRIMARY KEY,
        //  env_version_id TEXT NOT NULL,

        conn.execute(
            "INSERT INTO workdir_env (workdir_id, env_version_id) VALUES (?1, ?2)",
            params![workdir_id, workdir_env],
        )?;
    }

    // Add to the env_history table all entries from the history vector
    for entry in cache.history.iter() {
        // Table format:
        //  workdir_id TEXT NOT NULL,
        //  head_sha TEXT NOT NULL,
        //  env_version_id TEXT NOT NULL,
        //  used_from_date TEXT NOT NULL
        //  used_until_date TEXT,

        conn.execute(
            concat!(
                "INSERT INTO env_history ",
                "(workdir_id, head_sha, env_version_id, used_from_date, used_until_date) ",
                "VALUES (?1, ?2, ?3, ?4, ?5)",
            ),
            params![
                entry.workdir_id,
                entry.head_sha,
                entry.env_version_id,
                handle_date_string(&entry.used_from_date),
                handle_optional_date_string(&entry.used_until_date),
            ],
        )?;
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseOmniPathCache {
    #[serde(default)]
    pub updated: serde_json::Value, // We don't care about this value, it won't be ported to the database
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub expires_at: serde_json::Value, // We don't care about this value, it won't be ported to the database
    #[serde(default)]
    pub update_error_log: String,
}

fn migrate_omnipath(conn: &Connection) -> Result<(), CacheManagerError> {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    let json_path = cache_dir_path.join("omnipath.json");
    if !json_path.exists() || json_path.metadata()?.len() == 0 {
        return Ok(());
    }

    let file = std::fs::File::open(json_path)?;
    let cache: PreDatabaseOmniPathCache = serde_json::from_reader(file)?;

    // We will dump data in the metadata table
    // Table format:
    //      key TEXT PRIMARY KEY,
    //      value TEXT

    conn.execute(
        "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
        params!["omnipath.updated_at", handle_date_string(&cache.updated_at)],
    )?;

    conn.execute(
        "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
        params!["omnipath.update_error_log", cache.update_error_log],
    )?;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PreDatabaseAsdfOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<PreDatabaseAsdfInstalled>,
    #[serde(default)]
    pub update_cache: PreDatabaseAsdfOperationUpdateCache,
    #[serde(default)]
    pub updated_at: serde_json::Value, // We don't care about this value, it won't be ported to the database
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PreDatabaseAsdfInstalled {
    #[serde(default)]
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_real_name: Option<String>,
    #[serde(default)]
    pub version: String,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default)]
    pub last_required_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PreDatabaseAsdfOperationUpdateCache {
    #[serde(default)]
    pub asdf_updated_at: String,
    #[serde(default = "HashMap::new")]
    pub plugins_updated_at: HashMap<String, String>, // Value is a datetime
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub plugins_versions: HashMap<String, PreDatabaseAsdfOperationUpdateCachePluginVersions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PreDatabaseAsdfOperationUpdateCachePluginVersions {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(default)]
    pub updated_at: String,
}

fn migrate_asdf_operation(conn: &Connection) -> Result<(), CacheManagerError> {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    let json_path = cache_dir_path.join("asdf_operation.json");
    if !json_path.exists() || json_path.metadata()?.len() == 0 {
        return Ok(());
    }

    let file = std::fs::File::open(json_path)?;
    let cache: PreDatabaseAsdfOperationCache = serde_json::from_reader(file)?;

    // asdf_installed:
    //  tool TEXT NOT NULL,
    //  tool_real_name TEXT,
    //  version TEXT NOT NULL,
    //  last_required_at TEXT NOT NULL,

    // asdf_installed_required_by:
    //  tool TEXT NOT NULL,
    //  version TEXT NOT NULL,
    //  env_version_id TEXT NOT NULL

    let mut installed_stmt = conn.prepare(concat!(
        "INSERT INTO asdf_installed ",
        "(tool, tool_real_name, version, last_required_at) ",
        "VALUES (?1, ?2, ?3, ?4)",
    ))?;
    let mut required_by_stmt = conn.prepare(concat!(
        "INSERT INTO asdf_installed_required_by ",
        "(tool, version, env_version_id) ",
        "VALUES (?1, ?2, ?3)",
    ))?;
    for installed in cache.installed.iter() {
        installed_stmt.execute(params![
            &installed.tool,
            &installed.tool_real_name,
            &installed.version,
            handle_date_string(&installed.last_required_at),
        ])?;

        // Insert the required_by data into the asdf_installed_required_by table
        for env_version_id in installed.required_by.iter() {
            if let Err(err) = required_by_stmt.execute(params![
                &installed.tool,
                &installed.version,
                &env_version_id
            ]) {
                if matches!(err, rusqlite::Error::SqliteFailure(error, _) if error.code == rusqlite::ErrorCode::ConstraintViolation)
                {
                    // Ignore constraint violation errors, it could simply be old invalid data
                } else {
                    return Err(err.into());
                }
            }
        }
    }

    // asdf update cache to be stored in metadata:
    //  key TEXT PRIMARY KEY,
    //  value TEXT

    conn.execute(
        "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
        params![
            "asdf.updated_at",
            handle_date_string(&cache.update_cache.asdf_updated_at)
        ],
    )?;

    // asdf plugins cache to be stored in asdf_plugins:
    //  plugin TEXT PRIMARY KEY,
    //  updated_at TEXT NOT NULL,
    //  versions TEXT,
    //  versions_fetched_at TEXT

    // Merge the plugin information from the plugins_versions and the plugins_updated_at
    // into a single hashmap
    struct PluginInfo {
        updated_at: Option<String>,
        versions_data: Option<PreDatabaseAsdfOperationUpdateCachePluginVersions>,
    }
    let mut plugins: HashMap<String, PluginInfo> = HashMap::new();
    for (plugin, updated_at) in cache.update_cache.plugins_updated_at.iter() {
        plugins.insert(
            plugin.clone(),
            PluginInfo {
                updated_at: Some(updated_at.clone()),
                versions_data: None,
            },
        );
    }
    for (plugin, versions_data) in cache.update_cache.plugins_versions.iter() {
        if let Some(info) = plugins.get_mut(plugin) {
            info.versions_data = Some(versions_data.clone());
        } else {
            plugins.insert(
                plugin.clone(),
                PluginInfo {
                    updated_at: None,
                    versions_data: Some(versions_data.clone()),
                },
            );
        }
    }

    // Insert the data into the database
    let mut plugin_stmt = conn.prepare(concat!(
        "INSERT INTO asdf_plugins ",
        "(plugin, updated_at, versions, versions_fetched_at) ",
        "VALUES (?1, ?2, ?3, ?4)",
    ))?;
    for (plugin, info) in plugins.iter() {
        let versions = info
            .versions_data
            .as_ref()
            .map(|v| serde_json::to_string(&v.versions).unwrap());

        plugin_stmt.execute(params![
            plugin,
            handle_date_string(&info.updated_at.as_ref().unwrap_or(&"".to_string())),
            versions,
            handle_optional_date_string(&info.versions_data.as_ref().map(|v| &v.updated_at)),
        ])?;
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PreDatabaseGithubReleaseOperationCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub installed: Vec<PreDatabaseGithubReleaseInstalled>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub releases: HashMap<String, PreDatabaseGithubReleases>,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PreDatabaseGithubReleaseInstalled {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default)]
    pub last_required_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PreDatabaseGithubReleases {
    #[serde(default = "Vec::new")]
    pub releases: Vec<PreDatabaseGithubReleaseVersion>,
    #[serde(default)]
    pub fetched_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PreDatabaseGithubReleaseVersion {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<PreDatabaseGithubReleaseAsset>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PreDatabaseGithubReleaseAsset {
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
    #[serde(default)]
    pub checksum_asset: serde_json::Value, // We don't care about this value, it won't be ported to the database
}

fn migrate_github_release_operation(conn: &Connection) -> Result<(), CacheManagerError> {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    let json_path = cache_dir_path.join("github_release_operation.json");
    if !json_path.exists() || json_path.metadata()?.len() == 0 {
        return Ok(());
    }

    let file = std::fs::File::open(json_path)?;
    let cache: PreDatabaseGithubReleaseOperationCache = serde_json::from_reader(file)?;

    // github_release_install:
    //  repository TEXT NOT NULL,
    //  version TEXT NOT NULL,
    //  last_required_at TEXT NOT NULL,

    // github_release_install_required_by:
    //   repository TEXT NOT NULL,
    //   version TEXT NOT NULL,
    //   env_version_id TEXT NOT NULL

    let mut installed_stmt = conn.prepare(concat!(
        "INSERT INTO github_release_install ",
        "(repository, version, last_required_at) ",
        "VALUES (?1, ?2, ?3)",
    ))?;

    let mut required_by_stmt = conn.prepare(concat!(
        "INSERT INTO github_release_install_required_by ",
        "(repository, version, env_version_id) ",
        "VALUES (?1, ?2, ?3)",
    ))?;

    for installed in cache.installed.iter() {
        installed_stmt.execute(params![
            &installed.repository,
            &installed.version,
            handle_date_string(&installed.last_required_at),
        ])?;

        for env_version_id in installed.required_by.iter() {
            if let Err(err) = required_by_stmt.execute(params![
                &installed.repository,
                &installed.version,
                &env_version_id,
            ]) {
                if matches!(err, rusqlite::Error::SqliteFailure(error, _) if error.code == rusqlite::ErrorCode::ConstraintViolation)
                {
                    // Ignore constraint violation errors, it could simply be old invalid data
                } else {
                    return Err(err.into());
                }
            }
        }
    }

    // For the cache of releases of a repository, github_releases:
    //  repository TEXT PRIMARY KEY,
    //  releases TEXT NOT NULL,  -- JSON array of GithubReleaseVersion
    //  fetched_at TEXT NOT NULL

    let mut releases_stmt = conn.prepare(concat!(
        "INSERT INTO github_releases ",
        "(repository, releases, fetched_at) ",
        "VALUES (?1, ?2, ?3)",
    ))?;

    for (repository, releases) in cache.releases.iter() {
        releases_stmt.execute(params![
            repository,
            serde_json::to_string(&releases.releases)?,
            handle_date_string(&releases.fetched_at),
        ])?;
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseHomebrewOperationCache {
    #[serde(default = "Vec::new")]
    pub installed: Vec<PreDatabaseHomebrewInstalled>,
    #[serde(default = "Vec::new")]
    pub tapped: Vec<PreDatabaseHomebrewTapped>,
    #[serde(default)]
    pub update_cache: Option<PreDatabaseHomebrewOperationUpdateCache>,
    #[serde(default)]
    pub updated_at: serde_json::Value, // We don't care about this value, it won't be ported to the database
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseHomebrewInstalled {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub cask: bool,
    #[serde(default)]
    pub installed: bool,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default)]
    pub last_required_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseHomebrewTapped {
    pub name: String,
    #[serde(default)]
    pub tapped: bool,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub required_by: HashSet<String>,
    #[serde(default)]
    pub last_required_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseHomebrewOperationUpdateCache {
    #[serde(default)]
    pub homebrew: PreDatabaseHomebrewOperationUpdateCacheHomebrew,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub install: HashMap<String, PreDatabaseHomebrewOperationUpdateCacheInstall>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub tap: HashMap<String, PreDatabaseHomebrewOperationUpdateCacheTap>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PreDatabaseHomebrewOperationUpdateCacheHomebrew {
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub bin_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseHomebrewOperationUpdateCacheInstall {
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_paths: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseHomebrewOperationUpdateCacheTap {
    #[serde(default)]
    pub updated_at: String,
}

fn migrate_homebrew_operation(conn: &Connection) -> Result<(), CacheManagerError> {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    let json_path = cache_dir_path.join("homebrew_operation.json");
    if !json_path.exists() || json_path.metadata()?.len() == 0 {
        return Ok(());
    }

    let file = std::fs::File::open(json_path)?;
    let cache: PreDatabaseHomebrewOperationCache = serde_json::from_reader(file)?;

    // homebrew_install:
    //  name TEXT NOT NULL,
    //  version TEXT NOT NULL DEFAULT "__NULL__",
    //  cask BOOLEAN NOT NULL DEFAULT 0,
    //  installed BOOLEAN NOT NULL DEFAULT 0,
    //  last_required_at TEXT NOT NULL,

    // homebrew_install_required_by:
    //  name TEXT NOT NULL,
    //  version TEXT NOT NULL DEFAULT "__NULL__",
    //  cask BOOLEAN NOT NULL DEFAULT 0,
    //  env_version_id TEXT NOT NULL

    let mut installed_stmt = conn.prepare(concat!(
        "INSERT INTO homebrew_install ",
        "(name, version, cask, installed, last_required_at) ",
        "VALUES (?1, COALESCE(?2, '__NULL__'), ?3, ?4, ?5)",
    ))?;

    let mut installed_required_by_stmt = conn.prepare(concat!(
        "INSERT INTO homebrew_install_required_by ",
        "(name, version, cask, env_version_id) ",
        "VALUES (?1, COALESCE(?2, '__NULL__'), ?3, ?4)",
    ))?;

    for installed in cache.installed.iter() {
        installed_stmt.execute(params![
            &installed.name,
            &installed.version,
            &installed.cask,
            &installed.installed,
            &installed.last_required_at
        ])?;

        for env_version_id in installed.required_by.iter() {
            if let Err(err) = installed_required_by_stmt.execute(params![
                &installed.name,
                &installed.version,
                &installed.cask,
                &env_version_id,
            ]) {
                if matches!(err, rusqlite::Error::SqliteFailure(error, _) if error.code == rusqlite::ErrorCode::ConstraintViolation)
                {
                    // Ignore constraint violation errors, it could simply be old invalid data
                } else {
                    return Err(err.into());
                }
            }
        }
    }

    // homebrew_tap:
    //  name TEXT PRIMARY KEY,
    //  tapped BOOLEAN NOT NULL DEFAULT 0,
    //  last_required_at TEXT NOT NULL

    // homebrew_tap_required_by:
    //   name TEXT NOT NULL,
    //   env_version_id TEXT NOT NULL

    let mut tapped_stmt = conn.prepare(concat!(
        "INSERT INTO homebrew_tap ",
        "(name, tapped, last_required_at) ",
        "VALUES (?1, ?2, ?3)",
    ))?;

    let mut tapped_required_by_stmt = conn.prepare(concat!(
        "INSERT INTO homebrew_tap_required_by ",
        "(name, env_version_id) ",
        "VALUES (?1, ?2)",
    ))?;

    for tapped in cache.tapped.iter() {
        tapped_stmt.execute(params![
            &tapped.name,
            &tapped.tapped,
            handle_date_string(&tapped.last_required_at.clone().unwrap_or("".to_string())),
        ])?;

        for env_version_id in tapped.required_by.iter() {
            if let Err(err) =
                tapped_required_by_stmt.execute(params![&tapped.name, &env_version_id])
            {
                if matches!(err, rusqlite::Error::SqliteFailure(error, _) if error.code == rusqlite::ErrorCode::ConstraintViolation)
                {
                    // Ignore constraint violation errors, it could simply be old invalid data
                } else {
                    return Err(err.into());
                }
            }
        }
    }

    // For the homebrew update cache, two keys (homebrew.bin_path and homebrew.updated_at) metadata:
    //   key TEXT PRIMARY KEY,
    //   value TEXT

    if let Some(homebrew) = cache.update_cache.as_ref().map(|c| &c.homebrew) {
        if let Some(bin_path) = &homebrew.bin_path {
            conn.execute(
                "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
                params!["homebrew.bin_path", bin_path],
            )?;
        }

        if let Some(updated_at) = &homebrew.updated_at {
            conn.execute(
                "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
                params!["homebrew.updated_at", handle_date_string(updated_at)],
            )?;
        }
    }

    let mut install_cache_stmt = conn.prepare(concat!(
        "INSERT INTO homebrew_install ",
        "(name, version, cask, updated_at, checked_at, bin_paths) ",
        "VALUES (?1, ?2, MIN(1, ?3), ?4, ?5, ?6) ",
        "ON CONFLICT(name, version, cask) DO UPDATE SET ",
        "updated_at = ?4, checked_at = ?5, bin_paths = ?6 ",
        "WHERE name = ?1 AND version = ?2 AND cask = MIN(1, ?3)",
    ))?;

    for (install_key, install) in cache
        .update_cache
        .as_ref()
        .map(|c| &c.install)
        .unwrap_or(&HashMap::new())
    {
        // Parse an install key:
        //   <cask|formula>:<name>[@<version>]

        let parts: Vec<&str> = install_key.split(':').collect();
        if parts.len() != 2 {
            continue;
        }

        let install_cask = parts[0] == "cask";
        let parts: Vec<&str> = parts[1].split('@').collect();

        let (install_name, install_version) = match parts.len() {
            1 => (parts[0], None),
            2 => (parts[0], Some(parts[1])),
            _ => continue,
        };

        install_cache_stmt.execute(params![
            install_name,
            install_version,
            install_cask,
            handle_date_string(&install.updated_at),
            handle_date_string(&install.checked_at),
            serde_json::to_string(&install.bin_paths)?
        ])?;
    }

    let mut tap_cache_stmt = conn.prepare(concat!(
        "INSERT INTO homebrew_tap ",
        "(name, updated_at) ",
        "VALUES (?1, ?2) ",
        "ON CONFLICT(name) DO UPDATE ",
        "SET updated_at = ?2",
        "WHERE name = ?1",
    ))?;

    for (tap_name, tap) in cache
        .update_cache
        .as_ref()
        .map(|c| &c.tap)
        .unwrap_or(&HashMap::new())
    {
        tap_cache_stmt.execute(params![tap_name, handle_date_string(&tap.updated_at)])?;
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabasePromptsCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<PreDatabasePromptAnswer>,
    #[serde(default)]
    pub updated_at: serde_json::Value, // We don't care about this value, it won't be ported to the database
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabasePromptAnswer {
    pub id: String,
    pub org: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    pub answer: serde_yaml::Value,
}

fn migrate_prompts(conn: &Connection) -> Result<(), CacheManagerError> {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    let json_path = cache_dir_path.join("prompts.json");
    if !json_path.exists() || json_path.metadata()?.len() == 0 {
        return Ok(());
    }

    let file = std::fs::File::open(json_path)?;
    let cache: PreDatabasePromptsCache = serde_json::from_reader(file)?;

    // prompts:
    //  prompt_id TEXT,
    //  organization TEXT NOT NULL,
    //  repository TEXT,
    //  answer TEXT,

    let mut answer_stmt = conn.prepare(concat!(
        "INSERT INTO prompts ",
        "(prompt_id, organization, repository, answer) ",
        "VALUES (?1, ?2, ?3, ?4)",
    ))?;

    for answer in cache.answers.iter() {
        answer_stmt.execute(params![
            &answer.id,
            &answer.org,
            &answer.repo,
            serde_json::to_string(&answer.answer)?
        ])?;
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseRepositoriesCache {
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub trusted: HashSet<String>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub fingerprints: HashMap<String, PreDatabaseRepositoryFingerprints>,
    #[serde(default)]
    pub updated_at: serde_json::Value, // We don't care about this value, it won't be ported to the database
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PreDatabaseRepositoryFingerprints {
    #[serde(
        default = "HashMap::new",
        skip_serializing_if = "HashMap::is_empty",
        flatten
    )]
    fingerprints: HashMap<String, serde_json::Value>,
}

fn migrate_repositories(conn: &Connection) -> Result<(), CacheManagerError> {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    let json_path = cache_dir_path.join("repositories.json");
    if !json_path.exists() || json_path.metadata()?.len() == 0 {
        return Ok(());
    }

    let file = std::fs::File::open(json_path)?;
    let cache: PreDatabaseRepositoriesCache = serde_json::from_reader(file)?;

    // workdir_trusted:
    //  workdir_id TEXT PRIMARY KEY

    // workdir_fingerprints:
    //  workdir_id TEXT NOT NULL,
    //  fingerprint_type TEXT NOT NULL,
    //  fingerprint TEXT NOT NULL

    let mut trusted_stmt = conn.prepare(concat!(
        "INSERT INTO workdir_trusted ",
        "(workdir_id) ",
        "VALUES (?1)",
    ))?;

    for trusted in cache.trusted.iter() {
        trusted_stmt.execute(params![trusted])?;
    }

    let mut fingerprints_stmt = conn.prepare(concat!(
        "INSERT INTO workdir_fingerprints ",
        "(workdir_id, fingerprint_type, fingerprint) ",
        "VALUES (?1, ?2, ?3)",
    ))?;

    for (workdir_id, fingerprints) in cache.fingerprints.iter() {
        for (fingerprint_type, fingerprint) in fingerprints.fingerprints.iter() {
            fingerprints_stmt.execute(params![
                &workdir_id,
                &fingerprint_type,
                serde_json::to_string(&fingerprint)?
            ])?;
        }
    }

    Ok(())
}
