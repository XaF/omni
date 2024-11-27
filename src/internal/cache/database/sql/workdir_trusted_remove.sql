-- Remote the trust from a workdir id
-- :param: ?1 - the workdir id
DELETE FROM
    workdir_trusted
WHERE
    workdir_id = ?1;
