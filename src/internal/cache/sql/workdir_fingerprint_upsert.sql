-- Insert or update a workdir fingerprint
-- :param: ?1 - the workdir id
-- :param: ?2 - the fingerprint type
-- :param: ?3 - the fingerprint value
INSERT INTO workdir_fingerprints (
    workdir_id,
    fingerprint_type,
    fingerprint
) VALUES (
    ?1,
    ?2,
    ?3
) ON CONFLICT(workdir_id, fingerprint_type) DO UPDATE
SET
    -- excluded is a special table that contains the values
    -- that would have been inserted if the insert had not
    -- conflicted
    fingerprint = excluded.fingerprint;
