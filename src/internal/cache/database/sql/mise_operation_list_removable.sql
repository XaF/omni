-- List all the mise installed tools that can be removed
-- :param1: number of seconds of the grace period before a tool can be removed
SELECT
    mi.normalized_name,
    mi.version
FROM mise_installed AS mi
WHERE NOT EXISTS (
    SELECT 1
    FROM mise_installed_required_by AS mirb
    WHERE mirb.normalized_name = mi.normalized_name
          AND mirb.version = mi.version
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
