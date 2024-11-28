-- Get the go versions cached for a given import path
-- :param: ?1 import_path - the import path used with 'go install'
SELECT
    versions,
    fetched_at
FROM
    go_versions
WHERE
    import_path = ?1;
