-- Set the last updated timestamp of the omnipath
-- :param ?1 - the last updated timestamp, taken as parameter
--             so that the update can be use as a locking mechanism
INSERT INTO metadata (
    key,
    value
)
VALUES (
    'omnipath.updated_at',
    strftime('%Y-%m-%d %H:%M:%S', 'now')
)
ON CONFLICT (key) DO UPDATE
SET
    value = strftime('%Y-%m-%d %H:%M:%S', 'now')
WHERE
    key = 'omnipath.updated_at'
    AND ((value IS NULL AND ?1 IS NULL) OR value = ?1);
