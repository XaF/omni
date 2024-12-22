-- First disable foreign key checks
PRAGMA foreign_keys = OFF;

-- Start a transaction
BEGIN TRANSACTION;

-- Delete asdf tables indexes
DROP INDEX IF EXISTS idx_asdf_installed_required_by;

-- Rename Go to Golang in asdf_installed_required_by
UPDATE asdf_installed_required_by
SET tool = 'go'
WHERE tool = 'golang';

-- Rename Go to Golang in asdf_installed
UPDATE asdf_installed
SET tool = 'go'
WHERE tool = 'golang';

-- Rename asdf_installed to mise_installed
ALTER TABLE asdf_installed
RENAME TO mise_installed;

-- Rename asdf_installed_required_by to mise_installed_required_by,
-- except that we want to keep the foreign key constraints and that
-- sqlite does not support renaming foreign keys, so we need to
-- create a new table and copy the data over
CREATE TABLE IF NOT EXISTS mise_installed_required_by (
    tool TEXT NOT NULL COLLATE NOCASE,
    version TEXT NOT NULL,
    env_version_id TEXT NOT NULL,
    PRIMARY KEY (tool, version, env_version_id),
    FOREIGN KEY(tool, version) REFERENCES mise_installed(tool, version) ON DELETE CASCADE,
    FOREIGN KEY(env_version_id) REFERENCES env_versions(env_version_id) ON DELETE CASCADE
);

-- Copy the data from asdf_installed_required_by to mise_installed_required_by
INSERT INTO mise_installed_required_by (tool, version, env_version_id)
SELECT tool, version, env_version_id
FROM asdf_installed_required_by;

-- Drop the old asdf_installed_required_by table
DROP TABLE asdf_installed_required_by;

-- Rename asdf_plugins to mise_plugins
ALTER TABLE asdf_plugins
RENAME TO mise_plugins;

-- Create new indexes
CREATE INDEX IF NOT EXISTS idx_mise_installed_required_by
ON mise_installed_required_by(tool, version);

-- Delete mentions to `asdf` in the `metadata` table
DELETE FROM metadata
WHERE key = 'asdf.updated_at';

-- Update the user_version to 2
PRAGMA user_version = 2;

-- Commit the transaction
COMMIT;

-- Re-enable foreign key checks
PRAGMA foreign_keys = ON;
