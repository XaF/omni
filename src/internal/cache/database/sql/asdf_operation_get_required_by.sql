-- Get the required_by list for a tool
-- :param1: tool - the name of the tool
-- :param2: version - the version of the tool
SELECT required_by
FROM asdf_installed
WHERE tool = ?1 AND version = ?2;
