-- Add a prompt answer to the cache
-- :param: ?1 - the prompt id
-- :param: ?2 - the organization
-- :param: ?3 - the repository
-- :param: ?4 - the answer
INSERT INTO prompts (
    prompt_id,
    organization,
    repository,
    answer,
    updated_at
)
VALUES (
    ?1,
    ?2,
    COALESCE(?3, '__NULL__'), -- SQLite does not support NULL in UNIQUE constraints
    ?4,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (prompt_id, organization, repository) DO UPDATE
SET
    answer = ?4,
    updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    prompt_id = ?1
    AND organization = ?2
    AND repository = COALESCE(?3, '__NULL__');
