-- Delete the cache of github releases for repositories that aren't installed
-- at this time and that have been fetched more than a grace period ago
-- :param1: number of seconds of the grace period before versions can be removed
DELETE FROM github_releases AS gr
WHERE NOT EXISTS (
    SELECT 1
    FROM github_release_installed AS gri
    WHERE gri.repository = gr.repository
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', fetched_at) AS INTEGER) + ?1)
);
