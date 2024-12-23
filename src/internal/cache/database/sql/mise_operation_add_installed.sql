-- Add a new installed tool to the database
-- :param1: tool - the name of the tool
-- :param2: plugin_name - the full name of the plugin
-- :param3: normalized_name - the normalized name for that install
-- :param4: version - the installed version
-- :param5: bin_paths - the paths to the bin directories
INSERT INTO mise_installed (
    tool,
    plugin_name,
    normalized_name,
    version,
    bin_paths,
    last_required_at
)
VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (normalized_name, version) DO UPDATE
SET last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE normalized_name = ?3 AND version = ?4;
