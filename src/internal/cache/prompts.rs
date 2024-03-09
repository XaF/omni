use std::collections::HashMap;
use std::io;

use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

use crate::internal::cache::handler::exclusive;
use crate::internal::cache::handler::shared;
use crate::internal::cache::loaders::get_prompts_cache;
use crate::internal::cache::loaders::set_prompts_cache;
use crate::internal::cache::utils;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::CacheObject;
use crate::internal::git_env;

const PROMPTS_CACHE_NAME: &str = "prompts";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptsCache {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<PromptAnswer>,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub updated_at: OffsetDateTime,
}

impl PromptsCache {
    pub fn updated(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn add_answer(
        &mut self,
        prompt_id: &str,
        org: String,
        repo: Option<String>,
        answer: serde_yaml::Value,
    ) {
        for existing_answer in &mut self.answers {
            if existing_answer.id == prompt_id
                && existing_answer.org == org
                && existing_answer.repo == repo
            {
                existing_answer.answer = answer;
                self.updated();
                return;
            }
        }

        self.answers
            .push(PromptAnswer::new(prompt_id, org, repo, answer));
        self.updated();
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
        let matching_answers = self
            .answers
            .iter()
            .filter(|answer| {
                answer.org == org
                    && match &answer.repo {
                        Some(answer_repo) => answer_repo == repo,
                        None => true,
                    }
            })
            .collect::<Vec<_>>();

        // Now we want to keep only a single of each prompt_id if there
        // are duplicates, but we want to keep the one defined for the
        // repository if there is one
        let mut answers = HashMap::new();
        for answer in matching_answers {
            if !answers.contains_key(&answer.id) || answer.repo.is_some() {
                answers.insert(answer.id.clone(), answer.answer.clone());
            }
        }

        answers
    }
}

impl Empty for PromptsCache {
    fn is_empty(&self) -> bool {
        self.answers.is_empty()
    }
}

impl CacheObject for PromptsCache {
    fn new_empty() -> Self {
        Self {
            answers: Vec::new(),
            updated_at: utils::origin_of_time(),
        }
    }

    fn get() -> Self {
        get_prompts_cache()
    }

    fn shared() -> io::Result<Self> {
        shared::<Self>(PROMPTS_CACHE_NAME)
    }

    fn exclusive<F>(processing_fn: F) -> io::Result<Self>
    where
        F: FnOnce(&mut Self) -> bool,
    {
        exclusive::<Self, F, fn(Self)>(PROMPTS_CACHE_NAME, processing_fn, set_prompts_cache)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptAnswer {
    pub id: String,
    pub org: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    pub answer: serde_yaml::Value,
}

impl PromptAnswer {
    pub fn new(id: &str, org: String, repo: Option<String>, answer: serde_yaml::Value) -> Self {
        Self {
            id: id.to_string(),
            org,
            repo,
            answer,
        }
    }
}
