-- Add a new installed tool to the database
-- :param1: normalized_name - the name of the tool
-- :param2: version - the version of the tool
-- :param3: env_version_id - the id of the environment version that is requiring the tool
INSERT INTO mise_installed_required_by (
    normalized_name,
    version,
    env_version_id
)
VALUES (
    ?1,
    ?2,
    ?3
)
ON CONFLICT (normalized_name, version, env_version_id) DO NOTHING;
