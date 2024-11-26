-- Add a new github release
-- :param: ?1 repository - the repository name
-- :param: ?2 version - the version of the release
INSERT INTO github_release_install (
    repository,
    version,
    last_required_at
)
VALUES (
    ?1,
    ?2,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (repository, version) DO UPDATE
SET
    last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    repository = ?1
    AND version = ?2;
