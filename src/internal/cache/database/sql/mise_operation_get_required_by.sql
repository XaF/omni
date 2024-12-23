-- Get the required_by list for a tool
-- :param1: normalized_name - the name of the plugin
-- :param2: version - the version of the tool
SELECT required_by
FROM mise_installed
WHERE normalized_name = ?1 AND version = ?2;
