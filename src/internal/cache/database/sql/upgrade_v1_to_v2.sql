-- Start a transaction
BEGIN TRANSACTION;

-- Create new tables
CREATE TABLE IF NOT EXISTS mise_installed (
    -- The name of the tool, e.g. python, ripgrep, etc.
    tool TEXT NOT NULL COLLATE NOCASE,
    -- The name used for the plugin, including the backend,
    -- so that operations can be done to the plugin such as uninstall
    -- e.g. python, aqua:BurntSushi/ripgrep, etc.
    plugin_name TEXT NOT NULL COLLATE NOCASE,
    -- The normalized name for the tool path so it can be imported
    -- in the path when loading the dynamic environment
    -- e.g. python, aqua-burnt-sushi-ripgrep, etc.
    normalized_name TEXT NOT NULL COLLATE NOCASE,
    -- The version of the tool that was installed
    version TEXT NOT NULL,
    -- Relative paths to the bin directories for the tool
    bin_paths TEXT,
    -- Last time the tool was required in an `omni up` operation
    last_required_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z',
    PRIMARY KEY (normalized_name, version)
);

CREATE TABLE IF NOT EXISTS mise_installed_required_by (
    normalized_name TEXT NOT NULL COLLATE NOCASE,
    version TEXT NOT NULL,
    env_version_id TEXT NOT NULL,
    PRIMARY KEY (normalized_name, version, env_version_id),
    FOREIGN KEY(normalized_name, version) REFERENCES mise_installed(normalized_name, version) ON DELETE CASCADE,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS mise_plugins (
    plugin_name TEXT PRIMARY KEY COLLATE NOCASE,
    updated_at TEXT NOT NULL,
    versions TEXT,
    versions_fetched_at TEXT
);

-- Copy the data from asdf_installed to mise_installed
INSERT INTO mise_installed (tool, plugin_name, normalized_name, version, bin_paths)
SELECT
    CASE tool_real_name IS NULL
        WHEN 1 THEN tool
        ELSE tool_real_name
    END,
    tool,
    tool,
    version,
    NULL
FROM asdf_installed;

-- Rename 'golang' (used by asdf) to 'go' (used by mise)
UPDATE mise_installed SET tool = 'go' WHERE tool = 'golang';
UPDATE mise_installed SET plugin_name = 'go' WHERE plugin_name = 'golang';
UPDATE mise_installed SET normalized_name = 'go' WHERE normalized_name = 'golang';

-- Rename 'nodejs' (used by asdf) to 'node' (used by mise)
UPDATE mise_installed SET tool = 'node' WHERE tool = 'nodejs';
UPDATE mise_installed SET plugin_name = 'node' WHERE plugin_name = 'nodejs';
UPDATE mise_installed SET normalized_name = 'node' WHERE normalized_name = 'nodejs';

-- Copy the data from asdf_installed_required_by to mise_installed_required_by
INSERT INTO mise_installed_required_by (normalized_name, version, env_version_id)
SELECT
    CASE tool
        -- Rename 'golang' (used by asdf) to 'go' (used by mise)
        WHEN 'golang' THEN 'go'
        -- Rename 'nodejs' (used by asdf) to 'node' (used by mise)
        WHEN 'nodejs' THEN 'node'
        ELSE tool
    END,
    version,
    env_version_id
FROM asdf_installed_required_by;

-- Copy the data from asdf_plugins to mise_plugins
INSERT INTO mise_plugins (plugin_name, updated_at, versions, versions_fetched_at)
SELECT plugin, updated_at, versions, versions_fetched_at
FROM asdf_plugins;

-- Delete the old asdf tables
DROP INDEX IF EXISTS idx_asdf_installed_required_by;
DROP TABLE asdf_installed;
DROP TABLE asdf_installed_required_by;
DROP TABLE asdf_plugins;

-- Delete mentions of `asdf` in the `metadata` table
DELETE FROM metadata
WHERE key = 'asdf.updated_at';

-- Create new indexes
CREATE INDEX IF NOT EXISTS idx_mise_installed_required_by_normalized_name_version ON mise_installed_required_by(normalized_name, version);
CREATE INDEX IF NOT EXISTS idx_mise_installed_plugin_name ON mise_installed(plugin_name);

-- Update the user_version to 2
PRAGMA user_version = 2;

-- Commit the transaction
COMMIT;
