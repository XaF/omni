use std::sync::Mutex;

use lazy_static::lazy_static;

use crate::internal::cache::CacheObject;
use crate::internal::cache::GithubReleaseOperationCache;
use crate::internal::cache::HomebrewOperationCache;
use crate::internal::cache::OmniPathCache;
use crate::internal::cache::PromptsCache;
use crate::internal::cache::RepositoriesCache;

lazy_static! {
    static ref GITHUB_RELEASES_OPERATION_CACHE: Mutex<GithubReleaseOperationCache> =
        Mutex::new(GithubReleaseOperationCache::new_load());
    static ref HOMEBREW_OPERATION_CACHE: Mutex<HomebrewOperationCache> =
        Mutex::new(HomebrewOperationCache::new_load());
    static ref OMNIPATH_CACHE: Mutex<OmniPathCache> = Mutex::new(OmniPathCache::new_load());
    static ref PROMPTS_CACHE: Mutex<PromptsCache> = Mutex::new(PromptsCache::new_load());
    static ref REPOSITORIES_CACHE: Mutex<RepositoriesCache> =
        Mutex::new(RepositoriesCache::new_load());
}

fn generic_get_cache<F>(cache: &Mutex<F>) -> F
where
    F: CacheObject + Clone,
{
    let cache = cache.lock().unwrap();
    cache.clone()
}

pub fn get_github_release_operation_cache() -> GithubReleaseOperationCache {
    generic_get_cache(&GITHUB_RELEASES_OPERATION_CACHE)
}

pub fn get_homebrew_operation_cache() -> HomebrewOperationCache {
    generic_get_cache(&HOMEBREW_OPERATION_CACHE)
}

pub fn get_omnipath_cache() -> OmniPathCache {
    generic_get_cache(&OMNIPATH_CACHE)
}

pub fn get_prompts_cache() -> PromptsCache {
    generic_get_cache(&PROMPTS_CACHE)
}

pub fn get_repositories_cache() -> RepositoriesCache {
    generic_get_cache(&REPOSITORIES_CACHE)
}

fn generic_set_cache<F>(cache: &Mutex<F>, cache_set: F)
where
    F: CacheObject,
{
    let mut cache = cache.lock().unwrap();
    *cache = cache_set;
}

pub fn set_github_release_operation_cache(cache_set: GithubReleaseOperationCache) {
    generic_set_cache(&GITHUB_RELEASES_OPERATION_CACHE, cache_set);
}

pub fn set_homebrew_operation_cache(cache_set: HomebrewOperationCache) {
    generic_set_cache(&HOMEBREW_OPERATION_CACHE, cache_set);
}

pub fn set_omnipath_cache(cache_set: OmniPathCache) {
    generic_set_cache(&OMNIPATH_CACHE, cache_set);
}

pub fn set_prompts_cache(cache_set: PromptsCache) {
    generic_set_cache(&PROMPTS_CACHE, cache_set);
}

pub fn set_repositories_cache(cache_set: RepositoriesCache) {
    generic_set_cache(&REPOSITORIES_CACHE, cache_set);
}
