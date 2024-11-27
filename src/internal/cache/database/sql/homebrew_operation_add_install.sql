-- Add a new homebrew formula or cask to the database
-- :param1: name - the name of the tap
-- :param2: version - the version of the formula or cask if any specified
-- :param3: cask - whether the formula is a cask or not
-- :param4: installed - whether the formula or cask was installed or not
INSERT INTO homebrew_install (
    name,
    version,
    cask,
    installed,
    last_required_at
)
VALUES (
    ?1,
    COALESCE(?2, '__NULL__'), -- SQLite does not support NULL in UNIQUE constraints
    MIN(1, ?3),
    MIN(1, ?4),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (name, version, cask) DO UPDATE
SET
    installed = MIN(1, (installed OR ?4)),
    last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    name = ?1
    AND version = COALESCE(?2, '__NULL__')
    AND cask = ?3;
