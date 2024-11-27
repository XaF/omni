-- Check if a workdir fingerprint matches the expected value
-- :param: ?1 - the workdir id
-- :param: ?2 - the fingerprint type
-- :param: ?3 - the expected fingerprint value
SELECT EXISTS (
    SELECT 1
    FROM workdir_fingerprints
    WHERE workdir_id = ?1
    AND fingerprint_type = ?2
    AND fingerprint = CAST(?3 AS STRING)
) OR (
    -- Special case: when ?3 is 0, returns true if either:
    --   - the row exists and fingerprint = 0
    --   - or the row doesn't exist at all
    CAST(?3 AS STRING) = '0' AND NOT EXISTS (
        SELECT 1
        FROM workdir_fingerprints
        WHERE workdir_id = ?1
        AND fingerprint_type = ?2
    )
) AS fingerprint_matches;
