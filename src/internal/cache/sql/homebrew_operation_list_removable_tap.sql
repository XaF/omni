-- List all the homebrew taps that can be removed
-- :param: ?1 - number of seconds of the grace period before a tap can be removed
SELECT
    ht.name,
    ht.tapped
FROM homebrew_tapped AS ht
WHERE NOT EXISTS (
    SELECT 1
    FROM homebrew_tapped_required_by AS htrb
    WHERE htrb.name = ht.name
)
AND (
    CAST(strftime('%s', 'now') AS INTEGER) >
    (CAST(strftime('%s', last_required_at) AS INTEGER) + ?1)
);
