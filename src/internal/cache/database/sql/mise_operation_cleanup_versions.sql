-- Delete the cache of github releases for repositories that aren't installed
-- at this time and that have been fetched more than a grace period ago
-- :param1: number of seconds of the grace period before versions can be removed
DELETE FROM mise_plugins AS mp
WHERE NOT EXISTS (
    SELECT 1
    FROM mise_installed AS mi
    WHERE mi.plugin_name = mp.plugin_name
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', versions_fetched_at) AS INTEGER) + ?1)
);
