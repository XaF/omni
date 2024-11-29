-- Get the cargo versions cached for a given crate
-- :param: ?1 crate - the crate used with 'cargo install'
SELECT
    versions,
    fetched_at
FROM
    cargo_versions
WHERE
    crate = ?1;
