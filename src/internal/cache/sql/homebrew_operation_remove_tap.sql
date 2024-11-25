-- Remove the provided tap from the homebrew_tapped table
-- :param: ?1 - the name of the tap to remove
DELETE FROM homebrew_tapped
WHERE name = ?1;
