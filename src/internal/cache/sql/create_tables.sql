PRAGMA user_version = 1;

-- Basic key-value data not requiring individual tables
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT
);

-- Table containing the environment versions that the work directories
-- are currently using
CREATE TABLE IF NOT EXISTS workdir_env (
    workdir_id TEXT PRIMARY KEY,
    env_version_id TEXT NOT NULL,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id)
);

-- Table containing the versions of the environment
CREATE TABLE IF NOT EXISTS env_versions (
    env_version_id TEXT PRIMARY KEY,
    versions TEXT NOT NULL,  -- JSON array of UpVersion
    paths TEXT NOT NULL,     -- JSON array of PathBuf
    env_vars TEXT NOT NULL,  -- JSON array of UpEnvVar
    config_modtimes TEXT NOT NULL, -- JSON object
    config_hash TEXT NOT NULL,
    last_assigned_at TEXT NOT NULL
);

-- Table containing the history of the work directories environments,
-- including the dynamic environment they were using at that time, and
-- the commit hash that was checked out
CREATE TABLE IF NOT EXISTS env_history (
    env_history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    workdir_id TEXT NOT NULL,
    env_version_id TEXT NOT NULL,
    head_sha TEXT,
    used_from_date TEXT NOT NULL,
    used_until_date TEXT,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id)
);

-- Table containing the tools that were installed using asdf
-- and the versions that were installed
CREATE TABLE IF NOT EXISTS asdf_installed (
    tool TEXT NOT NULL,
    tool_real_name TEXT,
    version TEXT NOT NULL,
    required_by TEXT NOT NULL, -- JSON array
    last_required_at TEXT NOT NULL,
    PRIMARY KEY (tool, version)
);

-- Table containing the cache of asdf plugins and when they have been updated
CREATE TABLE IF NOT EXISTS asdf_plugins (
    plugin TEXT PRIMARY KEY,
    updated_at TEXT NOT NULL,
    versions TEXT,
    versions_fetched_at TEXT
);

-- Table containing the tools that were installed using Github releases
-- and the versions that were installed
CREATE TABLE IF NOT EXISTS github_release_installed (
    repository TEXT NOT NULL,
    version TEXT NOT NULL,
    required_by TEXT NOT NULL, -- JSON array
    last_required_at TEXT NOT NULL,
    PRIMARY KEY (repository, version)
);

-- Table containing the cache of Github releases per repository
CREATE TABLE IF NOT EXISTS github_releases (
    repository TEXT PRIMARY KEY,
    releases TEXT NOT NULL,  -- JSON array of GithubReleaseVersion
    fetched_at TEXT NOT NULL
);

-- Table containing the formulae and casks that were installed using Homebrew
CREATE TABLE IF NOT EXISTS homebrew_installed (
    name TEXT NOT NULL,
    version TEXT,
    cask BOOLEAN NOT NULL DEFAULT 0,
    installed BOOLEAN NOT NULL DEFAULT 0,
    required_by TEXT NOT NULL, -- JSON array
    last_required_at TEXT NOT NULL,
    PRIMARY KEY (name, version, cask)
);

-- Table containing the taps that were tapped using Homebrew
CREATE TABLE IF NOT EXISTS homebrew_tapped (
    name TEXT PRIMARY KEY,
    tapped BOOLEAN NOT NULL DEFAULT 0,
    required_by TEXT NOT NULL, -- JSON array
    last_required_at TEXT NOT NULL
);

-- Table containing the cache of Homebrew formulae and casks and when they
-- have been updated / checked the last time
CREATE TABLE IF NOT EXISTS homebrew_install_cache (
    install_key TEXT PRIMARY KEY,
    updated_at TEXT NOT NULL,
    checked_at TEXT NOT NULL,
    bin_paths TEXT  -- JSON array
);

-- Table containing the cache of Homebrew taps and when they have been updated
CREATE TABLE IF NOT EXISTS homebrew_tap_cache (
    tap_name TEXT PRIMARY KEY,
    updated_at TEXT NOT NULL
);

-- Table containing the cache of the prompts and their answers per organization
-- and per repository; NOTE: how does this work for a workdir id?
CREATE TABLE IF NOT EXISTS prompts (
    prompt_id TEXT,
    organization TEXT NOT NULL,
    repository TEXT,
    answer TEXT,
    PRIMARY KEY (prompt_id, organization, repository)
);

-- Table containing the trusted work directories
CREATE TABLE IF NOT EXISTS workdir_trusted (
    workdir TEXT PRIMARY KEY
);

-- Table containing the fingerprints of the work directories
CREATE TABLE IF NOT EXISTS workdir_fingerprints (
    workdir_id TEXT NOT NULL,
    fingerprint_type TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    PRIMARY KEY (workdir_id, fingerprint_type)
);

-- Add indexes for frequently queried columns
CREATE INDEX IF NOT EXISTS idx_workdir_env_env_version_id ON workdir_env(env_version_id);
--  CREATE INDEX IF NOT EXISTS idx_env_versions_assigned ON env_versions(last_assigned_at);
CREATE INDEX IF NOT EXISTS idx_env_history_workdir ON env_history(workdir_id);
CREATE INDEX IF NOT EXISTS idx_env_history_env_version_id ON env_history(env_version_id);
--  CREATE INDEX IF NOT EXISTS idx_asdf_installed_required ON asdf_installed(last_required_at);
--  CREATE INDEX IF NOT EXISTS idx_github_installed_required ON github_release_installed(last_required_at);
--  CREATE INDEX IF NOT EXISTS idx_homebrew_installed_required ON homebrew_installed(last_required_at);
CREATE INDEX IF NOT EXISTS idx_prompts_organization ON prompts(organization);
CREATE INDEX IF NOT EXISTS idx_prompts_repository ON prompts(organization, repository);
