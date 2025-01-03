use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils as cache_utils;
use crate::internal::cache::CargoInstallOperationCache;
use crate::internal::cache::CargoInstallVersions;
use crate::internal::config::config;
use crate::internal::config::global_config;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::up::mise_tool_path;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::progress_handler::ProgressHandler;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::config::up::utils::VersionParser;
use crate::internal::config::up::UpConfigMise;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::env::data_home;
use crate::internal::env::tmpdir_cleanup_prefix;
use crate::internal::user_interface::StringColor;

cfg_if::cfg_if! {
    if #[cfg(test)] {
        fn cargo_install_bin_path() -> PathBuf {
            PathBuf::from(data_home()).join("cargo-install")
        }
    } else {
        use once_cell::sync::Lazy;

        static CARGO_INSTALL_BIN_PATH: Lazy<PathBuf> = Lazy::new(|| PathBuf::from(data_home()).join("cargo-install"));

        fn cargo_install_bin_path() -> PathBuf {
            CARGO_INSTALL_BIN_PATH.clone()
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct UpConfigCargoInstalls {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    crates: Vec<UpConfigCargoInstall>,
}

impl Serialize for UpConfigCargoInstalls {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.crates.len() {
            0 => serializer.serialize_none(),
            1 => serializer.serialize_newtype_struct("UpConfigCargoInstalls", &self.crates[0]),
            _ => serializer.collect_seq(self.crates.iter()),
        }
    }
}

impl UpConfigCargoInstalls {
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        if config_value.as_str_forced().is_some() {
            return Self {
                crates: vec![UpConfigCargoInstall::from_config_value(
                    Some(config_value),
                    error_key,
                    errors,
                )],
            };
        }

        if let Some(array) = config_value.as_array() {
            return Self {
                crates: array
                    .iter()
                    .enumerate()
                    .map(|(idx, config_value)| {
                        UpConfigCargoInstall::from_config_value(
                            Some(config_value),
                            &format!("{}[{}]", error_key, idx),
                            errors,
                        )
                    })
                    .collect(),
            };
        }

        if let Some(table) = config_value.as_table() {
            // Check if there is a 'crate' key, in which case it's a single
            // crate and we can just parse it and return it
            if table.contains_key("crate") {
                return Self {
                    crates: vec![UpConfigCargoInstall::from_config_value(
                        Some(config_value),
                        error_key,
                        errors,
                    )],
                };
            }

            // Otherwise, we have a table of crates, where crates are
            // the keys and the values are the configuration for the crate;
            // we want to go over them in lexico-graphical order to ensure that
            // the order is consistent
            let mut crates = Vec::new();
            for crate_name in table.keys().sorted() {
                let value = table.get(crate_name).expect("crate config not found");
                let crate_name = match ConfigValue::from_str(crate_name) {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                let mut crate_config = if let Some(table) = value.as_table() {
                    table.clone()
                } else if let Some(version) = value.as_str_forced() {
                    let mut crate_config = HashMap::new();
                    let value = match ConfigValue::from_str(&version) {
                        Ok(value) => value,
                        Err(_) => continue,
                    };
                    crate_config.insert("version".to_string(), value);
                    crate_config
                } else {
                    HashMap::new()
                };

                crate_config.insert("crate".to_string(), crate_name.clone());
                crates.push(UpConfigCargoInstall::from_table(
                    &crate_config,
                    &format!("{}.{}", error_key, crate_name),
                    errors,
                ));
            }

            return Self { crates };
        }

        UpConfigCargoInstalls::default()
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        if self.crates.len() == 1 {
            progress_handler.init(self.crates[0].desc().light_blue());
        } else {
            progress_handler.init("cargo install:".light_blue());
            if self.crates.is_empty() {
                progress_handler.error_with_message("no crate".to_string());
                return Err(UpError::Config("at least one crate required".to_string()));
            }
        }

        if !global_config()
            .up_command
            .operations
            .is_operation_allowed("cargo-install")
        {
            let errmsg = "cargo-install operation is not allowed".to_string();
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
        }

        let cargo_bin = self.get_cargo_bin(options, progress_handler)?;

        let num = self.crates.len();
        for (idx, tool) in self.crates.iter().enumerate() {
            let subhandler = if self.crates.len() == 1 {
                progress_handler
            } else {
                &progress_handler.subhandler(
                    &format!(
                        "[{current:padding$}/{total:padding$}] {tool} ",
                        current = idx + 1,
                        total = num,
                        padding = format!("{}", num).len(),
                        tool = tool.desc(),
                    )
                    .light_yellow(),
                )
            };
            tool.up(options, environment, subhandler, &cargo_bin)
                .inspect_err(|_err| {
                    progress_handler.error();
                })?;
        }

        if self.crates.len() != 1 {
            progress_handler.success_with_message(self.get_up_message());
        }

        Ok(())
    }

    fn get_cargo_bin(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<PathBuf, UpError> {
        progress_handler.progress("install dependencies".to_string());
        let rust_tool = UpConfigTool::Mise(UpConfigMise::new_any_version("rust"));

        // We create a fake environment since we do not want to add this
        // rust version as part of it, but we want to be able to use `cargo`
        // to call `cargo install`
        let mut fake_env = UpEnvironment::new();

        let subhandler = progress_handler.subhandler(&"rust: ".light_black());
        rust_tool.up(options, &mut fake_env, &subhandler)?;

        // Grab the tool from inside go_tool
        let mise = match rust_tool {
            UpConfigTool::Mise(mise) => mise,
            _ => unreachable!("rust_tool is not a mise tool"),
        };

        let installed_version = mise.version()?;
        let install_path = PathBuf::from(mise_tool_path("rust", &installed_version));
        let cargo_bin = install_path.join("cargo");

        Ok(cargo_bin)
    }

    pub fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        for tool in &self.crates {
            if tool.was_upped() {
                tool.commit(options, env_version_id)?;
            }
        }

        Ok(())
    }

    pub fn was_upped(&self) -> bool {
        self.crates.iter().any(|tool| tool.was_upped())
    }

    fn get_up_message(&self) -> String {
        let count: HashMap<CargoInstallHandled, usize> = self
            .crates
            .iter()
            .map(|tool| tool.handling())
            .fold(HashMap::new(), |mut map, item| {
                *map.entry(item).or_insert(0) += 1;
                map
            });
        let handled: Vec<String> = self
            .crates
            .iter()
            .filter_map(|tool| match tool.handling() {
                CargoInstallHandled::Handled | CargoInstallHandled::Noop => Some(format!(
                    "{}@{}",
                    tool.crate_name,
                    tool.actual_version
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

        if let Some(count) = count.get(&CargoInstallHandled::Handled) {
            numbers.push(format!("{} installed", count).green());
        }

        if let Some(count) = count.get(&CargoInstallHandled::Noop) {
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
        if self.crates.len() == 1 {
            return self.crates[0].down(progress_handler);
        }

        progress_handler.init("go install:".light_blue());
        progress_handler.progress("updating dependencies".to_string());

        let num = self.crates.len();
        for (idx, tool) in self.crates.iter().enumerate() {
            let subhandler = progress_handler.subhandler(
                &format!(
                    "[{current:padding$}/{total:padding$}] ",
                    current = idx + 1,
                    total = num,
                    padding = format!("{}", num).len(),
                )
                .light_yellow(),
            );
            tool.down(&subhandler)?;
        }

        progress_handler.success_with_message("dependencies cleaned".light_green());

        Ok(())
    }

    pub fn cleanup(progress_handler: &UpProgressHandler) -> Result<Option<String>, UpError> {
        progress_handler.init("cargo install:".light_blue());

        let cache = CargoInstallOperationCache::get();

        // Cleanup removable crates from the database
        cache.cleanup().map_err(|err| {
            let msg = format!("failed to cleanup cargo install cache: {}", err);
            progress_handler.progress(msg.clone());
            UpError::Cache(msg)
        })?;

        // List crates that should exist
        let expected_crates = cache.list_installed().map_err(|err| {
            let msg = format!("failed to list cargo-installed crates: {}", err);
            progress_handler.progress(msg.clone());
            UpError::Cache(msg)
        })?;

        let expected_paths = expected_crates
            .iter()
            .map(|install| {
                cargo_install_bin_path()
                    .join(&install.crate_name)
                    .join(&install.version)
            })
            .collect::<Vec<PathBuf>>();

        let (root_removed, num_removed, removed_paths) = cleanup_path(
            cargo_install_bin_path(),
            expected_paths,
            progress_handler,
            true,
        )?;

        if root_removed {
            return Ok(Some("removed all crates".to_string()));
        }

        if num_removed == 0 {
            return Ok(None);
        }

        // We want to go over the paths that were removed to
        // return a proper message about the go install
        // that were removed
        let removed_crates = removed_paths
            .iter()
            .filter_map(|path| {
                // Path should starts with the bin path if it is a go-install tool
                let rest_of_path = match path.strip_prefix(cargo_install_bin_path()) {
                    Ok(rest_of_path) => rest_of_path,
                    Err(_) => return None,
                };

                // Path should have 2 components after stripping the bin path:
                // the crate name (1) and the version (1)
                let parts = rest_of_path.components().collect::<Vec<_>>();
                if parts.len() > 2 {
                    return None;
                }

                let parts = parts
                    .into_iter()
                    .map(|part| part.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<String>>();

                let crate_name = parts[0].clone();
                let version = if parts.len() > 1 {
                    Some(parts[2].clone())
                } else {
                    None
                };

                Some((crate_name, version))
            })
            .collect::<Vec<_>>();

        if removed_crates.is_empty() {
            return Ok(Some(format!(
                "removed {} cargo-installed crate{}",
                num_removed.light_yellow(),
                if num_removed > 1 { "s" } else { "" }
            )));
        }

        let removed_crates = removed_crates
            .iter()
            .map(|(path, version)| match version {
                Some(version) => format!("{}@{}", path.light_yellow(), version.light_yellow()),
                None => format!("{} (all versions)", path.light_yellow(),),
            })
            .collect::<Vec<_>>();

        Ok(Some(format!("removed {}", removed_crates.join(", "))))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
pub enum CargoInstallHandled {
    Handled,
    Noop,
    Unhandled,
}

#[derive(Debug, Clone, Error)]
pub enum CargoInstallError {
    #[error("invalid crate name: {0}")]
    InvalidCrateName(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UpConfigCargoInstall {
    /// The name of the crate to install
    pub crate_name: String,

    /// The version of the crate to install
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Whether to install the exact version specified in the `version` field;
    /// if `true`, there will be no check for the available versions and the
    /// `cargo install` command will be called with the version specified;
    /// if `false`, the latest version that matches the version will be installed.
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    pub exact: bool,

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

    /// The URL of the Crates API; this is only used for testing purposes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,

    /// In case there was an error while parsing the configuration, this field
    /// will contain the error message
    #[serde(default, skip)]
    config_error: Option<String>,

    #[serde(default, skip)]
    actual_version: OnceCell<String>,

    #[serde(default, skip)]
    was_handled: OnceCell<CargoInstallHandled>,
}

impl Default for UpConfigCargoInstall {
    fn default() -> Self {
        UpConfigCargoInstall {
            crate_name: "".to_string(),
            version: None,
            exact: false,
            upgrade: false,
            prerelease: false,
            build: false,
            api_url: None,
            config_error: None,
            actual_version: OnceCell::new(),
            was_handled: OnceCell::new(),
        }
    }
}

impl UpConfigCargoInstall {
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => {
                return Self {
                    config_error: Some("no configuration provided".to_string()),
                    ..Default::default()
                }
            }
        };

        if let Some(table) = config_value.as_table() {
            Self::from_table(&table, error_key, errors)
        } else if let Some(crate_name) = config_value.as_str_forced() {
            let (crate_name, version) = match parse_cargo_crate_name(&crate_name) {
                Ok((crate_name, version)) => (crate_name, version),
                Err(err) => {
                    errors.push(ConfigErrorKind::ParsingError {
                        key: error_key.to_string(),
                        found: serde_yaml::Value::String(crate_name.to_string()),
                        error: err.to_string(),
                    });
                    return Self {
                        crate_name: crate_name.to_string(),
                        config_error: Some(err.to_string()),
                        ..Default::default()
                    };
                }
            };

            // If version is set through the path, it is exact
            let exact = version.is_some();

            UpConfigCargoInstall {
                crate_name,
                version,
                exact,
                ..UpConfigCargoInstall::default()
            }
        } else {
            Self {
                config_error: Some("no crate provided".to_string()),
                ..Default::default()
            }
        }
    }

    fn from_table(
        table: &HashMap<String, ConfigValue>,
        error_key: &str,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = ConfigValue::from_table(table.clone());

        let crate_name = match table.get("crate") {
            Some(crate_name) => {
                if let Some(crate_name) = crate_name.as_str_forced() {
                    crate_name.to_string()
                } else {
                    errors.push(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.crate", error_key),
                        expected: "string".to_string(),
                        found: serde_yaml::Value::String(crate_name.to_string()),
                    });
                    return UpConfigCargoInstall {
                        config_error: Some("crate_name must be a string".to_string()),
                        ..Default::default()
                    };
                }
            }
            None => {
                if table.len() == 1 {
                    let (key, value) = table.iter().next().unwrap();
                    if let Some(version) = value.as_str_forced() {
                        return UpConfigCargoInstall {
                            crate_name: key.clone(),
                            version: Some(version.to_string()),
                            ..UpConfigCargoInstall::default()
                        };
                    } else if let (Some(table), Ok(crate_name_config_value)) =
                        (value.as_table(), ConfigValue::from_str(key))
                    {
                        let mut crate_name_config = table.clone();
                        crate_name_config.insert("crate_name".to_string(), crate_name_config_value);
                        return UpConfigCargoInstall::from_table(
                            &crate_name_config,
                            error_key,
                            errors,
                        );
                    } else if let (true, Ok(crate_name_config_value)) =
                        (value.is_null(), ConfigValue::from_str(key))
                    {
                        let crate_name_config = HashMap::from_iter(vec![(
                            "crate".to_string(),
                            crate_name_config_value,
                        )]);
                        return UpConfigCargoInstall::from_table(
                            &crate_name_config,
                            error_key,
                            errors,
                        );
                    }
                }
                errors.push(ConfigErrorKind::NotExactlyOneKeyInTable {
                    key: error_key.to_string(),
                    found: config_value.as_serde_yaml(),
                });
                return UpConfigCargoInstall {
                    config_error: Some("crate is required".to_string()),
                    ..Default::default()
                };
            }
        };

        let (crate_name, version) = match parse_cargo_crate_name(&crate_name) {
            Ok((crate_name, version)) => (crate_name, version),
            Err(err) => {
                errors.push(ConfigErrorKind::ParsingError {
                    key: format!("{}.crate", error_key),
                    found: serde_yaml::Value::String(crate_name.to_string()),
                    error: err.to_string(),
                });
                return UpConfigCargoInstall {
                    crate_name,
                    config_error: Some(err.to_string()),
                    ..Default::default()
                };
            }
        };

        let exact = match table.get("exact") {
            Some(value) => match value.as_bool_forced() {
                Some(exact) => exact,
                None => {
                    errors.push(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.exact", error_key),
                        expected: "bool".to_string(),
                        found: value.as_serde_yaml(),
                    });
                    version.is_some()
                }
            },
            None => version.is_some(),
        };

        // If version is specified, and version is also specified in the path,
        // then we raise an error as the version should not be specified in both
        let version = match table
            .get("version")
            .map(|v| v.as_str_forced())
            .unwrap_or(None)
        {
            Some(version_field) => {
                if version.is_some() {
                    errors.push(ConfigErrorKind::UnsupportedValueInContext {
                        key: format!("{}.version", error_key),
                        found: serde_yaml::Value::String(version_field.to_string()),
                    });

                    return UpConfigCargoInstall {
                        crate_name,
                        config_error: Some(
                            "version should not be specified in both crate and version fields"
                                .to_string(),
                        ),
                        ..Default::default()
                    };
                }
                Some(version_field.to_string())
            }
            None => version,
        };

        let upgrade = config_value.get_as_bool_or_default(
            "upgrade",
            false,
            &format!("{}.upgrade", error_key),
            errors,
        );
        let prerelease = config_value.get_as_bool_or_default(
            "prerelease",
            false,
            &format!("{}.prerelease", error_key),
            errors,
        );
        let build = config_value.get_as_bool_or_default(
            "build",
            false,
            &format!("{}.build", error_key),
            errors,
        );

        UpConfigCargoInstall {
            crate_name,
            version,
            exact,
            upgrade,
            prerelease,
            build,
            ..Default::default()
        }
    }

    fn update_cache(
        &self,
        _options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &dyn ProgressHandler,
    ) {
        let version = match self.actual_version.get() {
            Some(version) => version,
            None => {
                progress_handler.error_with_message("version not set".to_string());
                return;
            }
        };

        progress_handler.progress("updating cache".to_string());

        if let Err(err) = CargoInstallOperationCache::get().add_installed(&self.crate_name, version)
        {
            progress_handler.progress(format!("failed to update github release cache: {}", err));
            return;
        }

        let version_crate_name = self.version_crate_name(version);
        environment.add_path(version_crate_name.join("bin"));

        progress_handler.progress("updated cache".to_string());
    }

    fn short_crate_name(&self) -> String {
        // Get the last, non-empty part of the crate_name
        self.crate_name
            .split('/')
            .filter(|part| !part.is_empty())
            .last()
            .unwrap_or("")
            .to_string()
    }

    fn desc(&self) -> String {
        if self.crate_name.is_empty() {
            "go install:".to_string()
        } else if self.config_error.is_some() {
            format!("{}:", self.crate_name)
        } else {
            format!(
                "{} ({}):",
                self.short_crate_name(),
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
        cargo_bin: &Path,
    ) -> Result<(), UpError> {
        progress_handler.init(self.desc().light_blue());

        if let Some(config_error) = &self.config_error {
            progress_handler.error_with_message(config_error.clone());
            return Err(UpError::Config(config_error.clone()));
        }

        if self.crate_name.is_empty() {
            progress_handler.error_with_message("crate_name is required".to_string());
            return Err(UpError::Config("crate_name is required".to_string()));
        }

        if !global_config()
            .up_command
            .operations
            .is_cargo_install_crate_allowed(&self.crate_name)
        {
            let errmsg = format!("crate {} not allowed", self.crate_name);
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
        }

        let installed = self.resolve_and_install_version(cargo_bin, options, progress_handler)?;

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
            Some(CargoInstallHandled::Handled) | Some(CargoInstallHandled::Noop)
        )
    }

    pub fn commit(&self, _options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        let version = match self.actual_version.get() {
            Some(version) => version,
            None => {
                return Err(UpError::Exec("version not set".to_string()));
            }
        };

        if let Err(err) = CargoInstallOperationCache::get().add_required_by(
            env_version_id,
            &self.crate_name,
            version,
        ) {
            return Err(UpError::Cache(format!(
                "failed to update go install cache: {}",
                err
            )));
        }

        Ok(())
    }

    pub fn down(&self, _progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        Ok(())
    }

    fn handling(&self) -> CargoInstallHandled {
        match self.was_handled.get() {
            Some(handled) => handled.clone(),
            None => CargoInstallHandled::Unhandled,
        }
    }

    fn upgrade_tool(&self, options: &UpOptions) -> bool {
        self.upgrade || options.upgrade || config(".").up_command.upgrade
    }

    fn resolve_and_install_version(
        &self,
        cargo_bin: &Path,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<bool, UpError> {
        if self.exact {
            let version = self.version.clone().unwrap_or("latest".to_string());
            if version == "latest" {
                progress_handler.error_with_message("exact version cannot be 'latest'".to_string());
                return Err(UpError::Config(
                    "exact version cannot be 'latest'".to_string(),
                ));
            }

            match self.install_version(cargo_bin, options, &version, progress_handler) {
                Ok(installed) => return self.handle_installed(&version, Ok(installed)),
                Err(err) => {
                    progress_handler.error_with_message(err.message());
                    return Err(err);
                }
            }
        }

        let mut version = "".to_string();
        let mut install_version = Err(UpError::Exec("did not even try".to_string()));
        let mut versions = None;

        // If the options do not include upgrade, then we can try using
        // an already-installed version if any matches the requirements
        if !self.upgrade_tool(options) {
            let resolve_str = match self.version.as_ref() {
                Some(version) if version != "latest" => version.to_string(),
                _ => {
                    let list_versions = self.list_versions(options, progress_handler)?;
                    versions = Some(list_versions.clone());
                    let latest = self.latest_version(&list_versions)?;
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
                    install_version = Ok(false);
                }
                Err(_err) => {
                    progress_handler.progress("no matching version installed".to_string());
                }
            }
        }

        if version.is_empty() {
            let versions = match versions {
                Some(versions) => versions,
                None => self.list_versions(options, progress_handler)?,
            };
            version = match self.resolve_version(&versions.versions) {
                Ok(version) => version,
                Err(err) => {
                    // If the versions are not fresh of now, and we failed to
                    // resolve the version to install, we should try to refresh the
                    // versions list and try again
                    if options.read_cache && !versions.is_fresh() {
                        progress_handler.progress("no matching version found in cache".to_string());

                        let versions = self.list_versions(
                            &UpOptions {
                                read_cache: false,
                                ..options.clone()
                            },
                            progress_handler,
                        )?;

                        self.resolve_version(&versions.versions)
                            .inspect_err(|err| {
                                progress_handler.error_with_message(err.message());
                            })?
                    } else {
                        progress_handler.error_with_message(err.message());
                        return Err(err);
                    }
                }
            };

            // Try installing the version found
            install_version = self.install_version(cargo_bin, options, &version, progress_handler);
            if install_version.is_err() && !options.fail_on_upgrade {
                // If we get here and there is an issue downloading the version,
                // list all installed versions and check if one of those could
                // fit the requirement, in which case we can fallback to it
                let installed_versions = self.list_installed_versions(progress_handler)?;
                match self.resolve_version(&installed_versions) {
                    Ok(installed_version) => {
                        progress_handler.progress(format!(
                            "falling back to {}@{}",
                            self.crate_name,
                            installed_version.light_yellow(),
                        ));

                        version = installed_version;
                        install_version = Ok(false);
                    }
                    Err(_err) => {}
                }
            }
        }

        self.handle_installed(&version, install_version)
    }

    fn handle_installed(
        &self,
        version: &str,
        installed: Result<bool, UpError>,
    ) -> Result<bool, UpError> {
        if let Ok(installed) = &installed {
            self.actual_version.set(version.to_string()).map_err(|_| {
                let errmsg = "failed to set actual version".to_string();
                UpError::Exec(errmsg)
            })?;

            if self
                .was_handled
                .set(if *installed {
                    CargoInstallHandled::Handled
                } else {
                    CargoInstallHandled::Noop
                })
                .is_err()
            {
                unreachable!("failed to set was_handled");
            }
        }

        installed
    }

    fn list_versions(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<CargoInstallVersions, UpError> {
        let cache = CargoInstallOperationCache::get();
        let cached_versions = if options.read_cache {
            if let Some(versions) = cache.get_versions(&self.crate_name) {
                let versions = versions.clone();
                let config = global_config();
                let expire = config.cache.go_install.versions_expire;
                if !versions.is_stale(expire) {
                    progress_handler.progress("using cached version list".light_black());
                    return Ok(versions);
                }
                Some(versions)
            } else {
                None
            }
        } else {
            None
        };

        progress_handler.progress("refreshing versions list".to_string());
        match self.list_versions_from_api(progress_handler) {
            Ok(versions) => {
                if options.write_cache {
                    progress_handler.progress("updating cache with version list".to_string());
                    if let Err(err) = cache.add_versions(&self.crate_name, &versions) {
                        progress_handler.progress(format!("failed to update cache: {}", err));
                    }
                }

                Ok(versions)
            }
            Err(err) => {
                if let Some(cached_versions) = cached_versions {
                    progress_handler.progress(format!(
                        "{}; {}",
                        format!("error refreshing version list: {}", err).red(),
                        "using cached data".light_black()
                    ));
                    Ok(cached_versions)
                } else {
                    Err(err)
                }
            }
        }
    }

    fn list_versions_from_api(
        &self,
        progress_handler: &UpProgressHandler,
    ) -> Result<CargoInstallVersions, UpError> {
        // Use https://crates.io/api/v1/crates/<crate>/versions URL
        // to list the available versions for the crate
        let api_url = self
            .api_url
            .clone()
            .unwrap_or("https://crates.io/api/v1".to_string());
        let versions_url = format!(
            "{}/crates/{}/versions",
            api_url.trim_end_matches('/'),
            self.crate_name
        );

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

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

        let response = client.get(&versions_url).send().map_err(|err| {
            let errmsg = format!("failed to get versions: {}", err);
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
            let errmsg = match CratesApiError::from_json(&contents) {
                Ok(err) => err.detail(),
                Err(_) => contents.clone(),
            };

            let errmsg = format!("{}: {} ({})", versions_url, errmsg, status);
            progress_handler.error_with_message(errmsg.to_string());
            return Err(UpError::Exec(errmsg));
        }

        let versions = CratesApiVersions::from_json(&contents).map_err(|err| {
            let errmsg = format!("failed to parse versions: {}", err);
            progress_handler.error_with_message(errmsg.clone());
            UpError::Exec(errmsg)
        })?;

        Ok(CargoInstallVersions::new(versions.versions()))
    }

    fn latest_version(&self, versions: &CargoInstallVersions) -> Result<String, UpError> {
        let latest = self.resolve_version_from_str("latest", &versions.versions)?;
        Ok(VersionParser::parse(&latest)
            .expect("failed to parse version string")
            .major()
            .to_string())
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
                    "no matching version found for {}@{}",
                    self.crate_name, match_version,
                ))
            })?;

        Ok(version.to_string())
    }

    fn list_installed_versions(
        &self,
        _progress_handler: &dyn ProgressHandler,
    ) -> Result<Vec<String>, UpError> {
        let version_crate_name = cargo_install_bin_path().join(&self.crate_name);

        if !version_crate_name.exists() {
            return Ok(vec![]);
        }

        let installed_versions = std::fs::read_dir(&version_crate_name)
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

    fn install_version(
        &self,
        cargo_bin: &Path,
        options: &UpOptions,
        version: &str,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<bool, UpError> {
        let install_path = self.version_crate_name(version);

        if options.read_cache && install_path.exists() && install_path.is_dir() {
            progress_handler.progress(
                format!("installed {}@{} (cached)", self.crate_name, version).light_black(),
            );

            return Ok(false);
        }

        // Make a temporary directory to download the release
        let tmp_dir = tempfile::Builder::new()
            .prefix(&tmpdir_cleanup_prefix("cargo-install"))
            .tempdir()
            .map_err(|err| {
                progress_handler.error_with_message(format!("failed to create temp dir: {}", err));
                UpError::Exec(format!("failed to create temp dir: {}", err))
            })?;
        let tmp_bin_path = tmp_dir.path().join("bin");

        // CARGO_HOME and RUSTUP_HOME need to be set to the cargo binary install path
        // We need to get the parent() twice to get the cargo home directory
        let cargo_home = cargo_bin
            .parent()
            .expect("cargo bin has no parent")
            .parent()
            .expect("cargo bin has no parent");

        let mut cargo_install_cmd = TokioCommand::new(cargo_bin);
        cargo_install_cmd.arg("install");
        cargo_install_cmd.arg(&self.crate_name);
        cargo_install_cmd.arg("--version");
        cargo_install_cmd.arg(version);
        cargo_install_cmd.arg("--no-track");
        cargo_install_cmd.arg("--root");
        cargo_install_cmd.arg(tmp_dir.path());
        cargo_install_cmd.arg("--bins");
        cargo_install_cmd.arg("--force");

        // Override GO environment variables to ensure that the
        // installation is done in the temporary directory
        cargo_install_cmd.env("CARGO_HOME", cargo_home);
        cargo_install_cmd.env("RUSTUP_HOME", cargo_home);
        cargo_install_cmd.env("CARGO_INSTALL_ROOT", tmp_dir.path());

        cargo_install_cmd.stdout(std::process::Stdio::piped());
        cargo_install_cmd.stderr(std::process::Stdio::piped());

        run_progress(
            &mut cargo_install_cmd,
            Some(progress_handler),
            RunConfig::default().with_askpass(),
        )?;

        if !tmp_bin_path.exists() {
            let msg = "failed to install (bin directory was not created)".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        // Check that there is at least one binary in the bin directory
        let bin_files = std::fs::read_dir(&tmp_bin_path).map_err(|err| {
            let msg = format!("failed to read bin directory: {}", err);
            progress_handler.error_with_message(msg.clone());
            UpError::Exec(msg)
        })?;
        let found_binary = bin_files
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .any(|entry| {
                entry
                    .metadata()
                    .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            });
        if !found_binary {
            let msg = "failed to install (no binary found in bin directory)".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        // Move the installed version to the correct crate_name
        std::fs::create_dir_all(&install_path).map_err(|err| {
            progress_handler.error_with_message(format!("failed to create dir: {}", err));
            UpError::Exec(format!("failed to create dir: {}", err))
        })?;

        // Move the tmp_bin_crate_name to the install_crate_name/<bin> directory
        std::fs::rename(&tmp_bin_path, install_path.join("bin")).map_err(|err| {
            progress_handler.error_with_message(format!("failed to move bin: {}", err));
            UpError::Exec(format!("failed to move bin: {}", err))
        })?;

        Ok(true)
    }

    fn version_crate_name(&self, version: &str) -> PathBuf {
        cargo_install_bin_path()
            .join(&self.crate_name)
            .join(version)
    }
}

/// Main function that parses and validates a complete go install string
fn parse_cargo_crate_name<T>(input: T) -> Result<(String, Option<String>), CargoInstallError>
where
    T: AsRef<str>,
{
    let input = input.as_ref();
    let parts: Vec<&str> = input.split('@').collect();
    if parts.len() > 2 {
        return Err(CargoInstallError::InvalidCrateName(
            "multiple @ symbols found".to_string(),
        ));
    }

    let crate_name = parts[0].to_string();

    let version = if parts.len() == 2 {
        Some(parts[1].to_string())
    } else {
        None
    };

    Ok((crate_name, version))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CratesApiError {
    #[serde(default)]
    errors: Vec<CratesApiErrorItem>,
}

impl CratesApiError {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn detail(&self) -> String {
        self.errors
            .iter()
            .map(|error| error.detail.clone())
            .collect::<Vec<String>>()
            .join(", ")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CratesApiErrorItem {
    #[serde(default)]
    detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CratesApiVersions {
    #[serde(default)]
    versions: Vec<CratesApiVersion>,
}

impl CratesApiVersions {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn versions(&self) -> Vec<String> {
        self.versions
            .iter()
            .filter_map(|version| {
                // Skip yanked versions
                if version.yanked {
                    None
                } else {
                    Some(version.num.clone())
                }
            })
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CratesApiVersion {
    #[serde(default)]
    num: String,
    #[serde(default)]
    yanked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::unix::fs::PermissionsExt;

    use crate::internal::testutils::run_with_env;

    mod parse_cargo_crate_name {
        use super::*;

        #[test]
        fn simple_crate() {
            let (name, version) = parse_cargo_crate_name("serde").unwrap();
            assert_eq!(name, "serde");
            assert_eq!(version, None);
        }

        #[test]
        fn crate_with_version() {
            let (name, version) = parse_cargo_crate_name("serde@1.0.0").unwrap();
            assert_eq!(name, "serde");
            assert_eq!(version, Some("1.0.0".to_string()));
        }

        #[test]
        fn invalid_multiple_at() {
            let result = parse_cargo_crate_name("serde@1.0.0@latest");
            assert!(matches!(
                result,
                Err(CargoInstallError::InvalidCrateName(_))
            ));
        }
    }

    mod install {
        use super::*;

        #[test]
        fn latest_version() {
            test_install_crate(
                TestOptions::default().version("1.2.3"),
                UpConfigCargoInstall {
                    crate_name: "test-crate".to_string(),
                    ..UpConfigCargoInstall::default()
                },
            );
        }

        #[test]
        fn specific_version() {
            test_install_crate(
                TestOptions::default().version("1.0.0").no_list(),
                UpConfigCargoInstall {
                    crate_name: "test-crate".to_string(),
                    version: Some("1.0.0".to_string()),
                    exact: true,
                    ..UpConfigCargoInstall::default()
                },
            );
        }

        #[test]
        fn with_prerelease() {
            test_install_crate(
                TestOptions::default().version("2.0.0-alpha"),
                UpConfigCargoInstall {
                    crate_name: "test-crate".to_string(),
                    prerelease: true,
                    ..UpConfigCargoInstall::default()
                },
            );
        }

        #[test]
        fn with_build() {
            test_install_crate(
                TestOptions::default().version("2.0.0+build"),
                UpConfigCargoInstall {
                    crate_name: "test-crate".to_string(),
                    build: true,
                    ..UpConfigCargoInstall::default()
                },
            );
        }

        struct TestOptions {
            expected_version: Option<String>,
            list_versions: bool,
            versions: CratesApiVersions,
        }

        impl Default for TestOptions {
            fn default() -> Self {
                TestOptions {
                    expected_version: None,
                    list_versions: true,
                    versions: CratesApiVersions {
                        versions: vec![
                            CratesApiVersion {
                                num: "1.0.0".to_string(),
                                yanked: false,
                            },
                            CratesApiVersion {
                                num: "1.2.3".to_string(),
                                yanked: false,
                            },
                            CratesApiVersion {
                                num: "2.0.0-alpha".to_string(),
                                yanked: false,
                            },
                            CratesApiVersion {
                                num: "2.0.0+build".to_string(),
                                yanked: false,
                            },
                            CratesApiVersion {
                                num: "3.0.0".to_string(),
                                yanked: true,
                            },
                        ],
                    },
                }
            }
        }

        impl TestOptions {
            fn version(mut self, version: &str) -> Self {
                self.expected_version = Some(version.to_string());
                self
            }

            fn no_list(mut self) -> Self {
                self.list_versions = false;
                self
            }
        }

        fn test_install_crate(test: TestOptions, config: UpConfigCargoInstall) {
            run_with_env(&[], || {
                let mut mock_server = mockito::Server::new();
                let api_url = mock_server.url();

                let config = UpConfigCargoInstall {
                    api_url: Some(api_url.to_string()),
                    ..config
                };

                // Mock the crates.io API response
                let versions_response =
                    serde_json::to_string(&test.versions).expect("failed to serialize versions");

                let mock_versions = mock_server
                    .mock(
                        "GET",
                        format!("/crates/{}/versions", config.crate_name).as_str(),
                    )
                    .with_status(200)
                    .with_body(versions_response)
                    .create();

                // Create a temporary cargo binary for testing
                let temp_dir = tempfile::tempdir().unwrap();
                let cargo_bin = temp_dir.path().join("cargo");
                let script = r#"#!/usr/bin/env bash
                    echo "Running mock cargo with args: $@" >&2
                    if [[ "$1" != "install" ]]; then
                        echo "Nothing to do" >&2
                        exit 0
                    fi
                    next_arg_is_root=false
                    for arg in "$@"; do
                        echo "Processing arg: $arg" >&2
                        if [[ "$next_arg_is_root" == "true" ]]; then
                            root_dir="$arg"
                            next_arg_is_root=false
                            break
                        fi
                        case "$arg" in
                            --root=*)
                                root_dir="${arg#--root=}"
                                break
                                ;;
                            --root)
                                next_arg_is_root=true
                                ;;
                        esac
                    done
                    if [[ -z "$root_dir" ]]; then
                        echo "No root directory provided" >&2
                        exit 1
                    fi
                    echo "Creating bin directory in $root_dir" >&2
                    mkdir -p "$root_dir/bin"
                    new_bin="$root_dir/bin/fakecrate"
                    touch "$new_bin"
                    chmod +x "$new_bin"
                    exit 0
                "#;

                std::fs::write(&cargo_bin, script).expect("failed to write cargo script");
                std::fs::set_permissions(&cargo_bin, std::fs::Permissions::from_mode(0o755))
                    .expect("failed to set permissions");

                let options = UpOptions::default().cache_disabled();
                let mut environment = UpEnvironment::new();
                let progress_handler = UpProgressHandler::new_void();

                let result = config.up(&options, &mut environment, &progress_handler, &cargo_bin);

                assert!(result.is_ok(), "result should be ok, got {:?}", result);
                if test.list_versions {
                    mock_versions.assert();
                } else {
                    assert!(
                        !mock_versions.matched(),
                        "should not have called the API to list versions"
                    );
                }

                // Verify the installed version
                if let Some(expected_version) = test.expected_version {
                    assert_eq!(
                        config.actual_version.get(),
                        Some(&expected_version),
                        "Wrong version installed"
                    );
                }
            });
        }
    }

    mod cleanup {
        use super::*;

        #[test]
        fn cleanup_removes_unused() {
            run_with_env(&[], || {
                let progress_handler = UpProgressHandler::new_void();

                // Create some fake installed crates
                let base_path = cargo_install_bin_path();
                std::fs::create_dir_all(&base_path).unwrap();

                // Create test structure
                std::fs::create_dir_all(base_path.join("serde/1.0.0/bin")).unwrap();
                std::fs::create_dir_all(base_path.join("tokio/1.0.0/bin")).unwrap();
                std::fs::create_dir_all(base_path.join("old-crate/0.1.0/bin")).unwrap();

                // Add some crates to cache as "in use"
                let cache = CargoInstallOperationCache::get();
                cache.add_installed("serde", "1.0.0").unwrap();
                cache.add_installed("tokio", "1.0.0").unwrap();

                let result = UpConfigCargoInstalls::cleanup(&progress_handler).unwrap();

                // Verify cleanup message
                assert!(result.is_some());
                assert!(result.unwrap().contains("old-crate"));

                // Verify directory structure
                assert!(base_path.join("serde/1.0.0").exists());
                assert!(base_path.join("tokio/1.0.0").exists());
                assert!(!base_path.join("old-crate").exists());
            });
        }
    }
}
