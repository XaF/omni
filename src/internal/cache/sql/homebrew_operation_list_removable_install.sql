-- List all the homebrew formulas and cask that can be removed
-- :param1: number of seconds of the grace period before a formula or cask can be removed
SELECT
    hi.name,
    CASE
        WHEN hi.version = '__NULL__' THEN NULL
        ELSE hi.version
    END AS version,
    hi.cask,
    hi.installed
FROM homebrew_installed AS hi
WHERE NOT EXISTS (
    SELECT 1
    FROM homebrew_installed_required_by AS hirb
    WHERE hirb.name = hi.name
          AND hirb.version = hi.version
          AND hirb.cask = hi.cask
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
