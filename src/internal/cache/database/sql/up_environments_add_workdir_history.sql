-- Add an entry in the workdir env history for the given workdir and env version
INSERT INTO env_history (
    workdir_id,
    env_version_id,
    head_sha,
    used_from_date,
    used_until_date
)
VALUES (?1, ?2, ?3, strftime('%Y-%m-%d %H:%M:%S', 'now'), NULL);
