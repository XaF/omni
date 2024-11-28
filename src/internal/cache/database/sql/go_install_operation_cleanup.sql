-- Delete the go install versions that are not required by any workdir
-- :param1: number of seconds of the grace period before a version can be removed
DELETE FROM go_installed AS gi
WHERE NOT EXISTS (
    SELECT 1
    FROM go_install_required_by AS girb
    WHERE girb.import_path = gi.import_path
          AND girb.version = gi.version
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
