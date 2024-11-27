-- Close any open entry in env_history for this workdir
UPDATE env_history
SET used_until_date = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE workdir_id = ?1
AND used_until_date IS NULL;
