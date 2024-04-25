use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use itertools::Itertools;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils as cache_utils;
use crate::internal::cache::CacheObject;
use crate::internal::cache::GithubReleaseOperationCache;
use crate::internal::cache::GithubReleaseVersion;
use crate::internal::cache::GithubReleases;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::force_remove_dir_all;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::config::up::utils::VersionParser;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::env::data_home;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;

static GITHUB_RELEASES_BIN_PATH: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from(data_home()).join("ghreleases"));

fn github_releases_bin_path() -> PathBuf {
    GITHUB_RELEASES_BIN_PATH.clone()
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct UpConfigGithubReleases {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    releases: Vec<UpConfigGithubRelease>,
}

impl Serialize for UpConfigGithubReleases {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.releases.len() {
            0 => serializer.serialize_none(),
            1 => serializer.serialize_newtype_struct("UpConfigGithubReleases", &self.releases[0]),
            _ => serializer.collect_seq(self.releases.iter()),
        }
    }
}

impl UpConfigGithubReleases {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return UpConfigGithubReleases::default(),
        };

        if let Some(_repository) = config_value.as_str_forced() {
            return Self {
                releases: vec![UpConfigGithubRelease::from_config_value(Some(config_value))],
            };
        }

        if let Some(array) = config_value.as_array() {
            return Self {
                releases: array
                    .iter()
                    .map(|config_value| {
                        UpConfigGithubRelease::from_config_value(Some(config_value))
                    })
                    .collect(),
            };
        }

        if let Some(table) = config_value.as_table() {
            // Check if there is a 'repository' key, in which case it's a single
            // repository and we can just parse it and return it
            if ["repository", "repo"]
                .iter()
                .find_map(|key| table.get(*key))
                .is_some()
            {
                return Self {
                    releases: vec![UpConfigGithubRelease::from_config_value(Some(config_value))],
                };
            }

            // Otherwise, we have a table of repositories, where repositories are
            // the keys and the values are the configuration for the repository;
            // we want to go over them in lexico-graphical order to ensure that
            // the order is consistent
            let mut releases = Vec::new();
            for repo in table.keys().sorted() {
                let value = table.get(repo).expect("repo config not found");
                let repository = match ConfigValue::from_str(repo) {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                let mut repo_config = if let Some(table) = value.as_table() {
                    table.clone()
                } else if let Some(version) = value.as_str_forced() {
                    let mut repo_config = HashMap::new();
                    let value = match ConfigValue::from_str(&version) {
                        Ok(value) => value,
                        Err(_) => continue,
                    };
                    repo_config.insert("version".to_string(), value);
                    repo_config
                } else {
                    HashMap::new()
                };

                repo_config.insert("repository".to_string(), repository);
                releases.push(UpConfigGithubRelease::from_table(&repo_config));
            }

            return Self { releases };
        }

        UpConfigGithubReleases::default()
    }

    pub fn up(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        if self.releases.len() == 1 {
            return self.releases[0].up(options, progress_handler);
        }

        progress_handler.init("github releases:".light_blue());
        if self.releases.is_empty() {
            progress_handler.error_with_message("no release information".to_string());
            return Err(UpError::Config("at least one release required".to_string()));
        }

        progress_handler.progress("install dependencies".to_string());

        let num = self.releases.len();
        for (idx, release) in self.releases.iter().enumerate() {
            let subhandler = progress_handler.subhandler(
                &format!(
                    "[{current:padding$}/{total:padding$}] {release} ",
                    current = idx + 1,
                    total = num,
                    padding = format!("{}", num).len(),
                    release = release.desc(),
                )
                .light_yellow(),
            );
            release.up(options, &subhandler)?;
        }

        progress_handler.success_with_message(self.get_up_message());

        Ok(())
    }

    fn get_up_message(&self) -> String {
        let count: HashMap<GithubReleaseHandled, usize> = self
            .releases
            .iter()
            .map(|release| release.handling())
            .fold(HashMap::new(), |mut map, item| {
                *map.entry(item).or_insert(0) += 1;
                map
            });
        let handled: Vec<String> = self
            .releases
            .iter()
            .filter_map(|release| match release.handling() {
                GithubReleaseHandled::Handled | GithubReleaseHandled::Noop => Some(format!(
                    "{} {}",
                    release.repository,
                    release
                        .actual_version
                        .get()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "?".to_string())
                )),
                _ => None,
            })
            .sorted()
            .collect();

        if handled.is_empty() {
            return "nothing done".to_string();
        }

        let mut numbers = vec![];

        if let Some(count) = count.get(&GithubReleaseHandled::Handled) {
            numbers.push(format!("{} installed", count).green());
        }

        if let Some(count) = count.get(&GithubReleaseHandled::Noop) {
            numbers.push(format!("{} already installed", count).light_black());
        }

        if numbers.is_empty() {
            return "nothing done".to_string();
        }

        format!(
            "{} {}",
            numbers.join(", "),
            format!("({})", handled.join(", ")).light_black().italic(),
        )
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        if self.releases.len() == 1 {
            return self.releases[0].down(progress_handler);
        }

        progress_handler.init("github releases:".light_blue());
        progress_handler.progress("updating dependencies".to_string());

        let num = self.releases.len();
        for (idx, release) in self.releases.iter().enumerate() {
            let subhandler = progress_handler.subhandler(
                &format!(
                    "[{current:padding$}/{total:padding$}] ",
                    current = idx + 1,
                    total = num,
                    padding = format!("{}", num).len(),
                )
                .light_yellow(),
            );
            release.down(&subhandler)?;
        }

        progress_handler.success_with_message("dependencies cleaned".light_green());

        Ok(())
    }

    pub fn cleanup(progress_handler: &UpProgressHandler) -> Result<Option<String>, UpError> {
        let wd = workdir(".");
        let wd_id = match wd.id() {
            Some(wd_id) => wd_id,
            None => return Err(UpError::Exec("failed to get workdir id".to_string())),
        };

        let mut return_value: Result<(bool, usize, Vec<PathBuf>), UpError> =
            Err(UpError::Exec("cleanup_path not run".to_string()));

        if let Err(err) = GithubReleaseOperationCache::exclusive(|ghrelease| {
            progress_handler.init("github releases:".light_blue());
            progress_handler.progress("checking for unused github releases".to_string());

            let mut updated = false;

            let expected_paths = ghrelease
                .installed
                .iter_mut()
                .filter_map(|install| {
                    // Cleanup the references to this repository for
                    // any installed github release that is not currently
                    // listed in the up configuration
                    if install.required_by.contains(&wd_id) && install.stale() {
                        install.required_by.retain(|id| id != &wd_id);
                        updated = true;
                    }

                    // Only return the path if the github release is
                    // expected, as we will clear the bin path from
                    // all unexpected github releases
                    if install.removable() {
                        None
                    } else {
                        Some(
                            github_releases_bin_path()
                                .join(&install.repository)
                                .join(&install.version),
                        )
                    }
                })
                .collect::<Vec<PathBuf>>();

            return_value = cleanup_path(
                github_releases_bin_path(),
                expected_paths,
                progress_handler,
                true,
            );

            return_value.is_ok() && updated
        }) {
            progress_handler.progress(format!("failed to update cache: {}", err).light_yellow());
        }

        let (root_removed, num_removed, removed_paths) = return_value?;

        if root_removed {
            return Ok(Some("removed all github releases".to_string()));
        }

        if num_removed == 0 {
            return Ok(None);
        }

        // We want to go over the paths that were removed to
        // return a proper message about the github releases
        // that were removed
        let removed_releases = removed_paths
            .iter()
            .filter_map(|path| {
                // Path should starts with the bin path if it is a release
                let rest_of_path = match path.strip_prefix(github_releases_bin_path()) {
                    Ok(rest_of_path) => rest_of_path,
                    Err(_) => return None,
                };

                // Path should have three components left after stripping
                // the bin path: the repository (2) and the version (1)
                let parts = rest_of_path.components().collect::<Vec<_>>();
                if parts.len() > 3 {
                    return None;
                }

                let parts = parts
                    .into_iter()
                    .map(|part| part.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<String>>();

                let repo_owner = parts[0].clone();
                let repo_name = if parts.len() > 1 {
                    Some(parts[1].clone())
                } else {
                    None
                };
                let version = if parts.len() > 2 {
                    Some(parts[2].clone())
                } else {
                    None
                };

                Some((repo_owner, repo_name, version))
            })
            .collect::<Vec<_>>();

        if removed_releases.is_empty() {
            return Ok(Some(format!(
                "removed {} release{}",
                num_removed.light_yellow(),
                if num_removed > 1 { "s" } else { "" }
            )));
        }

        let removed_releases = removed_releases
            .iter()
            .map(
                |(repo_owner, repo_name, version)| match (repo_name, version) {
                    (Some(repo_name), Some(version)) => format!(
                        "{}/{} {}",
                        repo_owner.light_yellow(),
                        repo_name.light_yellow(),
                        version.light_yellow()
                    ),
                    (Some(repo_name), None) => {
                        format!(
                            "{}/{} (all versions)",
                            repo_owner.light_yellow(),
                            repo_name.light_yellow()
                        )
                    }
                    (None, _) => format!("{} (all releases)", repo_owner.light_yellow()),
                },
            )
            .collect::<Vec<_>>();

        Ok(Some(format!("removed {}", removed_releases.join(", "))))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
pub enum GithubReleaseHandled {
    Handled,
    Noop,
    Unhandled,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UpConfigGithubRelease {
    /// The repository to install the tool from, should
    /// be in the format `owner/repo`
    pub repository: String,

    /// The version of the tool to install
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Whether to install the pre-release version of the tool
    /// if it is the most recent matching version
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    pub prerelease: bool,

    /// Whether to allow versions containing build details
    /// (e.g. 1.2.3+build)
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    pub build: bool,

    /// Whether to install a file that is not currently in an
    /// archive. This is useful for tools that are being
    /// distributed as a single binary file outside of an archive.
    #[serde(
        default = "cache_utils::set_true",
        skip_serializing_if = "cache_utils::is_true"
    )]
    pub binary: bool,

    /// The URL of the GitHub API; this is only required if downloading
    /// using Github Enterprise. By default, this is set to the public
    /// GitHub API URL (https://api.github.com). If you are using
    /// Github Enterprise, you should set this to the URL of your
    /// Github Enterprise instance (e.g. https://github.example.com/api/v3)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,

    #[serde(default, skip)]
    pub actual_version: OnceCell<String>,

    #[serde(default, skip)]
    was_handled: OnceCell<GithubReleaseHandled>,
}

impl Default for UpConfigGithubRelease {
    fn default() -> Self {
        UpConfigGithubRelease {
            repository: "".to_string(),
            version: None,
            prerelease: false,
            build: false,
            binary: true,
            api_url: None,
            actual_version: OnceCell::new(),
            was_handled: OnceCell::new(),
        }
    }
}

impl UpConfigGithubRelease {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return UpConfigGithubRelease::default(),
        };

        if let Some(table) = config_value.as_table() {
            Self::from_table(&table)
        } else if let Some(repository) = config_value.as_str_forced() {
            UpConfigGithubRelease {
                repository,
                ..UpConfigGithubRelease::default()
            }
        } else {
            UpConfigGithubRelease::default()
        }
    }

    fn from_table(table: &HashMap<String, ConfigValue>) -> Self {
        let repository = ["repository", "repo"]
            .iter()
            .find_map(|key| table.get(*key));

        let repository = match repository {
            Some(repository) => repository,
            None => {
                if table.len() == 1 {
                    let (key, value) = table.iter().next().unwrap();
                    if let Some(version) = value.as_str_forced() {
                        return UpConfigGithubRelease {
                            repository: key.clone(),
                            version: Some(version.to_string()),
                            ..UpConfigGithubRelease::default()
                        };
                    } else if let (Some(table), Ok(repo_config_value)) =
                        (value.as_table(), ConfigValue::from_str(key))
                    {
                        let mut repo_config = table.clone();
                        repo_config.insert("repository".to_string(), repo_config_value);
                        return UpConfigGithubRelease::from_table(&repo_config);
                    }
                }
                return UpConfigGithubRelease::default();
            }
        };

        let repository = if let Some(repository_details) = repository.as_table() {
            let owner = repository_details
                .get("owner")
                .map(|v| v.as_str_forced())
                .unwrap_or(None)
                .unwrap_or("".to_string());
            let name = repository_details
                .get("name")
                .map(|v| v.as_str_forced())
                .unwrap_or(None)
                .unwrap_or("".to_string());
            format!("{}/{}", owner, name)
        } else if let Some(repository) = repository.as_str_forced() {
            repository.to_string()
        } else {
            "".to_string()
        };

        let version = table
            .get("version")
            .map(|v| v.as_str_forced())
            .unwrap_or(None);
        let prerelease = table
            .get("prerelease")
            .map(|v| v.as_bool())
            .unwrap_or(None)
            .unwrap_or(false);
        let build = table
            .get("build")
            .map(|v| v.as_bool())
            .unwrap_or(None)
            .unwrap_or(false);
        let binary = table
            .get("binary")
            .map(|v| v.as_bool())
            .unwrap_or(None)
            .unwrap_or(true);
        let api_url = table
            .get("api_url")
            .map(|v| v.as_str_forced())
            .unwrap_or(None);

        UpConfigGithubRelease {
            repository,
            version,
            prerelease,
            build,
            binary,
            api_url,
            ..UpConfigGithubRelease::default()
        }
    }

    fn update_cache(&self, progress_handler: &dyn ProgressHandler) {
        let wd = workdir(".");
        let wd_id = match wd.id() {
            Some(wd_id) => wd_id,
            None => return,
        };

        let version = match self.actual_version.get() {
            Some(version) => version,
            None => {
                progress_handler.error_with_message("version not set".to_string());
                return;
            }
        };

        progress_handler.progress("updating cache".to_string());

        if let Err(err) = GithubReleaseOperationCache::exclusive(|ghrelease| {
            ghrelease.add_installed(&wd_id, &self.repository, version)
        }) {
            progress_handler.progress(format!("failed to update github release cache: {}", err));
            return;
        }

        let release_version_path = self.release_version_path(version);

        if let Err(err) =
            UpEnvironmentsCache::exclusive(|up_env| up_env.add_path(&wd_id, release_version_path))
        {
            progress_handler.progress(format!("failed to update up environment cache: {}", err));
            return;
        }

        progress_handler.progress("updated cache".to_string());
    }

    fn desc(&self) -> String {
        if self.repository.is_empty() {
            "github release:".to_string()
        } else {
            format!(
                "{} ({}):",
                self.repository,
                match self.version {
                    None => "latest".to_string(),
                    Some(ref version) if version.is_empty() => "latest".to_string(),
                    Some(ref version) => version.clone(),
                }
            )
        }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.init(self.desc().light_blue());
        progress_handler.progress("install github release".to_string());

        if self.repository.is_empty() {
            progress_handler.error_with_message("repository is required".to_string());
            return Err(UpError::Config("repository is required".to_string()));
        }

        let installed = self.resolve_and_download_release(options, progress_handler)?;

        self.update_cache(progress_handler);

        let version = match self.actual_version.get() {
            Some(version) => version.to_string(),
            None => "unknown".to_string(),
        };
        let msg = match installed {
            true => format!("{} installed", version.light_yellow()),
            false => format!("{} already installed", version).light_black(),
        };
        progress_handler.success_with_message(msg);

        Ok(())
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        let wd = workdir(".");
        let wd_id = match wd.id() {
            Some(wd_id) => wd_id,
            None => return Ok(()),
        };

        if let Err(err) = GithubReleaseOperationCache::exclusive(|ghrelease| {
            progress_handler.init(self.desc().light_blue());
            progress_handler.progress("updating github release dependencies".to_string());

            let mut updated = false;

            for install in ghrelease
                .installed
                .iter_mut()
                .filter(|install| install.required_by.contains(&wd_id))
            {
                install.required_by.retain(|id| id != &wd_id);
                updated = true;
            }

            updated
        }) {
            progress_handler.progress(format!("failed to update cache: {}", err).light_yellow());
        }

        progress_handler.success_with_message("github release dependencies cleaned".light_green());

        Ok(())
    }

    fn resolve_and_download_release(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<bool, UpError> {
        let releases = self.list_releases(options, progress_handler)?;
        let release = match self.resolve_release(&releases) {
            Ok(release) => release,
            Err(err) => {
                // If the release is not fresh of now, and we failed to
                // resolve the release, we should try to refresh the
                // release list and try again
                if options.read_cache && !releases.is_fresh() {
                    progress_handler.progress("no matching release found in cache".to_string());

                    let releases = self.list_releases(
                        &UpOptions {
                            read_cache: false,
                            ..options.clone()
                        },
                        progress_handler,
                    )?;

                    self.resolve_release(&releases).map_err(|err| {
                        progress_handler.error_with_message(err.message());
                        err
                    })?
                } else {
                    progress_handler.error_with_message(err.message());
                    return Err(err);
                }
            }
        };

        let mut version = release.version();

        // Try installing the release found
        let mut download_release = self.download_release(options, &release, progress_handler);
        if download_release.is_err() {
            // If we get here and there is an issue downloading the release,
            // list all installed versions and check if one of those could
            // fit the requirement, in which case we can fallback to it
            let installed_versions = self.list_installed_versions(progress_handler)?;
            match self.resolve_version(&installed_versions) {
                Ok(installed_version) => {
                    progress_handler.progress(format!(
                        "falling back to {} {}",
                        self.repository,
                        installed_version.light_yellow(),
                    ));

                    version = installed_version;
                    download_release = Ok(false);
                }
                Err(_err) => {}
            }
        }

        if let Ok(downloaded) = &download_release {
            self.actual_version.set(version.to_string()).map_err(|_| {
                let errmsg = "failed to set actual version".to_string();
                UpError::Exec(errmsg)
            })?;

            if self
                .was_handled
                .set(if *downloaded {
                    GithubReleaseHandled::Handled
                } else {
                    GithubReleaseHandled::Noop
                })
                .is_err()
            {
                unreachable!("failed to set was_handled");
            }
        }

        download_release
    }

    fn list_releases(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<GithubReleases, UpError> {
        let cached_releases = if options.read_cache {
            let cache = GithubReleaseOperationCache::get();
            if let Some(releases) = cache.get_releases(&self.repository) {
                let releases = releases.clone();
                let config = global_config();
                let expire = config.cache.github_release.versions_expire;
                if !releases.is_stale(expire) {
                    progress_handler.progress("using cached release list".light_black());
                    return Ok(releases);
                }
                Some(releases)
            } else {
                None
            }
        } else {
            None
        };

        progress_handler.progress("refreshing releases list from GitHub".to_string());
        match self.list_releases_from_api(progress_handler) {
            Ok(releases) => {
                if options.write_cache {
                    progress_handler.progress("updating cache with release list".to_string());
                    if let Err(err) = GithubReleaseOperationCache::exclusive(|ghrelease| {
                        ghrelease.add_releases(&self.repository, &releases)
                    }) {
                        progress_handler.progress(format!("failed to update cache: {}", err));
                    }
                }

                Ok(releases)
            }
            Err(err) => {
                if let Some(cached_releases) = cached_releases {
                    progress_handler.progress(format!(
                        "{}; {}",
                        format!("error refreshing release list: {}", err).red(),
                        "using cached data".light_black()
                    ));
                    Ok(cached_releases)
                } else {
                    Err(err)
                }
            }
        }
    }

    fn get_auth_token(&self, progress_handler: &UpProgressHandler) -> Option<String> {
        // TODO: allow to fetch PAT from config too, so someone
        // could configure it if they don't have `gh` installed
        if which::which("gh").is_err() {
            return None;
        }

        progress_handler.progress(format!(
            "preparing to get auth token from {}",
            "gh".light_yellow()
        ));

        let hostname = if let Some(api_url) = &self.api_url {
            match url::Url::parse(api_url) {
                Ok(url) => url.host_str().unwrap_or(api_url).to_string(),
                Err(err) => {
                    progress_handler.progress(format!("failed to parse URL: {}", err));

                    api_url.clone()
                }
            }
        } else {
            "github.com".to_string()
        };

        progress_handler.progress(format!(
            "getting auth token from {} for hostname {}",
            "gh".light_yellow(),
            hostname.light_yellow()
        ));

        let mut gh_auth_token = ProcessCommand::new("gh");
        gh_auth_token.arg("auth");
        gh_auth_token.arg("token");
        gh_auth_token.arg("--hostname");
        gh_auth_token.arg(hostname);
        gh_auth_token.stdout(std::process::Stdio::piped());
        gh_auth_token.stderr(std::process::Stdio::piped());

        let output = gh_auth_token.output().ok()?;

        if !output.status.success() {
            progress_handler.progress(format!(
                "failed to get auth token: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
            return None;
        }

        progress_handler.progress("auth token retrieved".to_string());

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(token)
    }

    fn list_releases_from_api(
        &self,
        progress_handler: &UpProgressHandler,
    ) -> Result<GithubReleases, UpError> {
        // Use https://api.github.com/repos/<owner>/<repo>/releases to
        // list the available releases
        let api_url = self
            .api_url
            .clone()
            .unwrap_or("https://api.github.com".to_string());
        let releases_url = format!("{}/repos/{}/releases", api_url, self.repository);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            reqwest::header::HeaderValue::from_static("2022-11-28"),
        );

        if let Some(token) = self.get_auth_token(progress_handler) {
            match reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)) {
                Ok(header_value) => {
                    headers.insert(reqwest::header::AUTHORIZATION, header_value);
                }
                Err(err) => {
                    progress_handler.progress(format!("failed to set auth token: {}", err));
                }
            }
        }

        let client = match reqwest::blocking::Client::builder()
            .user_agent(format!("omni {}", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .build()
        {
            Ok(client) => client,
            Err(err) => {
                let errmsg = format!("failed to create client: {}", err);
                progress_handler.error_with_message(errmsg.clone());
                return Err(UpError::Exec(errmsg));
            }
        };

        let response = client.get(releases_url).send().map_err(|err| {
            let errmsg = format!("failed to get releases: {}", err);
            progress_handler.error_with_message(errmsg.clone());
            UpError::Exec(errmsg)
        })?;

        let status = response.status();
        let contents = response.text().map_err(|err| {
            let errmsg = format!("failed to read response: {}", err);
            progress_handler.error_with_message(errmsg.clone());
            UpError::Exec(errmsg)
        })?;

        if !status.is_success() {
            // Try parsing the error message from the body, and default to
            // the body if we can't parse it
            let errmsg = match GithubApiError::from_json(&contents) {
                Ok(gherr) => gherr.message,
                Err(_) => contents.clone(),
            };

            let errmsg = format!("{} ({})", errmsg, status);
            progress_handler.error_with_message(errmsg.to_string());
            return Err(UpError::Exec(errmsg));
        }

        let releases = GithubReleases::from_json(&contents).map_err(|err| {
            let errmsg = format!("failed to parse releases: {}", err);
            progress_handler.error_with_message(errmsg.clone());
            UpError::Exec(errmsg)
        })?;

        Ok(releases)
    }

    fn resolve_release(&self, releases: &GithubReleases) -> Result<GithubReleaseVersion, UpError> {
        let version = self.version.clone().unwrap_or_else(|| "latest".to_string());

        let (_version, release) = releases
            .get(&version, self.prerelease, self.build, self.binary)
            .ok_or_else(|| {
                let errmsg = format!(
                    "no matching release found for {} {}",
                    self.repository, version,
                );
                UpError::Exec(errmsg)
            })?;

        Ok(release)
    }

    fn list_installed_versions(
        &self,
        _progress_handler: &dyn ProgressHandler,
    ) -> Result<Vec<String>, UpError> {
        let release_path = github_releases_bin_path().join(&self.repository);

        if !release_path.exists() {
            return Ok(vec![]);
        }

        let installed_versions = std::fs::read_dir(&release_path)
            .map_err(|err| {
                let errmsg = format!("failed to read directory: {}", err);
                UpError::Exec(errmsg)
            })?
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    if entry.file_type().ok()?.is_dir() {
                        entry.file_name().into_string().ok()
                    } else {
                        None
                    }
                })
            })
            .collect();

        Ok(installed_versions)
    }

    fn resolve_version(&self, versions: &[String]) -> Result<String, UpError> {
        let match_version = self.version.clone().unwrap_or_else(|| "latest".to_string());
        let mut matcher = VersionMatcher::new(&match_version);
        matcher.prerelease(self.prerelease);
        matcher.build(self.build);
        matcher.prefix(true);

        let version = versions
            .iter()
            .filter_map(|version| VersionParser::parse(version))
            .sorted()
            .rev()
            .find(|version| matcher.matches(&version.to_string()))
            .ok_or_else(|| {
                UpError::Exec(format!(
                    "no matching release found for {} {}",
                    self.repository, match_version,
                ))
            })?;

        Ok(version.to_string())
    }

    fn release_version_path(&self, version: &str) -> PathBuf {
        github_releases_bin_path()
            .join(&self.repository)
            .join(version)
    }

    fn download_release(
        &self,
        options: &UpOptions,
        release: &GithubReleaseVersion,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<bool, UpError> {
        let version = release.version();
        let install_path = self.release_version_path(&version);

        if options.read_cache && install_path.exists() && install_path.is_dir() {
            progress_handler.progress(
                format!("downloaded {} {} (cached)", self.repository, version).light_black(),
            );

            return Ok(false);
        }

        // Make a temporary directory to download the release
        let tmp_dir = tempfile::Builder::new()
            .prefix("omni_download.")
            .tempdir()
            .map_err(|err| {
                progress_handler.error_with_message(format!("failed to create temp dir: {}", err));
                UpError::Exec(format!("failed to create temp dir: {}", err))
            })?;

        // Go over each of the assets that matched the current platform
        // and download them all
        let mut binary_found = false;
        for asset in &release.assets {
            let asset_name = asset.name.clone();
            let asset_url = asset.browser_download_url.clone();
            let asset_path = tmp_dir.path().join(&asset_name);

            progress_handler.progress(format!("downloading {}", asset_name.light_yellow()));

            // Download the asset
            let mut response = reqwest::blocking::get(&asset_url).map_err(|err| {
                let errmsg = format!("failed to download {}: {}", asset_name, err);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;

            // Write the file to disk
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(asset_path.clone())
                .map_err(|err| {
                    let errmsg = format!("failed to open {}: {}", asset_name, err);
                    progress_handler.error_with_message(errmsg.clone());
                    UpError::Exec(errmsg)
                })?;

            io::copy(&mut response, &mut file).map_err(|err| {
                let errmsg = format!("failed to write {}: {}", asset_name, err);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;

            // Get the parsed asset name
            let (asset_type, target_dir) = asset.file_type().ok_or_else(|| {
                let errmsg = format!("file type not supported: {}", asset_name);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;
            let target_dir = tmp_dir.path().join(target_dir);

            if asset_type.is_binary() {
                // Make the binary executable
                let mut perms = file
                    .metadata()
                    .map_err(|err| {
                        let errmsg = format!("failed to get metadata for {}: {}", asset_name, err);
                        progress_handler.error_with_message(errmsg.clone());
                        UpError::Exec(errmsg)
                    })?
                    .permissions();
                perms.set_mode(0o755);
                file.set_permissions(perms).map_err(|err| {
                    let errmsg = format!("failed to set permissions for {}: {}", asset_name, err);
                    progress_handler.error_with_message(errmsg.clone());
                    UpError::Exec(errmsg)
                })?;

                // Rename the file to get rid of the os, architecture
                // and version information
                let new_path = tmp_dir.path().join(asset.clean_name(&version));
                std::fs::rename(&asset_path, &new_path).map_err(|err| {
                    let errmsg = format!("failed to rename {}: {}", asset_name, err);
                    progress_handler.error_with_message(errmsg.clone());
                    UpError::Exec(errmsg)
                })?;
            } else {
                progress_handler.progress(format!("extracting {}", asset_name.light_yellow()));

                // Open the downloaded file
                let archive_file = std::fs::File::open(&asset_path).map_err(|err| {
                    let errmsg = format!("failed to open {}: {}", asset_name, err);
                    progress_handler.error_with_message(errmsg.clone());
                    UpError::Exec(errmsg)
                })?;

                // Perform the extraction
                if asset_type.is_zip() {
                    zip_extract::extract(&archive_file, &target_dir, true).map_err(|err| {
                        let errmsg = format!("failed to extract {}: {}", asset_name, err);
                        progress_handler.error_with_message(errmsg.clone());
                        UpError::Exec(errmsg)
                    })?;
                } else if asset_type.is_tgz() {
                    let tar = flate2::read::GzDecoder::new(archive_file);
                    let mut archive = tar::Archive::new(tar);
                    archive.unpack(&target_dir).map_err(|err| {
                        let errmsg = format!("failed to extract {}: {}", asset_name, err);
                        progress_handler.error_with_message(errmsg.clone());
                        UpError::Exec(errmsg)
                    })?;
                } else {
                    let errmsg = format!("file extension not supported: {}", asset_name);
                    progress_handler.error_with_message(errmsg.clone());
                    return Err(UpError::Exec(errmsg));
                }
            }
        }

        // Locate the binary file(s) in the extracted directory, recursively
        // and move them to the workdir data path
        for entry in walkdir::WalkDir::new(tmp_dir.path())
            .into_iter()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let entry_path = entry.path();
                if entry_path.is_file() {
                    let metadata = entry.metadata().ok()?;
                    let is_executable = metadata.permissions().mode() & 0o111 != 0;
                    if is_executable {
                        Some(entry)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        {
            let source_path = entry.path();
            let binary_name = source_path
                .file_name()
                .unwrap_or(source_path.as_os_str())
                .to_string_lossy()
                .to_string();

            progress_handler.progress(format!("found binary {}", binary_name.light_yellow()));

            let target_path = install_path.join(&binary_name);

            // Make sure the target directory exists
            if !install_path.exists() {
                std::fs::create_dir_all(&install_path).map_err(|err| {
                    let errmsg = format!("failed to create {}: {}", install_path.display(), err);
                    progress_handler.error_with_message(errmsg.clone());
                    UpError::Exec(errmsg)
                })?;
            }

            // Copy the binary to the install path
            std::fs::copy(source_path, target_path).map_err(|err| {
                let errmsg = format!("failed to copy {}: {}", binary_name, err);
                progress_handler.error_with_message(errmsg.clone());

                // Force delete the install path if we fail to copy
                // the binary to avoid leaving a partial installation
                // behind
                let _ = force_remove_dir_all(&install_path);

                UpError::Exec(errmsg)
            })?;

            binary_found = true;
        }

        if !binary_found {
            progress_handler
                .error_with_message(format!("no binaries found in {}", self.repository));
            return Err(UpError::Exec("no binaries found".to_string()));
        }

        progress_handler.progress(format!(
            "downloaded {} {}",
            self.repository.light_yellow(),
            version.light_yellow()
        ));

        Ok(true)
    }

    fn handling(&self) -> GithubReleaseHandled {
        match self.was_handled.get() {
            Some(handled) => handled.clone(),
            None => GithubReleaseHandled::Unhandled,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GithubApiError {
    message: String,
    documentation_url: String,
}

impl GithubApiError {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
