-- Remove a workdir fingerprint
-- :param: ?1 - the workdir id
-- :param: ?2 - the fingerprint type
DELETE FROM
   workdir_fingerprints
WHERE
   workdir_id = ?1
   AND fingerprint_type = ?2;
