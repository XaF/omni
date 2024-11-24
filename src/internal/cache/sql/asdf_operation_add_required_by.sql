-- Add a new installed tool to the database
-- :param1: tool - the name of the tool
-- :param2: tool_real_name - the real name of the tool
-- :param3: version - the version of the tool
-- :param4: required_by - the JSON array of environments that require this tool
INSERT INTO asdf_installed (
    tool,
    tool_real_name,
    version,
    required_by,
    last_required_at
)
VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (tool, version) DO UPDATE
SET
    required_by = ?4,
    last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE tool = ?1 AND version = ?3;
