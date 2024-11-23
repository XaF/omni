-- Keep only max per workdir (?1)
DELETE FROM env_history
WHERE env_history_id IN (
    SELECT env_history_id FROM (
        SELECT
            env_history_id,
            ROW_NUMBER() OVER (
                PARTITION BY workdir_id
                ORDER BY
                    used_until_date IS NULL DESC,
                    COALESCE(used_until_date, used_from_date) DESC,
                    used_from_date DESC
            ) as rn
        FROM env_history
    ) ranked
    WHERE rn > ?1
    AND used_until_date IS NOT NULL
);
