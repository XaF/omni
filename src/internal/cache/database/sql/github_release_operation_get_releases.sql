-- Get the github releases cached for a given repository
-- :param: ?1 repository
SELECT
    releases,
    fetched_at
FROM
    github_releases
WHERE
    repository = ?1;
