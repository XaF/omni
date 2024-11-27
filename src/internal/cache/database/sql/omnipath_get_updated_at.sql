-- Get the last updated timestamp of the omnipath, and check if it is expired
-- :param ?1 - validity of the update in seconds
SELECT
    CASE
        WHEN value IS NULL THEN 1
        WHEN strftime('%s', value) IS NULL THEN 1
        WHEN CAST(strftime('%s', 'now') AS INTEGER) >
             (CAST(strftime('%s', value) AS INTEGER) + ?1) THEN 1
        ELSE 0
    END AS is_expired,
    value AS timestamp
FROM
    metadata
WHERE
    key = 'omnipath.updated_at';
