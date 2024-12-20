-- Check if mise should be updated
-- :param ?1 - validity of the update in seconds
-- :return - boolean, 1 if mise should be updated, 0 otherwise
WITH updated_at AS (
  SELECT value as timestamp
  FROM metadata
  WHERE key = 'mise.updated_at'
),
is_expired AS (
  SELECT
    CASE
      WHEN timestamp IS NULL THEN 1
      WHEN CAST(strftime('%s', 'now') AS INTEGER) >
           (CAST(strftime('%s', timestamp) AS INTEGER) + ?1) THEN 1
      ELSE 0
    END as expired
  FROM updated_at
  UNION ALL
  SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM updated_at)
)
SELECT expired = 1 as is_expired FROM is_expired;
