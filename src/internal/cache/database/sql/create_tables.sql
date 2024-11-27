PRAGMA user_version = 1;

-- Basic key-value data not requiring individual tables
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY COLLATE NOCASE,
    value TEXT
);

-- Table containing the environment versions that the work directories
-- are currently using
CREATE TABLE IF NOT EXISTS workdir_env (
    workdir_id TEXT PRIMARY KEY COLLATE NOCASE,
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
    last_assigned_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z'
);

-- Table containing the history of the work directories environments,
-- including the dynamic environment they were using at that time, and
-- the commit hash that was checked out
CREATE TABLE IF NOT EXISTS env_history (
    env_history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    workdir_id TEXT NOT NULL COLLATE NOCASE,
    env_version_id TEXT NOT NULL,
    head_sha TEXT,
    used_from_date TEXT NOT NULL,
    used_until_date TEXT,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id)
);

-- Table containing the tools that were installed using asdf
-- and the versions that were installed
CREATE TABLE IF NOT EXISTS asdf_installed (
    tool TEXT NOT NULL COLLATE NOCASE,
    tool_real_name TEXT COLLATE NOCASE,
    version TEXT NOT NULL,
    last_required_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z',
    PRIMARY KEY (tool, version)
);

-- Table containing the information of which workdir is
-- requiring a given asdf tool
CREATE TABLE IF NOT EXISTS asdf_installed_required_by (
    tool TEXT NOT NULL COLLATE NOCASE,
    version TEXT NOT NULL,
    env_version_id TEXT NOT NULL,
    PRIMARY KEY (tool, version, env_version_id),
    FOREIGN KEY(tool, version) REFERENCES asdf_installed(tool, version) ON DELETE CASCADE,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id) ON DELETE CASCADE
);

-- Table containing the cache of asdf plugins and when they have been updated
CREATE TABLE IF NOT EXISTS asdf_plugins (
    plugin TEXT PRIMARY KEY COLLATE NOCASE,
    updated_at TEXT NOT NULL,
    versions TEXT,
    versions_fetched_at TEXT
);

-- Table containing the tools that were installed using Github releases
-- and the versions that were installed
CREATE TABLE IF NOT EXISTS github_release_installed (
    repository TEXT NOT NULL COLLATE NOCASE,
    version TEXT NOT NULL COLLATE NOCASE,
    last_required_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z',
    PRIMARY KEY (repository, version)
);

-- Table containing the information of which workdir is
-- requiring a given Github release
CREATE TABLE IF NOT EXISTS github_release_required_by (
    repository TEXT NOT NULL COLLATE NOCASE,
    version TEXT NOT NULL COLLATE NOCASE,
    env_version_id TEXT NOT NULL,
    PRIMARY KEY (repository, version, env_version_id),
    FOREIGN KEY(repository, version) REFERENCES github_release_installed(repository, version) ON DELETE CASCADE,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id) ON DELETE CASCADE
);

-- Table containing the cache of Github releases per repository
CREATE TABLE IF NOT EXISTS github_releases (
    repository TEXT PRIMARY KEY COLLATE NOCASE,
    releases TEXT NOT NULL,  -- JSON array of GithubReleaseVersion
    fetched_at TEXT NOT NULL
);

-- Table containing the formulae and casks that were installed using Homebrew
CREATE TABLE IF NOT EXISTS homebrew_install (
    name TEXT NOT NULL COLLATE NOCASE,
    version TEXT NOT NULL DEFAULT '__NULL__',
    cask BOOLEAN NOT NULL DEFAULT 0,
    installed BOOLEAN NOT NULL DEFAULT 0,
    last_required_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z',
    updated_at TEXT,
    checked_at TEXT,
    bin_paths TEXT,  -- JSON array
    PRIMARY KEY (name, version, cask)
);

-- Table containing the information of which workdir is
-- requiring a given Homebrew formula or cask
CREATE TABLE IF NOT EXISTS homebrew_install_required_by (
    name TEXT NOT NULL COLLATE NOCASE,
    version TEXT NOT NULL DEFAULT '__NULL__',
    cask BOOLEAN NOT NULL DEFAULT 0,
    env_version_id TEXT NOT NULL,
    PRIMARY KEY (name, version, cask, env_version_id),
    FOREIGN KEY(name, version, cask) REFERENCES homebrew_install(name, version, cask) ON DELETE CASCADE,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id) ON DELETE CASCADE
);

-- Table containing the taps that were tapped using Homebrew
CREATE TABLE IF NOT EXISTS homebrew_tap (
    name TEXT PRIMARY KEY COLLATE NOCASE,
    tapped BOOLEAN NOT NULL DEFAULT 0,
    last_required_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z',
    updated_at TEXT
);

-- Table containing the information of which workdir is
-- requiring a given Homebrew tap
CREATE TABLE IF NOT EXISTS homebrew_tap_required_by (
    name TEXT NOT NULL COLLATE NOCASE,
    env_version_id TEXT NOT NULL,
    PRIMARY KEY (name, env_version_id),
    FOREIGN KEY(name) REFERENCES homebrew_tap(name) ON DELETE CASCADE,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id) ON DELETE CASCADE
);

-- Table containing the cache of the prompts and their answers per organization
-- and per repository; NOTE: how does this work for a workdir id?
CREATE TABLE IF NOT EXISTS prompts (
    prompt_id TEXT NOT NULL COLLATE NOCASE,
    organization TEXT NOT NULL COLLATE NOCASE,
    repository TEXT NOT NULL DEFAULT '__NULL__' COLLATE NOCASE,
    answer TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z',
    PRIMARY KEY (prompt_id, organization, repository)
);

-- Table containing the trusted work directories
CREATE TABLE IF NOT EXISTS workdir_trusted (
    workdir_id TEXT PRIMARY KEY COLLATE NOCASE
);

-- Table containing the fingerprints of the work directories
CREATE TABLE IF NOT EXISTS workdir_fingerprints (
    workdir_id TEXT NOT NULL COLLATE NOCASE,
    fingerprint_type TEXT NOT NULL COLLATE NOCASE,
    fingerprint TEXT NOT NULL,
    PRIMARY KEY (workdir_id, fingerprint_type)
);

-- Add indexes for frequently queried columns
CREATE INDEX IF NOT EXISTS idx_workdir_env_env_version_id ON workdir_env(env_version_id);
--  CREATE INDEX IF NOT EXISTS idx_env_versions_assigned ON env_versions(last_assigned_at);
CREATE INDEX IF NOT EXISTS idx_env_history_workdir ON env_history(workdir_id);
CREATE INDEX IF NOT EXISTS idx_env_history_env_version_id ON env_history(env_version_id);
CREATE INDEX IF NOT EXISTS idx_asdf_installed_required_by ON asdf_installed_required_by(tool, version);
--  CREATE INDEX IF NOT EXISTS idx_asdf_installed_required ON asdf_installed(last_required_at);
CREATE INDEX IF NOT EXISTS idx_github_installed_required_by ON github_release_required_by(repository, version);
--  CREATE INDEX IF NOT EXISTS idx_github_installed_required ON github_release_install(last_required_at);
CREATE INDEX IF NOT EXISTS idx_homebrew_install_required_by ON homebrew_install_required_by(name, version, cask);
CREATE INDEX IF NOT EXISTS idx_homebrew_tap_required_by ON homebrew_tap(name);
--  CREATE INDEX IF NOT EXISTS idx_homebrew_install_required ON homebrew_install(last_required_at);
CREATE INDEX IF NOT EXISTS idx_prompts_organization ON prompts(organization);
CREATE INDEX IF NOT EXISTS idx_prompts_repository ON prompts(organization, repository);
