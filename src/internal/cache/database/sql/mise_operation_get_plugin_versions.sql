-- Get the available versions for a plugin
-- :param ?1 - plugin name
-- :return - JSON array of available versions, and the time they were fetched
SELECT
    versions,
    versions_fetched_at
FROM mise_plugins
WHERE plugin = ?1;
