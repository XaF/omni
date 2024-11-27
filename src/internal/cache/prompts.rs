use std::collections::HashMap;

use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::git_env;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptsCache {}

impl PromptsCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn add_answer(
        &self,
        prompt_id: &str,
        org: String,
        repo: Option<String>,
        answer: serde_yaml::Value,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/prompts_add_answer.sql"),
            params![prompt_id, org, repo, serde_json::to_string(&answer)?],
        )?;
        Ok(inserted > 0)
    }

    pub fn answers(&self, path: &str) -> HashMap<String, serde_yaml::Value> {
        let git = git_env(path);
        match git.url() {
            Some(url) => match url.owner {
                Some(org) => self.get_answers(&org, &url.name),
                None => HashMap::new(),
            },
            None => HashMap::new(),
        }
    }

    pub fn get_answers(&self, org: &str, repo: &str) -> HashMap<String, serde_yaml::Value> {
        // Find all answers matching on the org and for which repo
        // is either matching or none
        let db = CacheManager::get();
        let answers: Vec<(String, String)> = match db.query_as(
            include_str!("database/sql/prompts_get_answers.sql"),
            params![org, repo],
        ) {
            Ok(answers) => answers,
            Err(_) => return HashMap::new(),
        };

        let converted_answers = answers
            .iter()
            .flat_map(|(id, answer)| {
                serde_yaml::from_str::<serde_yaml::Value>(answer)
                    .ok()
                    .map(|answer| (id.clone(), answer))
            })
            .collect::<HashMap<_, _>>();

        let mut answers = HashMap::new();
        for (id, answer) in converted_answers {
            answers.entry(id).or_insert(answer);
        }

        answers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::internal::testutils::run_with_env;

    mod prompts_cache {
        use super::*;

        #[test]
        fn test_add_and_get_answers() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "testorg";
                let repo = "testrepo";

                // Create test answers
                let answer1 = serde_yaml::Value::String("answer1".to_string());
                let answer2 = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());

                // Add answers
                assert!(cache
                    .add_answer(
                        "prompt1",
                        org.to_string(),
                        Some(repo.to_string()),
                        answer1.clone()
                    )
                    .expect("Failed to add answer1"));
                assert!(cache
                    .add_answer(
                        "prompt2",
                        org.to_string(),
                        Some(repo.to_string()),
                        answer2.clone()
                    )
                    .expect("Failed to add answer2"));

                // Get answers
                let answers = cache.get_answers(org, repo);
                assert_eq!(answers.len(), 2);
                assert_eq!(answers["prompt1"], answer1);
                assert_eq!(answers["prompt2"], answer2);
            });
        }

        #[test]
        fn test_org_level_answers() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "testorg";
                let repo = "testrepo";

                let org_answer = serde_yaml::Value::String("org_answer".to_string());
                let repo_answer = serde_yaml::Value::String("repo_answer".to_string());

                // Add org-level answer
                assert!(cache
                    .add_answer("prompt1", org.to_string(), None, org_answer.clone())
                    .expect("Failed to add org-level answer"));

                // Add repo-level answer
                assert!(cache
                    .add_answer(
                        "prompt2",
                        org.to_string(),
                        Some(repo.to_string()),
                        repo_answer.clone()
                    )
                    .expect("Failed to add repo-level answer"));

                // Get answers - should include both org and repo level
                let answers = cache.get_answers(org, repo);
                assert_eq!(answers.len(), 2);
                assert_eq!(answers["prompt1"], org_answer);
                assert_eq!(answers["prompt2"], repo_answer);
            });
        }

        #[test]
        fn test_repo_override_org_answer() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "testorg";
                let repo = "testrepo";
                let prompt_id = "prompt1";

                let org_answer = serde_yaml::Value::String("org_answer".to_string());
                let repo_answer = serde_yaml::Value::String("repo_answer".to_string());

                // Add org-level answer
                assert!(cache
                    .add_answer(prompt_id, org.to_string(), None, org_answer)
                    .expect("Failed to add org-level answer"));

                // Add repo-level answer for same prompt
                assert!(cache
                    .add_answer(
                        prompt_id,
                        org.to_string(),
                        Some(repo.to_string()),
                        repo_answer.clone()
                    )
                    .expect("Failed to add repo-level answer"));

                // Get answers - repo answer should take precedence
                let answers = cache.get_answers(org, repo);
                assert_eq!(answers.len(), 1);
                assert_eq!(answers[prompt_id], repo_answer);
            });
        }

        #[test]
        fn test_invalid_yaml_answer() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "testorg";
                let repo = "testrepo";

                let db = CacheManager::get();

                // Directly insert invalid YAML through SQL
                db.execute(
                    include_str!("database/sql/prompts_add_answer.sql"),
                    params!["prompt1", org, repo, "{invalid: yaml: value:}"],
                )
                .expect("Failed to insert invalid YAML");

                // Get answers - should ignore invalid YAML
                let answers = cache.get_answers(org, repo);
                assert_eq!(answers.len(), 0);
            });
        }

        #[test]
        fn test_multiple_answers_same_prompt() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "testorg";
                let repo = "testrepo";
                let prompt_id = "prompt1";

                let answer1 = serde_yaml::Value::String("answer1".to_string());
                let answer2 = serde_yaml::Value::String("answer2".to_string());

                // Add multiple answers for same prompt
                assert!(cache
                    .add_answer(prompt_id, org.to_string(), Some(repo.to_string()), answer1)
                    .expect("Failed to add answer1"));
                assert!(cache
                    .add_answer(
                        prompt_id,
                        org.to_string(),
                        Some(repo.to_string()),
                        answer2.clone()
                    )
                    .expect("Failed to add answer2"));

                // Get answers - should return only the latest answer
                let answers = cache.get_answers(org, repo);
                assert_eq!(answers.len(), 1);
                assert_eq!(answers[prompt_id], answer2);
            });
        }

        #[test]
        fn test_empty_repo_get_answers() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "testorg";

                // Add answer with no repo
                let answer = serde_yaml::Value::String("org_level".to_string());
                assert!(cache
                    .add_answer("prompt1", org.to_string(), None, answer.clone())
                    .expect("Failed to add org-level answer"));

                // Try getting answers with empty string repo
                let answers = cache.get_answers(org, "");
                assert_eq!(answers.len(), 1);
                assert_eq!(answers["prompt1"], answer);
            });
        }

        #[test]
        fn test_case_sensitivity() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "TestOrg";
                let repo = "TestRepo";
                let answer = serde_yaml::Value::String("test".to_string());

                // Add answer with uppercase
                assert!(cache
                    .add_answer(
                        "prompt1",
                        org.to_string(),
                        Some(repo.to_string()),
                        answer.clone()
                    )
                    .expect("Failed to add answer"));

                // Try getting with different cases
                let answers_lower = cache.get_answers(&org.to_lowercase(), &repo.to_lowercase());
                assert_eq!(answers_lower.len(), 1);
                assert_eq!(answers_lower["prompt1"], answer);

                let answers_upper = cache.get_answers(&org.to_uppercase(), &repo.to_uppercase());
                assert_eq!(answers_upper.len(), 1);
                assert_eq!(answers_upper["prompt1"], answer);
            });
        }

        #[test]
        fn test_empty_answers() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();

                // Non-existent org/repo
                let answers = cache.get_answers("nonexistent", "repo");
                assert!(answers.is_empty());

                // Non-existent repo for existing org
                let org = "testorg";
                let answer = serde_yaml::Value::String("test".to_string());
                assert!(cache
                    .add_answer("prompt1", org.to_string(), None, answer)
                    .expect("Failed to add answer"));

                let answers = cache.get_answers(org, "nonexistent");
                assert_eq!(answers.len(), 1); // Should still get org-level answers
            });
        }

        #[test]
        fn test_yaml_value_types() {
            run_with_env(&[], || {
                let cache = PromptsCache::get();
                let org = "testorg";
                let repo = "testrepo";

                // Test different YAML value types
                let values = vec![
                    // Array
                    serde_yaml::Value::Sequence(vec![
                        serde_yaml::Value::String("item1".to_string()),
                        serde_yaml::Value::String("item2".to_string()),
                    ]),
                    // Number
                    serde_yaml::Value::Number(serde_yaml::Number::from(42)),
                    // Boolean
                    serde_yaml::Value::Bool(true),
                    // Null
                    serde_yaml::Value::Null,
                    // Complex mapping
                    {
                        let mut map = serde_yaml::Mapping::new();
                        map.insert(
                            serde_yaml::Value::String("key".to_string()),
                            serde_yaml::Value::String("value".to_string()),
                        );
                        serde_yaml::Value::Mapping(map)
                    },
                ];

                // Add and verify each type
                for (i, value) in values.iter().enumerate() {
                    let prompt_id = format!("prompt{}", i);
                    assert!(cache
                        .add_answer(
                            &prompt_id,
                            org.to_string(),
                            Some(repo.to_string()),
                            value.clone()
                        )
                        .expect("Failed to add answer"));

                    let answers = cache.get_answers(org, repo);
                    assert_eq!(answers[&prompt_id], *value);
                }
            });
        }
    }
}
