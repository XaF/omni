-- Set the bin path for a Homebrew formula or cask
-- :param1: The name of the Homebrew formula or cask
-- :param2: The version of the Homebrew formula or cask
-- :param3: Whether the formula or cask is a cask
-- :param4: A JSON array with the paths where the binaries of the Homebrew formula or cask are installed
INSERT INTO homebrew_install (
    name,
    version,
    cask,
    bin_paths,
    last_required_at
)
VALUES (
    ?1,
    COALESCE(?2, '__NULL__'),
    MIN(1, ?3),
    ?4,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (name, version, cask) DO UPDATE
SET
    bin_paths = ?4
WHERE
    name = ?1
    AND version = COALESCE(?2, '__NULL__')
    AND cask = MIN(1, ?3);
