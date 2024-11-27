-- Get the open environment history for a workdir
-- :param: ?1 - workdir_id
SELECT
    env_version_id,
    head_sha
FROM
    env_history
WHERE
    workdir_id = ?1
    AND used_until_date IS NULL
