-- Insert or update the asdf last updated timestamp
INSERT INTO metadata (
    key,
    value
)
VALUES ('asdf.updated_at', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
ON CONFLICT(key) DO UPDATE SET
    value = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE key = 'asdf.updated_at';
