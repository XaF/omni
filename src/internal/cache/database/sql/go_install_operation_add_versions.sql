-- Cache go versions for a given import path
-- :param: ?1 import_path - the import path used with 'go install'
-- :param: ?2 versions - JSON array of String
INSERT INTO go_versions (
    import_path,
    versions,
    fetched_at
)
VALUES (
    ?1,
    ?2,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (import_path) DO UPDATE
SET
    versions = ?2,
    fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    import_path = ?1;
