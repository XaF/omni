-- Close duplicate open entries
WITH latest_open AS (
    SELECT workdir_id, MAX(used_from_date) as max_from_date
    FROM env_history
    WHERE used_until_date IS NULL
    GROUP BY workdir_id
)
UPDATE env_history
SET used_until_date = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE used_until_date IS NULL
AND EXISTS (
    SELECT 1 FROM latest_open
    WHERE latest_open.workdir_id = env_history.workdir_id
    AND latest_open.max_from_date != env_history.used_from_date
);
