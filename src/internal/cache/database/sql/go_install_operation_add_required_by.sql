-- Add required_by relationship for go-install tool
-- :param: ?1 import_path - the import path used with 'go install'
-- :param: ?2 version - the version of the tool
-- :param: ?3 env_version_id - the id of the environment version that is requiring the tool
INSERT INTO go_install_required_by (
    import_path,
    version,
    env_version_id
)
VALUES (
    ?1,
    ?2,
    ?3
)
ON CONFLICT (import_path, version, env_version_id) DO NOTHING;
