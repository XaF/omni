-- Remove the provided tool and version from the cache
-- :param1: the name of the tool to remove
-- :param2: the version of the tool to remove
DELETE FROM mise_installed
WHERE tool = ?1 AND version = ?2
