-- Cache cargo versions for a given crate
-- :param: ?1 crate - the crate used with 'cargo install'
-- :param: ?2 versions - JSON array of String
INSERT INTO cargo_versions (
    crate,
    versions,
    fetched_at
)
VALUES (
    ?1,
    ?2,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (crate) DO UPDATE
SET
    versions = ?2,
    fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    crate = ?1;
