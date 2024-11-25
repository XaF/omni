-- Check if a given env version id already exists
SELECT EXISTS(
    SELECT
        1
    FROM
        env_versions
    WHERE
        env_version_id = ?1
)
