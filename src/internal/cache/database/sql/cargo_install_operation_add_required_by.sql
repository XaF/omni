-- Add required_by relationship for cargo-install tool
-- :param: ?1 crate - the crate used with 'cargo install'
-- :param: ?2 version - the version of the tool
-- :param: ?3 env_version_id - the id of the environment version that is requiring the tool
INSERT INTO cargo_install_required_by (
    crate,
    version,
    env_version_id
)
VALUES (
    ?1,
    ?2,
    ?3
)
ON CONFLICT (crate, version, env_version_id) DO NOTHING;
