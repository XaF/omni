-- Delete the cache of go versions for import paths that aren't installed
-- at this time and that have been fetched more than a grace period ago
-- :param1: number of seconds of the grace period before versions can be removed
DELETE FROM go_versions AS gv
WHERE NOT EXISTS (
    SELECT 1
    FROM go_installed AS gi
    WHERE gi.import_path = gv.import_path
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', fetched_at) AS INTEGER) + ?1)
);
