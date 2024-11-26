-- Add a github release requirement
-- :param: ?1 repository - the repository name
-- :param: ?2 version - the version of the release
-- :param: ?3 env_version_id - the id of the environment version that is requiring the tool
INSERT INTO github_release_install_required_by (
    repository,
    version,
    env_version_id
)
VALUES (
    ?1,
    ?2,
    ?3
)
ON CONFLICT (repository, version, env_version_id) DO NOTHING;
