-- Delete the cargo install versions that are not required by any workdir
-- :param1: number of seconds of the grace period before a version can be removed
DELETE FROM cargo_installed AS gi
WHERE NOT EXISTS (
    SELECT 1
    FROM cargo_install_required_by AS girb
    WHERE girb.crate = gi.crate
          AND girb.version = gi.version
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
