-- Add a workdir id as trusted
-- :param: ?1 - the workdir id
INSERT INTO workdir_trusted (
    workdir_id
) VALUES (
    ?1
) ON CONFLICT(workdir_id) DO NOTHING;
