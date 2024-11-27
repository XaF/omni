-- Check if a workdir is trusted
-- :param: ?1 - the workdir id
SELECT EXISTS (
    SELECT 1 FROM workdir_trusted WHERE workdir_id = ?1
) AS is_trusted;
