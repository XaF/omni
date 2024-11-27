-- List all the asdf installed tools that can be removed
-- :param1: number of seconds of the grace period before a tool can be removed
SELECT
    ai.tool,
    ai.version
FROM asdf_installed AS ai
WHERE NOT EXISTS (
    SELECT 1
    FROM asdf_installed_required_by AS airb
    WHERE airb.tool = ai.tool
          AND airb.version = ai.version
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
