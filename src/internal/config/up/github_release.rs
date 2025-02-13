use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use itertools::Itertools;
use md5::Md5;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use sha1::Sha1;
use sha2::Digest;
use sha2::Sha256;
use sha2::Sha384;
use sha2::Sha512;

use crate::internal::cache::github_release::GithubReleaseAsset;
use crate::internal::cache::github_release::GithubReleasesSelector;
use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils as cache_utils;
use crate::internal::cache::GithubReleaseOperationCache;
use crate::internal::cache::GithubReleaseVersion;
use crate::internal::cache::GithubReleases;
use crate::internal::config;
use crate::internal::config::global_config;
use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::GithubAuthConfig;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::directory::safe_rename;
use crate::internal::config::up::utils::force_remove_dir_all;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::config::up::utils::VersionParser;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::utils::check_allowed;
use crate::internal::config::ConfigValue;
use crate::internal::env::data_home;
use crate::internal::self_updater::current_arch;
use crate::internal::self_updater::current_os;
use crate::internal::user_interface::StringColor;

const GITHUB_API_URL: &str = "https://api.github.com";

cfg_if::cfg_if! {
    if #[cfg(test)] {
        fn github_releases_bin_path() -> PathBuf {
            PathBuf::from(data_home()).join("ghreleases")
        }

        fn get_github_token(_key: &str) -> Option<Option<String>> {
            None
        }

        fn set_github_token(_key: &str, _value: Option<String>) {}
    } else {
        use std::sync::Mutex;
        use once_cell::sync::Lazy;

        static GITHUB_RELEASES_BIN_PATH: Lazy<PathBuf> = Lazy::new(|| PathBuf::from(data_home()).join("ghreleases"));

        fn github_releases_bin_path() -> PathBuf {
            GITHUB_RELEASES_BIN_PATH.clone()
        }

        /// The tokens read from the `gh` command for the given `hostname` and `user`;
        /// We want to be able to update that during the runtime in a safe way
        static GITHUB_TOKENS: Lazy<Mutex<HashMap<String, Option<String>>>> =
            Lazy::new(|| Mutex::new(HashMap::new()));

        #[inline]
        fn get_github_token(key: &str) -> Option<Option<String>> {
            GITHUB_TOKENS.lock().expect("failed to lock github tokens").get(key).cloned()
        }

        #[inline]
        fn set_github_token(key: &str, value: Option<String>) {
            GITHUB_TOKENS.lock().expect("failed to lock github tokens").insert(key.to_string(), value);
        }
    }
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
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => {
                error_handler.error(ConfigErrorKind::EmptyKey);
                return Self::default();
            }
        };

        if config_value.as_str_forced().is_some() {
            return Self {
                releases: vec![UpConfigGithubRelease::from_config_value(
                    Some(config_value),
                    error_handler,
                )],
            };
        }

        if let Some(array) = config_value.as_array() {
            return Self {
                releases: array
                    .iter()
                    .enumerate()
                    .map(|(idx, config_value)| {
                        UpConfigGithubRelease::from_config_value(
                            Some(config_value),
                            &error_handler.with_index(idx),
                        )
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
                    releases: vec![UpConfigGithubRelease::from_config_value(
                        Some(config_value),
                        error_handler,
                    )],
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
                releases.push(UpConfigGithubRelease::from_table(
                    &repo_config,
                    &error_handler.with_key(repo),
                ));
            }

            if releases.is_empty() {
                error_handler.error(ConfigErrorKind::EmptyKey);
            }

            return Self { releases };
        }

        error_handler
            .with_expected(vec!["string", "array", "table"])
            .with_actual(config_value)
            .error(ConfigErrorKind::InvalidValueType);

        Self::default()
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        if self.releases.len() == 1 {
            return self.releases[0].up(options, environment, progress_handler);
        }

        progress_handler.init("github releases:".light_blue());
        if self.releases.is_empty() {
            progress_handler.error_with_message("no release information".to_string());
            return Err(UpError::Config("at least one release required".to_string()));
        }

        if !global_config()
            .up_command
            .operations
            .is_operation_allowed("github-release")
        {
            let errmsg = "github-release operation is not allowed".to_string();
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
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
            release
                .up(options, environment, &subhandler)
                .inspect_err(|_err| {
                    progress_handler.error();
                })?;
        }

        progress_handler.success_with_message(self.get_up_message());

        Ok(())
    }

    pub fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        for release in &self.releases {
            if release.was_upped() {
                release.commit(options, env_version_id)?;
            }
        }

        Ok(())
    }

    pub fn was_upped(&self) -> bool {
        self.releases.iter().any(|release| release.was_upped())
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
        progress_handler.init("github releases:".light_blue());

        let cache = GithubReleaseOperationCache::get();

        // Cleanup removable releases from the database
        cache.cleanup().map_err(|err| {
            progress_handler.progress(format!("failed to cleanup github releases cache: {}", err));
            UpError::Cache(format!("failed to cleanup github releases cache: {}", err))
        })?;

        // List releases that should exist
        let expected_releases = cache.list_installed().map_err(|err| {
            progress_handler.progress(format!("failed to list installed github releases: {}", err));
            UpError::Cache(format!("failed to list installed github releases: {}", err))
        })?;

        let expected_paths = expected_releases
            .iter()
            .map(|install| {
                github_releases_bin_path()
                    .join(&install.repository)
                    .join(&install.version)
            })
            .collect::<Vec<PathBuf>>();

        let (root_removed, num_removed, removed_paths) = cleanup_path(
            github_releases_bin_path(),
            expected_paths,
            progress_handler,
            true,
        )?;

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
pub struct UpConfigGithubRelease {
    /// The repository to install the tool from, should
    /// be in the format `owner/repo`
    pub repository: String,

    /// The version of the tool to install
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Whether to always upgrade the tool or use the latest matching
    /// already installed version.
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    pub upgrade: bool,

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

    /// The name of the asset to download from the release. All
    /// assets matching this pattern _and_ the current platform
    /// and architecture will be downloaded. It can take glob
    /// patterns, e.g. `*.tar.gz` or `special-asset-*`. If not
    /// set, will be similar as being set to `*`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_name: Vec<AssetNameMatcher>,

    /// Whether to skip the OS matching when downloading the
    /// release. This is useful when the release is not
    /// platform-specific and you want to download it anyway.
    #[serde(
        default = "cache_utils::set_false",
        skip_serializing_if = "cache_utils::is_false"
    )]
    pub skip_os_matching: bool,

    /// Whether to skip the architecture matching when downloading
    /// the release. This is useful when the release is not
    /// architecture-specific and you want to download it anyway.
    #[serde(
        default = "cache_utils::set_false",
        skip_serializing_if = "cache_utils::is_false"
    )]
    pub skip_arch_matching: bool,

    /// Whether to prefer the 'dist' assets over the 'bin' assets.
    /// This default to false as 'dist' assets are heavier
    #[serde(
        default = "cache_utils::set_false",
        skip_serializing_if = "cache_utils::is_false"
    )]
    pub prefer_dist: bool,

    /// The URL of the GitHub API; this is only required if downloading
    /// using Github Enterprise. By default, this is set to the public
    /// GitHub API URL (https://api.github.com). If you are using
    /// Github Enterprise, you should set this to the URL of your
    /// Github Enterprise instance (e.g. https://github.example.com/api/v3)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,

    /// The checksum configuration for the downloaded release
    /// assets. This is useful to ensure that the downloaded
    /// assets are not tampered with.
    #[serde(
        default,
        skip_serializing_if = "GithubReleaseChecksumConfig::is_default"
    )]
    pub checksum: GithubReleaseChecksumConfig,

    /// The authentication configuration for this specific
    /// github release. This will override the global
    /// authentication configuration, and the default behavior
    #[serde(
        default,
        with = "serde_yaml::with::singleton_map",
        skip_serializing_if = "GithubAuthConfig::is_default"
    )]
    pub auth: GithubAuthConfig,

    #[serde(default, skip)]
    actual_version: OnceCell<String>,

    #[serde(default, skip)]
    was_handled: OnceCell<GithubReleaseHandled>,
}

impl Default for UpConfigGithubRelease {
    fn default() -> Self {
        UpConfigGithubRelease {
            repository: "".to_string(),
            version: None,
            upgrade: false,
            prerelease: false,
            build: false,
            binary: true,
            asset_name: vec![],
            skip_os_matching: false,
            skip_arch_matching: false,
            prefer_dist: false,
            api_url: None,
            checksum: GithubReleaseChecksumConfig::default(),
            auth: GithubAuthConfig::default(),
            actual_version: OnceCell::new(),
            was_handled: OnceCell::new(),
        }
    }
}

impl UpConfigGithubRelease {
    pub fn new_with_version(repository: &str, version: &str) -> Self {
        Self {
            repository: repository.to_string(),
            version: Some(version.to_string()),
            upgrade: true,
            ..UpConfigGithubRelease::default()
        }
    }

    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        if let Some(table) = config_value.as_table() {
            Self::from_table(&table, error_handler)
        } else if let Some(repository) = config_value.as_str_forced() {
            Self {
                repository,
                ..Self::default()
            }
        } else {
            error_handler
                .with_expected("string or table")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);
            Self::default()
        }
    }

    fn from_table(
        table: &HashMap<String, ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = ConfigValue::from_table(table.clone());

        let repository = ["repository", "repo"]
            .iter()
            .find_map(|key| table.get(*key));

        let repository = match repository {
            Some(repository) => repository,
            None => {
                if table.len() == 1 {
                    let (key, value) = table.iter().next().unwrap();
                    if let Some(version) = value.as_str_forced() {
                        return Self {
                            repository: key.clone(),
                            version: Some(version.to_string()),
                            ..Self::default()
                        };
                    } else if let (Some(table), Ok(repo_config_value)) =
                        (value.as_table(), ConfigValue::from_str(key))
                    {
                        let mut repo_config = table.clone();
                        repo_config.insert("repository".to_string(), repo_config_value);
                        return Self::from_table(&repo_config, error_handler);
                    } else if let (true, Ok(repo_config_value)) =
                        (value.is_null(), ConfigValue::from_str(key))
                    {
                        let repo_config =
                            HashMap::from_iter(vec![("repository".to_string(), repo_config_value)]);
                        return Self::from_table(&repo_config, error_handler);
                    }
                }

                error_handler
                    .with_key("repository")
                    .error(ConfigErrorKind::MissingKey);

                return Self::default();
            }
        };

        let repository = if repository.is_table() {
            let owner = repository
                .get_as_str_or_none(
                    "owner",
                    &error_handler.with_key("repository").with_key("owner"),
                )
                .unwrap_or_else(|| {
                    error_handler
                        .with_key("repository")
                        .with_key("owner")
                        .error(ConfigErrorKind::MissingKey);

                    "".to_string()
                });

            let name = repository
                .get_as_str_or_none(
                    "name",
                    &error_handler.with_key("repository").with_key("name"),
                )
                .unwrap_or_else(|| {
                    error_handler
                        .with_key("repository")
                        .with_key("name")
                        .error(ConfigErrorKind::MissingKey);

                    "".to_string()
                });

            format!("{}/{}", owner, name)
        } else if let Some(repository) = repository.as_str_forced() {
            repository.to_string()
        } else {
            error_handler
                .with_key("repository")
                .with_expected("string or table")
                .with_actual(repository)
                .error(ConfigErrorKind::InvalidValueType);

            "".to_string()
        };

        if repository.is_empty() {
            error_handler
                .with_key("repository")
                .error(ConfigErrorKind::EmptyKey);
        }

        let version =
            config_value.get_as_str_or_none("version", &error_handler.with_key("version"));
        let upgrade = config_value.get_as_bool_or_default(
            "upgrade",
            false,
            &error_handler.with_key("upgrade"),
        );
        let prerelease = config_value.get_as_bool_or_default(
            "prerelease",
            false,
            &error_handler.with_key("prerelease"),
        );
        let build =
            config_value.get_as_bool_or_default("build", false, &error_handler.with_key("build"));
        let binary =
            config_value.get_as_bool_or_default("binary", true, &error_handler.with_key("binary"));
        let asset_name = AssetNameMatcher::from_config_value_multi(
            table.get("asset_name"),
            &error_handler.with_key("asset_name"),
        );
        let skip_os_matching = config_value.get_as_bool_or_default(
            "skip_os_matching",
            false,
            &error_handler.with_key("skip_os_matching"),
        );
        let skip_arch_matching = config_value.get_as_bool_or_default(
            "skip_arch_matching",
            false,
            &error_handler.with_key("skip_arch_matching"),
        );
        let prefer_dist = config_value.get_as_bool_or_default(
            "prefer_dist",
            false,
            &error_handler.with_key("prefer_dist"),
        );
        let api_url =
            config_value.get_as_str_or_none("api_url", &error_handler.with_key("api_url"));
        let checksum = GithubReleaseChecksumConfig::from_config_value(
            table.get("checksum"),
            &error_handler.with_key("checksum"),
        );
        let auth = GithubAuthConfig::from_config_value(
            table.get("auth").cloned(),
            &error_handler.with_key("auth"),
        );

        UpConfigGithubRelease {
            repository,
            version,
            upgrade,
            prerelease,
            build,
            binary,
            asset_name,
            skip_os_matching,
            skip_arch_matching,
            prefer_dist,
            api_url,
            checksum,
            auth,
            ..UpConfigGithubRelease::default()
        }
    }

    fn update_cache(
        &self,
        _options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &dyn ProgressHandler,
    ) {
        let version = match self.version() {
            Ok(version) => self.version_with_config(&version),
            Err(err) => {
                progress_handler.error_with_message(err.message());
                return;
            }
        };

        progress_handler.progress("updating cache".to_string());

        if let Err(err) =
            GithubReleaseOperationCache::get().add_installed(&self.repository, &version)
        {
            progress_handler.progress(format!("failed to update github release cache: {}", err));
            return;
        }

        let release_version_path = self.release_version_path(&version);
        environment.add_path(release_version_path);

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
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.init(self.desc().light_blue());
        progress_handler.progress("install github release".to_string());

        if self.repository.is_empty() {
            progress_handler.error_with_message("repository is required".to_string());
            return Err(UpError::Config("repository is required".to_string()));
        }

        if !global_config()
            .up_command
            .operations
            .is_github_repository_allowed(&self.repository)
        {
            let errmsg = format!("repository {} not allowed", self.repository);
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
        }

        let installed = self.resolve_and_download_release(options, progress_handler)?;

        self.update_cache(options, environment, progress_handler);

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

    pub fn was_upped(&self) -> bool {
        matches!(
            self.was_handled.get(),
            Some(GithubReleaseHandled::Handled) | Some(GithubReleaseHandled::Noop)
        )
    }

    pub fn version(&self) -> Result<String, UpError> {
        match self.actual_version.get() {
            Some(version) => Ok(version.to_string()),
            None => Err(UpError::Exec("version not set".to_string())),
        }
    }

    pub fn install_path(&self) -> Result<PathBuf, UpError> {
        Ok(self.release_version_path(&self.version_with_config(&self.version()?)))
    }

    pub fn commit(&self, _options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        let version = self.version_with_config(&self.version()?);
        if let Err(err) = GithubReleaseOperationCache::get().add_required_by(
            env_version_id,
            &self.repository,
            &version,
        ) {
            return Err(UpError::Cache(format!(
                "failed to update github release cache: {}",
                err
            )));
        }

        Ok(())
    }

    pub fn down(&self, _progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        Ok(())
    }

    fn upgrade_release(&self, options: &UpOptions) -> bool {
        self.upgrade || options.upgrade || config(".").up_command.upgrade
    }

    fn resolve_and_download_release(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<bool, UpError> {
        let mut version = "".to_string();
        let mut download_release = Err(UpError::Exec("did not even try".to_string()));
        let mut releases = None;

        if let Some(hash) = self.release_config_hash() {
            progress_handler.progress(format!("using configuration hash {}", hash.light_yellow()));
        }

        // If the options do not include upgrade, then we can try using
        // an already-installed version if any matches the requirements
        if !self.upgrade_release(options) {
            let resolve_str = match self.version.as_ref() {
                Some(version) if version != "latest" => version.to_string(),
                _ => {
                    let list_releases = self.list_releases(options, progress_handler)?;
                    releases = Some(list_releases.clone());
                    let latest = self.latest_release_version(&list_releases)?;
                    progress_handler.progress(
                        format!("considering installed versions matching {}", latest).light_black(),
                    );
                    latest
                }
            };

            let installed_versions = self.list_installed_versions(progress_handler)?;
            match self.resolve_version_from_str(&resolve_str, &installed_versions) {
                Ok(installed_version) => {
                    progress_handler.progress(format!(
                        "found matching installed version {}",
                        installed_version.light_yellow(),
                    ));

                    version = installed_version;
                    download_release = Ok(false);
                }
                Err(_err) => {
                    progress_handler.progress("no matching version installed".to_string());
                }
            }
        }

        if version.is_empty() {
            let releases = match releases {
                Some(releases) => releases,
                None => self.list_releases(options, progress_handler)?,
            };
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

                        self.resolve_release(&releases).inspect_err(|err| {
                            progress_handler.error_with_message(err.message());
                        })?
                    } else {
                        progress_handler.error_with_message(err.message());
                        return Err(err);
                    }
                }
            };

            version = release.version();

            // Try installing the release found
            download_release = self.download_release(options, &release, progress_handler);
            if download_release.is_err() && !options.fail_on_upgrade {
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
        let cache = GithubReleaseOperationCache::get();
        let cached_releases = if options.read_cache {
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
        match self.list_releases_from_api(progress_handler, cached_releases.as_ref()) {
            Ok(releases) => {
                if options.write_cache {
                    progress_handler.progress("updating cache with release list".to_string());
                    if let Err(err) = cache.add_releases(&self.repository, &releases) {
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

    fn get_api_hostname(&self, progress_handler: &dyn ProgressHandler) -> String {
        if let Some(api_url) = &self.api_url {
            match url::Url::parse(api_url) {
                Ok(url) => url.host_str().unwrap_or(api_url).to_string(),
                Err(err) => {
                    progress_handler.progress(format!("failed to parse URL: {}", err));

                    api_url.clone()
                }
            }
        } else {
            "github.com".to_string()
        }
    }

    fn get_auth_token(&self, progress_handler: &dyn ProgressHandler) -> Option<String> {
        let auth = if !self.auth.is_default() {
            self.auth.clone()
        } else {
            let hostname = self.get_api_hostname(progress_handler);
            global_config().github.auth_for(&self.repository, &hostname)
        };

        let (hostname, user) = match auth {
            GithubAuthConfig::Skip(true) => return None,
            GithubAuthConfig::Skip(false) => unreachable!("skip: false is not expected"),
            GithubAuthConfig::Token(token) => return Some(token),
            GithubAuthConfig::TokenEnvVar(env_var) => {
                eprintln!("using {} for auth token", env_var);
                let token = std::env::var(env_var).ok()?;
                return Some(token);
            }
            GithubAuthConfig::GhCli { hostname, user } => (hostname, user),
        };

        // If we get here, we need to use the `gh` command to get the token
        if which::which("gh").is_err() {
            return None;
        }

        let hostname = hostname.unwrap_or_else(|| self.get_api_hostname(progress_handler));

        // Try to get the token from the cache first
        let key = format!("{}/{}", hostname, user.clone().unwrap_or("".to_string()));
        if let Some(cached_token) = get_github_token(&key) {
            return cached_token;
        }

        progress_handler.progress(format!(
            "getting auth token from {} for hostname {}{}",
            "gh".light_yellow(),
            hostname.light_yellow(),
            user.clone().map_or_else(
                || "".to_string(),
                |user| format!(" and user {}", user.light_yellow())
            )
        ));

        let mut gh_auth_token = ProcessCommand::new("gh");
        gh_auth_token.arg("auth");
        gh_auth_token.arg("token");
        gh_auth_token.arg("--hostname");
        gh_auth_token.arg(hostname);
        if let Some(user) = user {
            gh_auth_token.arg("--user");
            gh_auth_token.arg(user);
        }
        gh_auth_token.stdout(std::process::Stdio::piped());
        gh_auth_token.stderr(std::process::Stdio::piped());

        let output = gh_auth_token.output().ok()?;

        if !output.status.success() {
            progress_handler.progress(format!(
                "failed to get auth token: {}",
                String::from_utf8_lossy(&output.stderr)
            ));

            set_github_token(&key, None);
            return None;
        }

        progress_handler.progress("auth token retrieved".to_string());

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Cache the token
        set_github_token(&key, Some(token.clone()));

        Some(token)
    }

    fn get_github_client(
        &self,
        progress_handler: &dyn ProgressHandler,
        json: bool,
    ) -> Result<reqwest::blocking::Client, UpError> {
        let mut headers = reqwest::header::HeaderMap::new();

        if json {
            headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
            );
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            );
        } else {
            headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/octet-stream"),
            );
        }

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

        Ok(client)
    }

    fn list_releases_from_api(
        &self,
        progress_handler: &UpProgressHandler,
        cached_releases: Option<&GithubReleases>,
    ) -> Result<GithubReleases, UpError> {
        // Use https://api.github.com/repos/<owner>/<repo>/releases to
        // list the available releases
        let api_url = self.api_url.clone().unwrap_or(GITHUB_API_URL.to_string());
        let releases_url = format!(
            "{}/repos/{}/releases?per_page=100",
            api_url, self.repository
        );

        let client = self.get_github_client(progress_handler, true)?;

        let mut releases = if let Some(releases) = cached_releases {
            // If we had cached releases, we are just trying to refresh the
            // available releases, so let's use that list as base and only
            // add the new releases to it
            releases.clone()
        } else {
            GithubReleases::new()
        };
        let mut cur_page = 1;

        loop {
            progress_handler.progress(format!("fetching releases (page {})", cur_page));

            let request_url = format!("{}&page={}", releases_url, cur_page);

            let response = client.get(&request_url).send().map_err(|err| {
                let errmsg = format!("failed to get releases (page {}): {}", cur_page, err);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;

            // Grab the response data we need before `text()` consumes the response
            let status = response.status();
            let link_header = response
                .headers()
                .get("link")
                .and_then(|link| link.to_str().ok())
                .unwrap_or("")
                .to_string();

            // Read the response body
            let contents = response.text().map_err(|err| {
                let errmsg = format!("failed to read response (page {}): {}", cur_page, err);
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

                let errmsg = format!("{}: {} ({})", releases_url, errmsg, status);
                progress_handler.error_with_message(errmsg.to_string());
                return Err(UpError::Exec(errmsg));
            }

            // Add the newly-fetched releases to the list
            let all_added = releases.add_json(&contents).map_err(|err| {
                let errmsg = format!("failed to parse releases (page {}): {}", cur_page, err);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;
            if !all_added {
                progress_handler
                    .progress("done fetching releases (all new releases fetched)".to_string());
                break;
            }

            // If we get here, we should try fetching the next page, but only if there is a next
            // page to fetch; we can know by checking if the `link` header contains a `rel="next"`
            // link; we do not need to fully parse it at this time since those links do not contain
            // any special identifiers that we need to keep track of
            if !link_header.contains(r#"rel="next""#) {
                progress_handler.progress("done fetching releases (no more pages)".to_string());
                break;
            }

            cur_page += 1;
        }

        Ok(releases)
    }

    fn resolve_release(&self, releases: &GithubReleases) -> Result<GithubReleaseVersion, UpError> {
        let match_version = self.version.clone().unwrap_or_else(|| "latest".to_string());
        self.resolve_release_from_str(&match_version, releases)
    }

    fn latest_release_version(&self, releases: &GithubReleases) -> Result<String, UpError> {
        let latest = self.resolve_release_from_str("latest", releases)?;
        let version_str = latest.version();
        Ok(VersionParser::parse(&version_str)
            .expect("failed to parse version string")
            .major()
            .to_string())
    }

    fn resolve_release_from_str(
        &self,
        match_version: &str,
        releases: &GithubReleases,
    ) -> Result<GithubReleaseVersion, UpError> {
        let (_version, release) = releases
            .get(
                GithubReleasesSelector::new(match_version)
                    .prerelease(self.prerelease)
                    .build(self.build)
                    .binary(self.binary)
                    .asset_name_matchers(self.asset_name.clone())
                    .skip_os_matching(self.skip_os_matching)
                    .skip_arch_matching(self.skip_arch_matching)
                    .prefer_dist(self.prefer_dist)
                    .checksum_lookup(self.checksum.is_enabled())
                    .checksum_algorithm(self.checksum.algorithm.clone().map(|a| a.to_string()))
                    .checksum_asset_name_matchers(self.checksum.asset_name.clone()),
            )
            .ok_or_else(|| {
                let errmsg = format!(
                    "no matching release found for {} {}",
                    self.repository, match_version,
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

        let installed_versions: Vec<_> = std::fs::read_dir(&release_path)
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

        // If we have a config hash, we should filter out the versions that
        // do not match it as they won't be the same as the expected version;
        // if we DO NOT have a config hash, we should filter out the versions
        // that DO have a config hash as they won't be the same as the expected
        // version
        let installed_versions: Vec<_> = if let Some(hash) = self.release_config_hash() {
            let ends_with = format!("~{}", hash);
            installed_versions
                .into_iter()
                .filter_map(|version| Some(version.strip_suffix(&ends_with)?.to_string()))
                .collect()
        } else {
            // We want to remove all versions that end with ~ followed
            // by a sha256 hash of 8 characters, as this will indicate
            // that the version was installed with a specific configuration
            // that could influence the assets being installed

            installed_versions
                .into_iter()
                .filter(|version| {
                    // If the version has less characters than the hash
                    // length, we should keep it as it is not a hash
                    let len = version.len();
                    if len < 9 {
                        return true;
                    }

                    // Check for the `~` character, which should be 9 characters
                    // from the end of the string
                    let tilde = &version[len - 9..len - 8];
                    if tilde != "~" {
                        return true;
                    }

                    // Now check all characters after the tilde are hex digits
                    let hash = &version[len - 8..];
                    !hash.chars().all(|c| c.is_ascii_hexdigit())
                })
                .collect()
        };

        Ok(installed_versions)
    }

    fn resolve_version(&self, versions: &[String]) -> Result<String, UpError> {
        let match_version = self.version.clone().unwrap_or_else(|| "latest".to_string());
        self.resolve_version_from_str(&match_version, versions)
    }

    fn resolve_version_from_str(
        &self,
        match_version: &str,
        versions: &[String],
    ) -> Result<String, UpError> {
        let mut matcher = VersionMatcher::new(match_version);
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

    /// Returns a hash that represents the configuration that could
    /// influence the assets being installed from the release. This
    /// hash is used to differentiate between two identical releases
    /// at the same version that had different configurations leading
    /// to different assets being installed for it.
    fn release_config_hash(&self) -> Option<String> {
        // If there is none of the configuration that could influence
        // the assets being installed from the release, we should not
        // generate a hash for it
        if self.asset_name.iter().all(|name| !name.any_filter())
            && self.binary
            && !self.skip_os_matching
            && !self.skip_arch_matching
        {
            return None;
        }

        let mut hasher = Sha256::new();

        for asset_name in self.asset_name.iter().filter(|name| name.any_filter()) {
            hasher.update(asset_name.hash_filter());
        }

        if !self.binary {
            hasher.update(b"disabled_binary");
        }

        if self.skip_os_matching {
            hasher.update(b"skip_os_matching");
        }

        if self.skip_arch_matching {
            hasher.update(b"skip_arch_matching");
        }

        if self.prefer_dist {
            hasher.update(b"prefer_dist");
        }

        let hash = format!("{:x}", hasher.finalize());
        let short_hash = &hash[0..8];
        Some(short_hash.to_string())
    }

    fn version_with_config(&self, version: &str) -> String {
        match self.release_config_hash() {
            Some(hash) => format!("{}~{}", version, hash),
            None => version.to_string(),
        }
    }

    fn release_path(&self) -> PathBuf {
        github_releases_bin_path().join(&self.repository)
    }

    fn release_version_path(&self, version: &str) -> PathBuf {
        self.release_path().join(version)
    }

    fn download_asset(
        &self,
        asset_name: &str,
        asset_url: &str,
        asset_path: &Path,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<std::fs::File, UpError> {
        progress_handler.progress(format!("downloading {}", asset_name.light_yellow()));

        let client = self.get_github_client(progress_handler, false)?;

        // Download the asset
        let mut response = client.get(asset_url).send().map_err(|err| {
            let errmsg = format!("failed to download {}: {}", asset_name, err);
            progress_handler.error_with_message(errmsg.clone());
            UpError::Exec(errmsg)
        })?;

        // Check if the download was successful
        let status = response.status();
        if !status.is_success() {
            let contents = response.text().map_err(|err| {
                let errmsg = format!("failed to read response: {}", err);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;

            // Try parsing the error message from the body, and default to
            // the body if we can't parse it
            let errmsg = match GithubApiError::from_json(&contents) {
                Ok(gherr) => gherr.message,
                Err(_) => contents.clone(),
            };

            let errmsg = format!("failed to download: {} ({})", errmsg, status);
            progress_handler.error_with_message(errmsg.to_string());
            return Err(UpError::Exec(errmsg));
        }

        // Write the file to disk
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(asset_path)
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

        Ok(file)
    }

    fn validate_checksum(
        &self,
        asset: &GithubReleaseAsset,
        tmp_dir_path: &Path,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<(), UpError> {
        if !self.checksum.is_enabled() {
            return Ok(());
        }

        let asset_name = asset.name.clone();
        let asset_path = tmp_dir_path.join(&asset_name);

        let checksum_value = if let Some(checksum_value) = &self.checksum.value {
            checksum_value.clone()
        } else if let Some(checksum_asset) = &asset.checksum_asset {
            let checksum_asset_name = checksum_asset.name.clone();
            let checksum_asset_path = tmp_dir_path.join(&checksum_asset_name);

            // Download the checksum assets but only if it does not exist
            if !checksum_asset_path.exists() {
                self.download_asset(
                    &checksum_asset_name,
                    &checksum_asset.url,
                    &checksum_asset_path,
                    progress_handler,
                )?;
            }

            // Find the checksum value from the file, either by finding a line
            // in the file that ends with the asset name, preceeded by spaces or
            // tabs, or if not found, if the file only contains a unique hash
            let checksum_file = std::fs::File::open(&checksum_asset_path).map_err(|err| {
                let errmsg = format!("failed to open {}: {}", checksum_asset_name, err);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;
            let checksum_reader = BufReader::new(checksum_file);

            let mut file_lines = 0;
            let mut unique_hash = None;
            let mut matching_hash = None;
            for line in checksum_reader.lines().map_while(|line| line.ok()) {
                file_lines += 1;
                let trim_line = line.trim();
                if trim_line.ends_with(format!(" {}", asset_name).as_str())
                    || trim_line.ends_with(format!("\t{}", asset_name).as_str())
                {
                    matching_hash = Some(trim_line.split_whitespace().next().unwrap().to_string());
                    break;
                } else if !trim_line.contains(' ') && !trim_line.contains('\t') {
                    unique_hash = Some(trim_line.to_string());
                }
            }

            let checksum_value = if matching_hash.is_some() {
                matching_hash
            } else if unique_hash.is_some() && file_lines == 1 {
                unique_hash
            } else {
                None
            };

            // If we had an asset file to check against, but we couldn't find
            // the checksum for that asset, raise an error since we can't validate
            // the asset
            if checksum_value.is_none() {
                let errmsg = format!(
                    "checksum not found in {}: {}",
                    checksum_asset_name, asset_name
                );
                progress_handler.error_with_message(errmsg.clone());
                return Err(UpError::Exec(errmsg));
            }

            checksum_value.unwrap()
        } else {
            return Ok(());
        };

        // If we have any value to check against, let's validate the checksum
        let checksum_algorithm = if let Some(checksum_algorithm) = &self.checksum.algorithm {
            checksum_algorithm.clone()
        } else if let Some(checksum_algorithm) =
            GithubReleaseChecksumAlgorithm::from_hash(&checksum_value)
        {
            checksum_algorithm
        } else {
            let errmsg = format!("checksum algorithm not found for {}", checksum_value);
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Exec(errmsg));
        };

        progress_handler.progress(format!(
            "validating checksum for {}",
            asset_name.light_yellow()
        ));

        let file_checksum = checksum_algorithm
            .compute_file_hash(&asset_path)
            .map_err(|err| {
                let errmsg = format!("failed to compute checksum for {}: {}", asset_name, err);
                progress_handler.error_with_message(errmsg.clone());
                UpError::Exec(errmsg)
            })?;

        if file_checksum != checksum_value {
            let errmsg = format!(
                "checksum mismatch for {}: expected {} but got {}",
                asset_name, checksum_value, file_checksum
            );
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Exec(errmsg));
        }

        Ok(())
    }

    fn download_release(
        &self,
        options: &UpOptions,
        release: &GithubReleaseVersion,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<bool, UpError> {
        let version = release.version();
        let install_path = self.release_version_path(&self.version_with_config(&version));

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
            // Raise an error if the checksum is required but no asset was
            // found to validate the checksum against
            if self.checksum.is_enabled()
                && self.checksum.is_required()
                && self.checksum.value.is_none()
                && asset.checksum_asset.is_none()
            {
                let errmsg = format!("could not find checksum file for {}", asset.name);
                progress_handler.error_with_message(errmsg.clone());
                return Err(UpError::Exec(errmsg));
            }

            let asset_name = asset.name.clone();
            let asset_url = asset.url.clone();
            let asset_path = tmp_dir.path().join(&asset_name);

            // Download the asset
            let file =
                self.download_asset(&asset_name, &asset_url, &asset_path, progress_handler)?;

            // Validate the checksum if required
            self.validate_checksum(asset, tmp_dir.path(), progress_handler)?;

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
                safe_rename(&asset_path, &new_path).map_err(|err| {
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
                } else if asset_type.is_txz() {
                    let tar = xz2::read::XzDecoder::new(archive_file);
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
            let copy = std::fs::copy(source_path, &target_path);
            if copy.is_err() || !target_path.exists() {
                let err = if let Err(err) = copy {
                    err
                } else {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "target file not found after copy".to_string(),
                    )
                };
                let errmsg = format!("failed to copy {}: {}", binary_name, err);
                progress_handler.error_with_message(errmsg.clone());

                // Force delete the install path if we fail to copy
                // the binary to avoid leaving a partial installation
                // behind
                let _ = force_remove_dir_all(&install_path);

                return Err(UpError::Exec(errmsg));
            }

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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GithubReleaseChecksumConfig {
    /// Whether checksum verification is enabled; if set to
    /// `false`, checksum verification will be skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,

    /// Whether checksum verification is required; if set to
    /// `false`, checksum verification will be best effort, while
    /// if set to `true`, failing to verify the checksum will
    /// result in an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    required: Option<bool>,

    /// The checksum algorithm to use for the downloaded release
    /// assets. This can be one of the following: `md5`, `sha1`,
    /// `sha256`, `sha384`, `sha512`. If not set, no checksum
    /// verification will be performed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    algorithm: Option<GithubReleaseChecksumAlgorithm>,

    /// The static checksum value to compare against the downloaded
    /// release assets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<String>,

    /// The name of the asset containing the checksum value to
    /// compare against the downloaded release assets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    asset_name: Vec<AssetNameMatcher>,
}

impl GithubReleaseChecksumConfig {
    pub fn is_default(&self) -> bool {
        self.enabled.is_none()
            && self.required.is_none()
            && self.algorithm.is_none()
            && self.value.is_none()
            && self.asset_name.is_empty()
    }

    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        if let Some(table) = config_value.as_table() {
            Self::from_table(&table, error_handler)
        } else if let Some(string) = config_value.as_str() {
            Self {
                value: Some(string.to_string()),
                ..Self::default()
            }
        } else {
            error_handler
                .with_expected("table or string")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            Self::default()
        }
    }

    fn from_table(
        table: &HashMap<String, ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = ConfigValue::from_table(table.clone());

        let enabled =
            config_value.get_as_bool_or_none("enabled", &error_handler.with_key("enabled"));
        let required =
            config_value.get_as_bool_or_none("required", &error_handler.with_key("required"));
        let algorithm = config_value
            .get_as_str_or_none("algorithm", &error_handler.with_key("algorithm"))
            .map(|v| GithubReleaseChecksumAlgorithm::from_str(&v))
            .unwrap_or(None);
        let value = config_value.get_as_str_or_none("value", &error_handler.with_key("value"));
        let asset_name = AssetNameMatcher::from_config_value_multi(
            config_value.get("asset_name").as_ref(),
            &error_handler.with_key("asset_name"),
        );

        GithubReleaseChecksumConfig {
            enabled,
            required,
            algorithm,
            value,
            asset_name,
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    fn is_required(&self) -> bool {
        self.required.unwrap_or(
            self.algorithm.is_some()
                || self.value.is_some()
                || self.asset_name.iter().any(|matcher| matcher.enabled()),
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum GithubReleaseChecksumAlgorithm {
    #[serde(rename = "md5")]
    Md5,
    #[serde(rename = "sha1")]
    Sha1,
    #[serde(rename = "sha256")]
    Sha256,
    #[serde(rename = "sha384")]
    Sha384,
    #[serde(rename = "sha512")]
    Sha512,
}

impl std::fmt::Display for GithubReleaseChecksumAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl GithubReleaseChecksumAlgorithm {
    pub fn from_hash(hash: &str) -> Option<Self> {
        match hash.len() {
            32 => Some(GithubReleaseChecksumAlgorithm::Md5),
            40 => Some(GithubReleaseChecksumAlgorithm::Sha1),
            64 => Some(GithubReleaseChecksumAlgorithm::Sha256),
            96 => Some(GithubReleaseChecksumAlgorithm::Sha384),
            128 => Some(GithubReleaseChecksumAlgorithm::Sha512),
            _ => None,
        }
    }

    pub fn from_str(algorithm: &str) -> Option<Self> {
        match algorithm.to_lowercase().as_str() {
            "md5" => Some(GithubReleaseChecksumAlgorithm::Md5),
            "sha1" => Some(GithubReleaseChecksumAlgorithm::Sha1),
            "sha256" => Some(GithubReleaseChecksumAlgorithm::Sha256),
            "sha384" => Some(GithubReleaseChecksumAlgorithm::Sha384),
            "sha512" => Some(GithubReleaseChecksumAlgorithm::Sha512),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            GithubReleaseChecksumAlgorithm::Md5 => "md5",
            GithubReleaseChecksumAlgorithm::Sha1 => "sha1",
            GithubReleaseChecksumAlgorithm::Sha256 => "sha256",
            GithubReleaseChecksumAlgorithm::Sha384 => "sha384",
            GithubReleaseChecksumAlgorithm::Sha512 => "sha512",
        }
    }

    pub fn compute_file_hash(&self, path: &PathBuf) -> io::Result<String> {
        match self {
            GithubReleaseChecksumAlgorithm::Md5 => {
                let mut hasher = Md5::new();
                let mut file = std::fs::File::open(path)?;
                std::io::copy(&mut file, &mut hasher)?;
                Ok(format!("{:x}", hasher.finalize()))
            }
            GithubReleaseChecksumAlgorithm::Sha1 => {
                let mut hasher = Sha1::new();
                let mut file = std::fs::File::open(path)?;
                std::io::copy(&mut file, &mut hasher)?;
                Ok(format!("{:x}", hasher.finalize()))
            }
            GithubReleaseChecksumAlgorithm::Sha256 => {
                let mut hasher = Sha256::new();
                let mut file = std::fs::File::open(path)?;
                std::io::copy(&mut file, &mut hasher)?;
                Ok(format!("{:x}", hasher.finalize()))
            }
            GithubReleaseChecksumAlgorithm::Sha384 => {
                let mut hasher = Sha384::new();
                let mut file = std::fs::File::open(path)?;
                std::io::copy(&mut file, &mut hasher)?;
                Ok(format!("{:x}", hasher.finalize()))
            }
            GithubReleaseChecksumAlgorithm::Sha512 => {
                let mut hasher = Sha512::new();
                let mut file = std::fs::File::open(path)?;
                std::io::copy(&mut file, &mut hasher)?;
                Ok(format!("{:x}", hasher.finalize()))
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AssetNameMatcher {
    /// This matcher will only match if the current OS matches the given OS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    os: Option<String>,
    /// This matcher will only match if the current architecture matches the given architecture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arch: Option<String>,
    /// This matcher will only match if the asset name matches the given value.
    patterns: Vec<String>,
    /// This is set programmatically to indicate this matcher did not match the os or architecture
    #[serde(skip)]
    disabled: bool,
}

impl AssetNameMatcher {
    fn from_config_value_multi(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Vec<Self> {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return vec![],
        };

        if let Some(string) = config_value.as_str() {
            vec![Self {
                patterns: Self::patterns_from_string(&string),
                ..Self::default()
            }]
        } else if let Some(array) = config_value.as_array() {
            array
                .iter()
                .enumerate()
                .filter_map(|(idx, value)| {
                    Self::from_config_value_unit(Some(value), &error_handler.with_index(idx))
                })
                .collect()
        } else {
            error_handler
                .with_expected(vec!["string", "array"])
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            vec![]
        }
    }

    fn from_config_value_unit(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let config_value = config_value?;

        if let Some(string) = config_value.as_str() {
            Some(Self {
                patterns: Self::patterns_from_string(&string),
                ..Self::default()
            })
        } else if let Some(array) = config_value.as_array() {
            Some(Self {
                patterns: Self::patterns_from_array(&array),
                ..Self::default()
            })
        } else if config_value.is_table() {
            let os = config_value.get_as_str_or_none("os", &error_handler.with_key("os"));
            let arch = config_value.get_as_str_or_none("arch", &error_handler.with_key("arch"));

            let patterns = if let Some(patterns) = config_value.get("patterns") {
                if let Some(string) = patterns.as_str_forced() {
                    Self::patterns_from_string(&string)
                } else if let Some(array) = patterns.as_array() {
                    Self::patterns_from_array(&array)
                } else {
                    error_handler
                        .with_key("patterns")
                        .with_expected(vec!["string", "array"])
                        .with_actual(patterns)
                        .error(ConfigErrorKind::InvalidValueType);

                    return None;
                }
            } else {
                error_handler
                    .with_key("patterns")
                    .error(ConfigErrorKind::MissingKey);

                return None;
            };

            let mut disabled = false;

            // If 'os' is set, we can ignore this filter if the current OS
            // does not match the given OS
            if let Some(ref os) = os {
                if os.to_lowercase() != current_os().to_lowercase() {
                    disabled = true;
                }
            }

            // If 'arch' is set, we can ignore this filter if the current
            // architecture does not match the given architecture
            if let Some(ref arch) = arch {
                if arch.to_lowercase() != current_arch().to_lowercase() {
                    disabled = true;
                }
            }

            Some(Self {
                os,
                arch,
                patterns,
                disabled,
            })
        } else {
            error_handler
                .with_expected(vec!["string", "array", "table"])
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            None
        }
    }

    fn patterns_from_array(array: &[ConfigValue]) -> Vec<String> {
        array
            .iter()
            .filter_map(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect()
    }

    fn patterns_from_string(string: &str) -> Vec<String> {
        string
            .split('\n')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn enabled(&self) -> bool {
        !self.disabled
    }

    fn any_filter(&self) -> bool {
        !self.disabled && !self.patterns.is_empty()
    }

    fn hash_filter(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        for pattern in &self.patterns {
            hasher.update(pattern.as_bytes());
        }
        hasher.finalize().to_vec()
    }

    pub fn matches(&self, asset_name: &str) -> bool {
        if self.disabled {
            // We do not need to check the os/arch matching since
            // the 'disabled' flag is set when those don't match
            return false;
        }

        check_allowed(asset_name, &self.patterns)
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

#[cfg(test)]
mod tests {
    use super::*;

    mod multi_from_config_value {
        use super::*;

        #[test]
        fn empty() {
            let yaml = "";
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubReleases::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.releases.len(), 0);
        }

        #[test]
        fn str() {
            let yaml = "owner/repo";
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubReleases::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.releases.len(), 1);
            assert_eq!(config.releases[0].repository, "owner/repo");
            assert_eq!(config.releases[0].version, None);
            assert!(!config.releases[0].prerelease);
            assert!(!config.releases[0].build);
            assert!(config.releases[0].binary);
            assert_eq!(config.releases[0].api_url, None);
        }

        #[test]
        fn object_single() {
            let yaml = r#"{"repository": "owner/repo"}"#;
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubReleases::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.releases.len(), 1);
            assert_eq!(config.releases[0].repository, "owner/repo");
            assert_eq!(config.releases[0].version, None);
            assert!(!config.releases[0].prerelease);
            assert!(!config.releases[0].build);
            assert!(config.releases[0].binary);
            assert_eq!(config.releases[0].api_url, None);
        }

        #[test]
        fn object_multi() {
            let yaml = r#"{"owner/repo": "1.2.3", "owner2/repo2": {"version": "2.3.4", "prerelease": true, "build": true, "binary": false, "api_url": "https://gh.example.com"}, "owner3/repo3": {}}"#;
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubReleases::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.releases.len(), 3);

            assert_eq!(config.releases[0].repository, "owner/repo");
            assert_eq!(config.releases[0].version, Some("1.2.3".to_string()));
            assert!(!config.releases[0].prerelease);
            assert!(!config.releases[0].build);
            assert!(config.releases[0].binary);
            assert_eq!(config.releases[0].api_url, None);

            assert_eq!(config.releases[1].repository, "owner2/repo2");
            assert_eq!(config.releases[1].version, Some("2.3.4".to_string()));
            assert!(config.releases[1].prerelease);
            assert!(config.releases[1].build);
            assert!(!config.releases[1].binary);
            assert_eq!(
                config.releases[1].api_url,
                Some("https://gh.example.com".to_string())
            );

            assert_eq!(config.releases[2].repository, "owner3/repo3");
            assert_eq!(config.releases[2].version, None);
            assert!(!config.releases[2].prerelease);
            assert!(!config.releases[2].build);
            assert!(config.releases[2].binary);
            assert_eq!(config.releases[2].api_url, None);
        }

        #[test]
        fn list_multi() {
            let yaml = r#"["owner/repo", {"repository": "owner2/repo2", "version": "2.3.4", "prerelease": true, "build": true, "binary": false, "api_url": "https://gh.example.com"}, {"owner3/repo3": "3.4.5"}, {"owner4/repo4": {"version": "4.5.6"}}]"#;
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubReleases::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.releases.len(), 4);

            assert_eq!(config.releases[0].repository, "owner/repo");
            assert_eq!(config.releases[0].version, None);
            assert!(!config.releases[0].prerelease);
            assert!(!config.releases[0].build);
            assert!(config.releases[0].binary);
            assert_eq!(config.releases[0].api_url, None);

            assert_eq!(config.releases[1].repository, "owner2/repo2");
            assert_eq!(config.releases[1].version, Some("2.3.4".to_string()));
            assert!(config.releases[1].prerelease);
            assert!(config.releases[1].build);
            assert!(!config.releases[1].binary);
            assert_eq!(
                config.releases[1].api_url,
                Some("https://gh.example.com".to_string())
            );

            assert_eq!(config.releases[2].repository, "owner3/repo3");
            assert_eq!(config.releases[2].version, Some("3.4.5".to_string()));
            assert!(!config.releases[2].prerelease);
            assert!(!config.releases[2].build);
            assert!(config.releases[2].binary);
            assert_eq!(config.releases[2].api_url, None);

            assert_eq!(config.releases[3].repository, "owner4/repo4");
            assert_eq!(config.releases[3].version, Some("4.5.6".to_string()));
            assert!(!config.releases[3].prerelease);
            assert!(!config.releases[3].build);
            assert!(config.releases[3].binary);
            assert_eq!(config.releases[3].api_url, None);
        }
    }

    mod single_from_config_value {
        use super::*;

        #[test]
        fn str() {
            let yaml = "owner/repo";
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubRelease::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.repository, "owner/repo");
            assert_eq!(config.version, None);
            assert!(!config.prerelease);
            assert!(!config.build);
            assert!(config.binary);
            assert_eq!(config.api_url, None);
        }

        #[test]
        fn object() {
            let yaml = r#"{"repository": "owner/repo"}"#;
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubRelease::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.repository, "owner/repo");
            assert_eq!(config.version, None);
            assert!(!config.prerelease);
            assert!(!config.build);
            assert!(config.binary);
            assert_eq!(config.api_url, None);
        }

        #[test]
        fn object_repo_alias() {
            let yaml = r#"{"repo": "owner/repo"}"#;
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubRelease::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.repository, "owner/repo");
            assert_eq!(config.version, None);
            assert!(!config.prerelease);
            assert!(!config.build);
            assert!(config.binary);
            assert_eq!(config.api_url, None);
        }

        #[test]
        fn with_all_values() {
            let yaml = r#"{"repository": "owner/repo", "version": "1.2.3", "prerelease": true, "build": true, "binary": false, "api_url": "https://gh.example.com"}"#;
            let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
            let config = UpConfigGithubRelease::from_config_value(
                Some(&config_value),
                &ConfigErrorHandler::noop(),
            );
            assert_eq!(config.repository, "owner/repo");
            assert_eq!(config.version, Some("1.2.3".to_string()));
            assert!(config.prerelease);
            assert!(config.build);
            assert!(!config.binary);
            assert_eq!(config.api_url, Some("https://gh.example.com".to_string()));
        }
    }

    mod up {
        use super::*;

        use crate::internal::self_updater::compatible_release_arch;
        use crate::internal::self_updater::compatible_release_os;
        use crate::internal::testutils::run_with_env;

        #[test]
        fn latest_release_binary() {
            test_download_release(
                TestOptions::default().version("v1.2.3"),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn latest_release_binary_with_prerelease() {
            test_download_release(
                TestOptions::default().version("v2.0.0-alpha"),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    prerelease: true,
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn specific_release_binary_1_major() {
            test_download_release(
                TestOptions::default().version("v1.2.3"),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("1".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn specific_release_binary_1_1_minor() {
            test_download_release(
                TestOptions::default().version("v1.1.9"),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("1.1".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn specific_release_binary_1_2_2_full() {
            test_download_release(
                TestOptions::default().version("v1.2.2"),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("1.2.2".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn different_prefix() {
            test_download_release(
                TestOptions::default().version("prefix-1.2.0"),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("1.2.0".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn non_standard_version() {
            test_download_release(
                TestOptions::default().version("nonstandard"),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("nonstandard".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn more_than_one_asset() {
            test_download_release(
                TestOptions::default().version("twoassets").assets(2),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("twoassets".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn fails_if_binary_is_false_and_only_binaries() {
            test_download_release(
                TestOptions::default(),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: None,
                    binary: false,
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn fails_if_no_assets() {
            test_download_release(
                TestOptions::default(),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("noassets".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[test]
        fn fails_if_no_matching_assets() {
            test_download_release(
                TestOptions::default(),
                UpConfigGithubRelease {
                    repository: "owner/repo".to_string(),
                    version: Some("nomatchingassets".to_string()),
                    ..UpConfigGithubRelease::default()
                },
            );
        }

        #[derive(Default)]
        struct TestOptions {
            expected_version: Option<String>,
            assets: usize,
        }

        impl TestOptions {
            fn version(mut self, version: &str) -> Self {
                self.expected_version = Some(version.to_string());
                if self.assets == 0 {
                    self.assets = 1;
                }
                self
            }

            fn assets(mut self, assets: usize) -> Self {
                self.assets = assets;
                self
            }
        }

        fn test_download_release(test: TestOptions, config: UpConfigGithubRelease) {
            run_with_env(&[], || {
                let mut mock_server = mockito::Server::new();
                let api_url = mock_server.url();

                let config = UpConfigGithubRelease {
                    api_url: Some(api_url.to_string()),
                    ..config
                };

                let current_arch = compatible_release_arch()
                    .into_iter()
                    .next()
                    .expect("no compatible arch")
                    .into_iter()
                    .next()
                    .expect("no compatible arch");
                let current_os = compatible_release_os()
                    .into_iter()
                    .next()
                    .expect("no compatible os");

                let list_releases_body = format!(
                    r#"[
                    {{
                        "name": "Release 2.0.0-alpha",
                        "tag_name": "v2.0.0-alpha",
                        "draft": false,
                        "prerelease": true,
                        "assets": [
                            {{
                                "name": "asset1_{arch}_{os}",
                                "url": "{url}/download/v2.0.0-alpha/asset1"
                            }}
                        ]
                    }},
                    {{
                        "name": "Release 1.2.3",
                        "tag_name": "v1.2.3",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "asset1_{arch}_{os}",
                                "url": "{url}/download/v1.2.3/asset1"
                            }}
                        ]
                    }},
                    {{
                        "name": "Release 1.2.2",
                        "tag_name": "v1.2.2",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "asset1_{arch}_{os}",
                                "url": "{url}/download/v1.2.2/asset1"
                            }}
                        ]
                    }},
                    {{
                        "name": "Release 1.2.0",
                        "tag_name": "prefix-1.2.0",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "asset1_{arch}_{os}",
                                "url": "{url}/download/prefix-1.2.0/asset1"
                            }}
                        ]
                    }},
                    {{
                        "name": "Release nonstandard",
                        "tag_name": "nonstandard",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "asset1_{arch}_{os}",
                                "url": "{url}/download/nonstandard/asset1"
                            }}
                        ]
                    }},
                    {{
                        "name": "Release noassets",
                        "tag_name": "noassets",
                        "draft": false,
                        "prerelease": false,
                        "assets": []
                    }},
                    {{
                        "name": "Release nomatchingassets",
                        "tag_name": "nomatchingassets",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "asset1",
                                "url": "{url}/download/nomatchingassets/asset1"
                            }}
                        ]
                    }},
                    {{
                        "name": "Release twoassets",
                        "tag_name": "twoassets",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "asset1_{arch}_{os}",
                                "url": "{url}/download/twoassets/asset1"
                            }},
                            {{
                                "name": "asset2_{arch}_{os}",
                                "url": "{url}/download/twoassets/asset2"
                            }}
                        ]
                    }},
                    {{
                        "name": "Release 1.1.9",
                        "tag_name": "v1.1.9",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "asset1_{arch}_{os}",
                                "url": "{url}/download/v1.1.9/asset1"
                            }}
                        ]
                    }}
                ]"#,
                    url = mock_server.url(),
                    arch = current_arch,
                    os = current_os
                );

                let mock_list_releases = mock_server
                    .mock("GET", "/repos/owner/repo/releases?per_page=100&page=1")
                    .with_status(200)
                    .with_body(list_releases_body)
                    .create();

                let mock_downloads = (1..=test.assets)
                    .map(|asset_id| {
                        eprintln!("Setting up asset id {}", asset_id);
                        mock_server
                            .mock(
                                "GET",
                                format!(
                                    "/download/{}/asset{}",
                                    test.expected_version.clone().unwrap(),
                                    asset_id
                                )
                                .as_str(),
                            )
                            .with_status(200)
                            .with_body(format!("asset{} contents", asset_id))
                            .create()
                    })
                    .collect::<Vec<_>>();

                let options = UpOptions::default().cache_disabled();
                let mut environment = UpEnvironment::new();
                let progress_handler = UpProgressHandler::new(None);

                let result = config.up(&options, &mut environment, &progress_handler);

                assert!(if test.expected_version.is_some() {
                    result.is_ok()
                } else {
                    result.is_err()
                });

                // Check the mocks have been called
                mock_list_releases.assert();
                mock_downloads.iter().for_each(|mock| mock.assert());

                for asset_id in 1..=test.assets {
                    // Check the binary file exists
                    let expected_bin = github_releases_bin_path()
                        .join("owner/repo")
                        .join(test.expected_version.clone().unwrap())
                        .join(format!("asset{}", asset_id));
                    if !expected_bin.exists() {
                        // Use walkdir to print all the tree under github_releases_bin_path()
                        let tree = walkdir::WalkDir::new(github_releases_bin_path())
                            .into_iter()
                            .flatten()
                            .map(|entry| entry.path().display().to_string())
                            .collect::<Vec<String>>()
                            .join("\n");
                        panic!(
                            "binary file not found at {}\nExisting paths:\n{}",
                            expected_bin.display(),
                            tree
                        );
                    }

                    // Check the file is executable
                    let metadata = expected_bin.metadata().expect("failed to get metadata");
                    assert_eq!(
                        metadata.permissions().mode() & 0o111,
                        0o111,
                        "file is not executable"
                    );
                }
            });
        }
    }
}
