SELECT
    env_version_id,
    versions,
    paths,
    env_vars,
    config_modtimes,
    config_hash,
    last_assigned_at
FROM env_versions
WHERE
    env_version_id = (
        SELECT env_version_id
        FROM workdir_env
        WHERE workdir_id = ?1
    )
;
