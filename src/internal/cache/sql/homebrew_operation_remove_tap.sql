-- Remove the provided tap from the homebrew_tap table
-- :param: ?1 - the name of the tap to remove
DELETE FROM homebrew_tap
WHERE name = ?1;
