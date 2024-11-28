-- Add a new go-installed tool
-- :param: ?1 import_path - the import path used with 'go install'
-- :param: ?2 version - the version of the tool
INSERT INTO go_installed (
    import_path,
    version,
    last_required_at
)
VALUES (
    ?1,
    ?2,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (import_path, version) DO UPDATE
SET
    last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    import_path = ?1
    AND version = ?2;
