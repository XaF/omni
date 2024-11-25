-- Insert or update a homebrew formula or cask updated timestamp
-- :param1: The name of the formula or cask
-- :param2: The version of the formula or cask
-- :param3: Whether the formula or cask is a cask
INSERT INTO homebrew_installed (
    name,
    version,
    cask,
    updated_at
)
VALUES (
    ?1,
    ?2,
    MIN(1, ?3),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
)
ON CONFLICT(name, version, cask) DO UPDATE SET
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE
    name = ?1
    AND version = ?2
    AND cask = MIN(1, ?3);
