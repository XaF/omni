-- Insert or update the homebrew last updated timestamp
INSERT INTO metadata (
    key,
    value
)
VALUES ('homebrew.updated_at', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
ON CONFLICT(key) DO UPDATE SET
    value = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE key = 'homebrew.updated_at';
