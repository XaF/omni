-- Remove the provided cask or formula from the homebrew_install table
-- :param: ?1 - the name of the cask or formula to remove
-- :param: ?2 - the version of the cask or formula to remove
-- :param: ?3 - whether this is a cask (1) or formula (0)
DELETE FROM homebrew_install
WHERE
    name = ?1
    AND version = COALESCE(?2, '__NULL__')
    AND cask = MIN(1, ?3);
