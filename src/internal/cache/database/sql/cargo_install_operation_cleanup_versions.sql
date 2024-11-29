-- Delete the cache of cargo versions for crates that aren't installed
-- at this time and that have been fetched more than a grace period ago
-- :param1: number of seconds of the grace period before versions can be removed
DELETE FROM cargo_versions AS cv
WHERE NOT EXISTS (
    SELECT 1
    FROM cargo_installed AS ci
    WHERE ci.crate = cv.crate
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', fetched_at) AS INTEGER) + ?1)
);
