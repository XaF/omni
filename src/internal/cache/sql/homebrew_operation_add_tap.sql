-- Add a new homebrew tap to the database
-- :param1: name - the name of the tap
-- :param2: tapped - whether the tap was tapped or not
INSERT INTO homebrew_tapped (
    name,
    tapped,
    last_required_at
)
VALUES (
    ?1,
    MIN(1, ?2),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (name) DO UPDATE
SET
    tapped = MIN(1, (tapped OR ?2)),
    last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE name = ?1;
