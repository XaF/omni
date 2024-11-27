-- Delete the github releases that are not required by any workdir
-- :param1: number of seconds of the grace period before a release can be removed
DELETE FROM github_release_installed AS gri
WHERE NOT EXISTS (
    SELECT 1
    FROM github_release_required_by AS grrb
    WHERE grrb.repository = gri.repository
          AND grrb.version = gri.version
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
