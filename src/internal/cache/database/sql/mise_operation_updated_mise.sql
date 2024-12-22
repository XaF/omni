-- Insert or update the mise last updated timestamp
INSERT INTO metadata (
    key,
    value
)
VALUES ('mise.updated_at', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
ON CONFLICT(key) DO UPDATE SET
    value = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE key = 'mise.updated_at';
