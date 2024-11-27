-- Insert or update a homebrew formula or cask checked timestamp
-- :param1: The name of the formula or cask
-- :param2: The version of the formula or cask
-- :param3: Whether the formula or cask is a cask
INSERT INTO homebrew_install (
    name,
    version,
    cask,
    checked_at
)
VALUES (
    ?1,
    COALESCE(?2, '__NULL__'),
    MIN(1, ?3),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
)
ON CONFLICT(name, version, cask) DO UPDATE SET
    checked_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE
    name = ?1
    AND version = COALESCE(?2, '__NULL__')
    AND cask = MIN(1, ?3);
