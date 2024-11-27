-- Delete orphaned environment versions
DELETE FROM env_versions AS ev
WHERE NOT EXISTS (
    SELECT 1 FROM (
        SELECT env_version_id FROM workdir_env
        UNION
        SELECT env_version_id FROM env_history
    ) AS combined
    WHERE combined.env_version_id = ev.env_version_id
);
