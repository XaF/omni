-- Add a new installed tool to the database
-- :param1: tool - the name of the tool
-- :param2: tool_real_name - the real name of the tool
-- :param3: version - the version of the tool
INSERT INTO mise_installed (
    tool,
    tool_real_name,
    version,
    last_required_at
)
VALUES (
    ?1,
    ?2,
    ?3,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (tool, version) DO UPDATE
SET last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE tool = ?1 AND version = ?3;
