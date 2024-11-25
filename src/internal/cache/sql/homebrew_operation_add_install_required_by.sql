-- Add a new formula/cask requirement to the database
-- :param1: name - the name of the tap
-- :param2: version - the version of the formula or cask if any specified
-- :param3: cask - whether the formula is a cask or not
-- :param4: env_version_id - the id of the environment version that is requiring the tool
INSERT INTO homebrew_tapped_required_by (
    name,
    version,
    cask,
    env_version_id,
)
VALUES (
    ?1,
    ?2,
    MIN(1, ?3),
    ?4
)
ON CONFLICT (name, version, cask, env_version_id) DO NOTHING;
