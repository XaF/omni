-- Insert or update a homebrew tap last updated timestamp
-- :param1: The name of the tap
INSERT INTO homebrew_tapped (
    name,
    updated_at
)
VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
ON CONFLICT(name) DO UPDATE SET
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE name = ?1;
