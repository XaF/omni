-- Check if an homebrew formula or cask should be updated
-- :param ?1 - name of the formula or cask
-- :param ?2 - version of the formula or cask
-- :param ?3 - 1 if the formula is a cask, 0 otherwise
-- :param ?4 - validity of the update in seconds
-- :return - boolean, 1 if should be updated, 0 otherwise
WITH last_update AS (
  SELECT updated_at as timestamp
  FROM homebrew_installed
  WHERE
    name = ?1
    AND version = COALESCE(?2, '__NULL__')
    AND cask = MIN(1, ?3)
),
is_expired AS (
  SELECT
    CASE
      WHEN timestamp IS NULL THEN 1
      WHEN CAST(strftime('%s', 'now') AS INTEGER) >
           (CAST(strftime('%s', timestamp) AS INTEGER) + ?4) THEN 1
      ELSE 0
    END as expired
  FROM last_update
  UNION ALL
  SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM last_update)
)
SELECT expired = 1 as is_expired FROM is_expired;
