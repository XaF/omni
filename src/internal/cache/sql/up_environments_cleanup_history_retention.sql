-- Apply retention period (?1 seconds)
DELETE FROM env_history
WHERE used_until_date IS NOT NULL
AND strftime('%s', used_until_date) < strftime('%s', 'now') - ?1;
