-- Add a new cargo-installed tool
-- :param: ?1 crate - the crate used with 'cargo install'
-- :param: ?2 version - the version of the tool
INSERT INTO cargo_installed (
    crate,
    version,
    last_required_at
)
VALUES (
    ?1,
    ?2,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (crate, version) DO UPDATE
SET
    last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    crate = ?1
    AND version = ?2;
