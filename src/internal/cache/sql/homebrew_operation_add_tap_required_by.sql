-- Add a new tap requirement to the database
-- :param1: name - the name of the tap
-- :param2: env_version_id - the id of the environment version that is requiring the tool
INSERT INTO homebrew_tapped_required_by (
    name,
    env_version_id
)
VALUES (
    ?1,
    ?2
)
ON CONFLICT (name, env_version_id) DO NOTHING;
