-- Insert or update the environment version used for a workdir
INSERT INTO workdir_env (
    workdir_id,
    env_version_id
)
VALUES (?1, ?2)
ON CONFLICT(workdir_id) DO UPDATE SET
    env_version_id = ?2
WHERE workdir_id = ?1;
