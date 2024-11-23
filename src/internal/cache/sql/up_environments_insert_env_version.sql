INSERT INTO env_versions (
    env_version_id,
    versions,
    paths,
    env_vars,
    config_modtimes,
    config_hash,
    last_assigned_at
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
ON CONFLICT(env_version_id) DO UPDATE SET
    last_assigned_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE env_version_id = ?1;
