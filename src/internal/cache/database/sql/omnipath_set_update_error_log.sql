-- Set the last error log when updating the omnipath
-- :param ?1 - the last update error log
INSERT INTO metadata (
    key,
    value
)
VALUES (
    'omnipath.update_error_log',
    ?1
)
ON CONFLICT (key) DO UPDATE
SET
    value = ?1;
WHERE
    key = 'omnipath.update_error_log';
