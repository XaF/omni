-- Get the answers for a given repository in a given organization
-- :param: ?1 - the organization
-- :param: ?2 - the repository
-- :return: the answers for the prompts in the given organization and repository;
--          if a prompt has answers for the repository and for the whole org, only
--          the repository answer is returned
WITH ranked_prompts AS (
  SELECT
    prompt_id,
    answer,
    ROW_NUMBER() OVER (
      PARTITION BY prompt_id
      ORDER BY CASE WHEN repository = '__NULL__' THEN 1 ELSE 0 END
    ) as rn
  FROM prompts
  WHERE organization = ?1
    AND (repository = ?2 OR repository = '__NULL__')
)
SELECT prompt_id, answer
FROM ranked_prompts
WHERE rn = 1;
