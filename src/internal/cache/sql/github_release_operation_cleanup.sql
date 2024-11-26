-- Delete the github releases that are not required by any workdir
-- :param1: number of seconds of the grace period before a release can be removed
DELETE FROM github_release_install AS gri
WHERE NOT EXISTS (
    SELECT 1
    FROM github_release_install_required_by AS grirb
    WHERE grirb.repository = gri.repository
          AND grirb.version = gri.version
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
