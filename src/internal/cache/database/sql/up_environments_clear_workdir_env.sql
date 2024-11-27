-- Remove the entry from workdir_env
DELETE FROM workdir_env
WHERE workdir_id = ?1;
