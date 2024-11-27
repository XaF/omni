-- Cache github releases for a given repository
-- :param: ?1 repository - the repository
-- :param: ?2 releases - JSON array of GithubReleaseVersion
INSERT INTO github_releases (
    repository,
    releases,
    fetched_at
)
VALUES (
    ?1,
    ?2,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (repository) DO UPDATE
SET
    releases = ?2,
    fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    repository = ?1;
