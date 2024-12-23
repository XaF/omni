-- Check if a mise plugin should be updated
-- :param ?1 - plugin name
-- :param ?2 - validity of the update in seconds
-- :return - boolean, 1 if mise should be updated, 0 otherwise
WITH plugin_update AS (
  SELECT updated_at as timestamp
  FROM mise_plugins
  WHERE plugin_name = ?1
),
is_expired AS (
  SELECT
    CASE
      WHEN timestamp IS NULL THEN 1
      WHEN CAST(strftime('%s', 'now') AS INTEGER) >
           (CAST(strftime('%s', timestamp) AS INTEGER) + ?2) THEN 1
      ELSE 0
    END as expired
  FROM plugin_update
  UNION ALL
  SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM plugin_update)
)
SELECT expired = 1 as is_expired FROM is_expired;
