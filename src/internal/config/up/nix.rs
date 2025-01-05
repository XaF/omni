use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::global_config;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::config::up::utils::get_command_output;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::env::current_dir;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_warning;

fn nix_command<T: AsRef<str>>(name: T) -> TokioCommand {
    let mut command = TokioCommand::new("nix");
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    command.arg("--extra-experimental-features");
    command.arg("nix-command flakes");
    command.arg(name.as_ref());
    command
}

fn nix_gcroot_command<T: AsRef<Path>>(tmp_profile: T, perm_profile: T) -> TokioCommand {
    let mut command = nix_command("build");
    command.arg("--print-out-paths");
    command.arg("--out-link");
    command.arg(perm_profile.as_ref());
    command.arg(tmp_profile.as_ref());
    command
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpConfigNix {
    /// List of nix packages to install.
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,

    /// Path to a nix file to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nixfile: Option<String>,

    /// Path to the nix profile(s) stored in the data path
    #[serde(skip)]
    pub data_paths: OnceCell<Vec<PathBuf>>,
}

impl UpConfigNix {
    pub fn new_from_packages(packages: Vec<String>) -> Self {
        UpConfigNix {
            packages,
            nixfile: None,
            data_paths: OnceCell::new(),
        }
    }

    /// Parse the configuration value into a `UpConfigNix` struct.
    ///
    /// The following are all valid ways to specify nix dependencies:
    /// ```yaml
    /// # Installing nix packages
    /// up:
    /// - nix:
    ///   - gcc
    ///   - gnused
    ///   - ...
    ///
    /// # Also valid, using the 'packages' key
    /// up:
    /// - nix:
    ///    packages:
    ///    - gcc
    ///    - gnused
    ///    - ...
    ///
    /// # Or specifying a nix file
    /// up:
    /// - nix: "shell.nix"
    ///
    /// # Also valid, using the file key; note that the 'packages' key
    /// # will be ignored if a nix file is specified
    /// up:
    /// - nix:
    ///     file: "shell.nix"
    ///
    /// # Finally, using the default configuration, which will look for
    /// # a `shell.nix` or `default.nix` file in the current directory.
    /// # Note that if no nix file is found, the operation will fail.
    /// up:
    /// - nix
    /// ```
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_key: &str,
        on_error: &mut impl FnMut(ConfigErrorKind),
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => {
                on_error(ConfigErrorKind::MissingKey {
                    key: format!("{}.packages", error_key),
                });
                return Self::default();
            }
        };

        if let Some(table) = config_value.as_table() {
            if let Some(nixfile) = table.get("file") {
                if let Some(nixfile) = nixfile.as_str_forced() {
                    return Self {
                        nixfile: Some(nixfile.to_string()),
                        ..Self::default()
                    };
                } else {
                    on_error(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.file", error_key),
                        actual: nixfile.as_serde_yaml(),
                        expected: "string".to_string(),
                    });
                }
            }

            if let Some(packages) = table.get("packages") {
                if let Some(pkg_array) = packages.as_array() {
                    return Self {
                        packages: pkg_array
                            .iter()
                            .filter_map(|v| v.as_str_forced())
                            .collect::<Vec<_>>(),
                        ..Self::default()
                    };
                } else {
                    on_error(ConfigErrorKind::InvalidValueType {
                        key: format!("{}.packages", error_key),
                        actual: packages.as_serde_yaml(),
                        expected: "array".to_string(),
                    });
                }
            }

            on_error(ConfigErrorKind::MissingKey {
                key: format!("{}.packages", error_key),
            });
            Self::default()
        } else if let Some(pkg_array) = config_value.as_array() {
            Self {
                packages: pkg_array
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, value)| match value.as_str_forced() {
                        Some(pkg) => Some(pkg.to_string()),
                        None => {
                            on_error(ConfigErrorKind::InvalidValueType {
                                key: format!("{}[{}]", error_key, idx),
                                actual: value.as_serde_yaml(),
                                expected: "string".to_string(),
                            });
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
                ..Self::default()
            }
        } else if let Some(nixfile) = config_value.as_str_forced() {
            Self {
                nixfile: Some(nixfile.to_string()),
                ..Self::default()
            }
        } else {
            on_error(ConfigErrorKind::InvalidValueType {
                key: error_key.to_string(),
                actual: config_value.as_serde_yaml(),
                expected: "string, array or table".to_string(),
            });

            Self::default()
        }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        let nix_handler = NixHandler::new(&self.nixfile, &self.packages)?;

        // Prepare the progress handler
        progress_handler.init(format!("nix ({}):", nix_handler.nix_source.name()).light_blue());

        if !global_config()
            .up_command
            .operations
            .is_operation_allowed("nix")
        {
            let errmsg = "nix operation is not allowed".to_string();
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
        }

        // If the profile already exists, we don't need to do anything
        if options.read_cache && nix_handler.exists()? {
            let paths = nix_handler.paths(progress_handler)?;
            if self.data_paths.set(paths).is_err() {
                omni_warning!("failed to save nix profile path".to_string());
            }

            self.update_cache(environment, progress_handler)?;

            progress_handler.success_with_message("already configured".light_black());
            return Ok(());
        }

        let paths = nix_handler.build(progress_handler)?;
        if self.data_paths.set(paths).is_err() {
            omni_warning!("failed to save nix profile path".to_string());
        }

        self.update_cache(environment, progress_handler)?;

        // We are all done, the temporary directory will be removed automatically,
        // so we don't need to worry about it, hence removing our temporary profile
        // for us.
        progress_handler
            .success_with_message(format!("installed {}", nix_handler.nix_source.desc()));

        Ok(())
    }

    pub fn down(&self, _progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        // At the end of the 'down' operation, the work directory cache will be
        // wiped. Cleaning dependencies for nix just means removing the gcroots
        // file, so we don't need to do anything here since it will be removed
        // with the cache directory.

        Ok(())
    }

    fn update_cache(
        &self,
        environment: &mut UpEnvironment,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<(), UpError> {
        // Get the nix profile path
        let profile_path = match self.data_paths.get() {
            Some(data_paths) => data_paths[0].clone(),
            None => {
                return Err(UpError::Exec("nix profile path not set".to_string()));
            }
        };

        // Load it into a nix profile struct
        let profile = match NixProfile::from_file(&profile_path) {
            Ok(profile) => profile,
            Err(e) => {
                return Err(UpError::Exec(format!("failed to load nix profile: {}", e)));
            }
        };

        let paths = profile.get_paths();
        let cflags = profile.get_cflags();
        let ldflags = profile.get_ldflags();
        let pkg_config_paths = profile.get_pkg_config_paths();

        if paths.is_empty() && cflags.is_none() && ldflags.is_none() && pkg_config_paths.is_empty()
        {
            return Ok(());
        }

        progress_handler.progress("updating cache".to_string());

        if !paths.is_empty() {
            environment.add_paths(profile.get_paths());
        }

        if let Some(cflags) = cflags {
            environment.add_env_var_operation("CFLAGS", &cflags, EnvOperationEnum::Suffix);
            environment.add_env_var_operation("CPPFLAGS", &cflags, EnvOperationEnum::Suffix);
        }

        if let Some(ldflags) = ldflags {
            environment.add_env_var_operation("LDFLAGS", &ldflags, EnvOperationEnum::Suffix);
        }

        for pkg_config_path in pkg_config_paths.iter().rev() {
            environment.add_env_var_operation(
                "PKG_CONFIG_PATH",
                pkg_config_path,
                EnvOperationEnum::Prepend,
            );
        }

        progress_handler.progress("cache updated".to_string());

        Ok(())
    }

    pub fn is_available(&self) -> bool {
        which::which("nix").is_ok()
    }

    pub fn was_upped(&self) -> bool {
        self.data_paths.get().is_some()
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        match self.data_paths.get() {
            Some(data_paths) => data_paths.clone(),
            None => vec![],
        }
    }
}

#[derive(Debug, Default)]
struct NixHandler {
    nix_source: NixSource,
    nix_data_path: OnceCell<PathBuf>,
}

impl NixHandler {
    fn new(nixfile: &Option<String>, packages: &[String]) -> Result<Self, UpError> {
        let nix_source = NixSource::new(nixfile, packages)?;

        Ok(Self {
            nix_source,
            ..Self::default()
        })
    }

    fn nix_data_path(&self) -> Result<PathBuf, UpError> {
        let nix_data_path = match self.nix_data_path.get() {
            Some(nix_data_path) => nix_data_path.clone(),
            None => {
                let wd = workdir(".");
                let data_path = match wd.data_path() {
                    Some(data_path) => data_path,
                    None => {
                        let msg =
                            format!("failed to get data path for {}", current_dir().display());
                        return Err(UpError::Exec(msg));
                    }
                };
                let nix_data_path = data_path.join("nix");
                if self.nix_data_path.set(nix_data_path.clone()).is_err() {
                    return Err(UpError::Exec("failed to save nix data path".to_string()));
                }
                nix_data_path
            }
        };

        Ok(nix_data_path)
    }

    fn exists(&self) -> Result<bool, UpError> {
        Ok(self
            .nix_data_path()?
            .join(self.nix_source.profile_id()?)
            .exists())
    }

    fn build(&self, progress_handler: &dyn ProgressHandler) -> Result<Vec<PathBuf>, UpError> {
        // Generate a temporary file to store the nix profile, we want a directory as
        // a second file will be generated in the same directory as our temp profile,
        // and we want to remove both of them once we're done. Using a temporary
        // directory will make sure that the directory is removed automatically
        // with all its content.
        let tmp_dir = match tempfile::Builder::new().prefix("omni_up_nix.").tempdir() {
            Ok(tmp_dir) => tmp_dir,
            Err(e) => {
                let msg = format!("failed to create temporary directory: {}", e);
                progress_handler.error_with_message(msg.clone());
                return Err(UpError::Exec(msg));
            }
        };
        let tmp_profile = tmp_dir.path().join("profile");

        // Prepare the nix profile
        self.nix_source
            .print_dev_env(&tmp_profile, progress_handler)?;

        // Build and return the built paths
        self.nix_source
            .build(&tmp_profile, &self.nix_data_path()?, progress_handler)
    }

    fn paths(&self, progress_handler: &dyn ProgressHandler) -> Result<Vec<PathBuf>, UpError> {
        self.nix_source
            .paths(&self.nix_data_path()?, progress_handler)
    }
}

/// NixSource is an enum that represents the different ways to specify
/// nix dependencies: either by listing packages, by specifying a nix
/// file, or by specifying a flake.
#[derive(Debug)]
enum NixSource {
    Packages(BTreeSet<String>),
    NixFile(PathBuf),
    Flake(PathBuf),
}

impl Default for NixSource {
    fn default() -> Self {
        Self::Packages(BTreeSet::new())
    }
}

impl NixSource {
    fn new(nixfile: &Option<String>, packages: &[String]) -> Result<Self, UpError> {
        let wd = workdir(".");
        let wd_root = match wd.root() {
            Some(wd_root) => PathBuf::from(wd_root),
            None => {
                let msg = format!(
                    "failed to get work directory root for {}",
                    current_dir().display()
                );
                return Err(UpError::Exec(msg));
            }
        };

        let nixfile = if let Some(nixfile) = &nixfile {
            Some(abs_path(nixfile))
        } else if !packages.is_empty() {
            None
        } else {
            let mut nixfile = None;

            for file in &["shell.nix", "default.nix", "flake.nix"] {
                let absfile = wd_root.join(file);
                if absfile.exists() && absfile.is_file() {
                    nixfile = Some(absfile);
                    break;
                }
            }

            // If no nix file was found, we fail, since if there is a `nix` step
            // we expect to have to install some dependencies.
            match nixfile {
                Some(nixfile) => Some(abs_path(nixfile)),
                None => {
                    return Err(UpError::Exec(format!(
                        "no nix file found in {}",
                        wd_root.display()
                    )))
                }
            }
        };

        match &nixfile {
            Some(nixfile) => {
                if !nixfile.starts_with(&wd_root) {
                    return Err(UpError::Exec(format!(
                        "file {} is not in work directory",
                        nixfile.display()
                    )));
                }

                let is_flake = nixfile
                    .file_name()
                    .map_or(false, |file_name| file_name == "flake.nix");

                if is_flake {
                    Ok(Self::Flake(nixfile.clone()))
                } else {
                    Ok(Self::NixFile(nixfile.clone()))
                }
            }
            None => Ok(Self::Packages(packages.iter().cloned().collect())),
        }
    }

    fn name(&self) -> String {
        match *self {
            Self::Packages(_) => "packages".to_string(),
            Self::NixFile(ref nixfile) => {
                nixfile.file_name().map_or("nixfile".into(), |file_name| {
                    file_name.to_string_lossy().to_string()
                })
            }
            Self::Flake(ref nixfile) => nixfile.file_name().map_or("flake".into(), |file_name| {
                file_name.to_string_lossy().to_string()
            }),
        }
    }

    fn hash_file(hasher: &mut blake3::Hasher, file: &PathBuf) -> Result<(), UpError> {
        match std::fs::read(file) {
            Ok(contents) => hasher.update(&contents),
            Err(e) => {
                let msg = format!("failed to read {}: {}", file.display(), e);
                return Err(UpError::Exec(msg));
            }
        };

        Ok(())
    }

    /// Generate a profile id either by hashing the list of packages
    /// or the contents of the nix file.
    fn profile_id(&self) -> Result<String, UpError> {
        let mut config_hasher = blake3::Hasher::new();

        let profile_suffix = match *self {
            Self::Packages(ref packages) => {
                let packages = packages.iter().join("\n");
                config_hasher.update(packages.as_bytes());

                "pkgs".to_string()
            }
            Self::NixFile(ref nixfile) => {
                Self::hash_file(&mut config_hasher, nixfile)?;

                match nixfile.file_name() {
                    Some(file_name) => file_name.to_string_lossy().to_string(),
                    None => "nixfile".to_string(),
                }
            }
            Self::Flake(ref nixfile) => {
                Self::hash_file(&mut config_hasher, nixfile)?;

                let dirname = nixfile.parent().ok_or_else(|| {
                    UpError::Exec(format!(
                        "failed to get parent directory of {}",
                        nixfile.display()
                    ))
                })?;
                for extra_file in ["flake.lock", "devshell.toml"] {
                    let extra_file = dirname.join(extra_file);
                    if extra_file.exists() {
                        Self::hash_file(&mut config_hasher, &extra_file)?;
                    }
                }

                "flake".to_string()
            }
        };
        let profile_hash = config_hasher.finalize().to_hex()[..16].to_string();
        let profile_id = format!("profile-{}-{}", profile_suffix, profile_hash);

        Ok(profile_id)
    }

    fn desc(&self) -> String {
        match *self {
            Self::Packages(ref packages) => format!(
                "{} package{}",
                packages.len().to_string().light_yellow(),
                if packages.len() > 1 { "s" } else { "" },
            ),
            Self::NixFile(ref nixfile) => {
                let wd = workdir(".");
                let wd_root = match wd.root() {
                    Some(wd_root) => PathBuf::from(wd_root),
                    None => unreachable!("should be in a work directory"),
                };

                format!(
                    "packages from {}",
                    nixfile
                        .strip_prefix(&wd_root)
                        .unwrap_or(nixfile)
                        .display()
                        .light_yellow()
                )
            }
            Self::Flake(ref _nixfile) => "packages from flake".to_string(),
        }
    }

    /// We want to generate a nix profile from either the package or the nix file;
    /// we do that first in a temporary directory so that if any issue happens, we
    /// don't overwrite the permanent file for now, protecting against an unexpected
    /// garbage collection.
    ///
    /// We want to call:
    ///    nix --extra-experimental-features "nix-command flakes" \
    ///          print-dev-env --profile "$tmp_profile" --impure $expression
    ///
    /// Where $expression is either:
    ///  - For a nixfile:
    ///      --file "$nixfile"
    ///  - For a nixfile with attribute:
    ///      --expr "(import ${nixfile} {}).${attribute}"
    ///      Where attribute is a string that can be either: "devShell", "shell", "buildInputs", etc.
    ///  - For a list of packages:
    ///      --expr "with import <nixpkgs> {}; mkShell { buildInputs = [ $packages ]; }"
    ///  - For multiple files: (to verify)
    ///      --expr "with import <nixpkgs> {}; mkShell { buildInputs = [ (import ./file1.nix {}) (import ./file2.nix {}) ]; }"
    fn print_dev_env(
        &self,
        tmp_profile: &PathBuf,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.progress("preparing nix environment".to_string());

        let mut nix_print_dev_env = nix_command("print-dev-env");
        nix_print_dev_env.arg("--verbose");
        nix_print_dev_env.arg("--print-build-logs");
        nix_print_dev_env.arg("--profile");
        nix_print_dev_env.arg(tmp_profile);

        match *self {
            Self::Packages(ref packages) => {
                nix_print_dev_env.arg("--impure");

                let mut context = tera::Context::new();
                context.insert("packages", &packages.iter().join(" "));
                let tmpl =
                    r#"with import <nixpkgs> {}; mkShell { buildInputs = [ {{ packages }} ]; }"#;
                let expr = match tera::Tera::one_off(tmpl, &context, false) {
                    Ok(expr) => expr,
                    Err(e) => {
                        let msg = format!("failed to render nix expression: {}", e);
                        progress_handler.error_with_message(msg.clone());
                        return Err(UpError::Exec(msg));
                    }
                };

                nix_print_dev_env.arg("--expr");
                nix_print_dev_env.arg(&expr);
            }
            Self::NixFile(ref nixfile) => {
                nix_print_dev_env.arg("--impure");

                nix_print_dev_env.arg("--file");
                nix_print_dev_env.arg(nixfile);
            }
            Self::Flake(ref _nixfile) => {}
        };
        progress_handler.progress(format!("installing {}", self.desc()));

        let result = run_progress(
            &mut nix_print_dev_env,
            Some(progress_handler),
            RunConfig::default(),
        );

        if let Err(e) = result {
            let msg = format!("failed to install nix packages: {}", e);
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        Ok(())
    }

    /// Now we want to build the nix profile and add it to the gcroots
    /// so that the packages don't get garbage collected.
    ///
    /// We want to call:
    ///      nix --extra-experimental-features "nix-command flakes" \
    ///          build --out-link "$perm_profile" "$tmp_profile"
    ///
    /// And we will put the permanent profile in the data directory
    /// of the work directory, so that its life is tied to the work
    /// directory data.
    fn build(
        &self,
        tmp_profile: &Path,
        nix_data_path: &Path,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<Vec<PathBuf>, UpError> {
        progress_handler.progress("protecting dependencies with gcroots".to_string());

        let profile_id = self.profile_id()?;
        let profile_path = nix_data_path.join(&profile_id);

        let mut built_paths = Vec::new();
        built_paths.push(profile_path.clone());

        let mut nix_build = nix_gcroot_command(tmp_profile, &profile_path);

        let result = run_progress(&mut nix_build, Some(progress_handler), RunConfig::default());
        if let Err(e) = result {
            let msg = format!("failed to build nix profile: {}", e);
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        // For flakes, we can also add the sources to the garbage collection root
        let paths = self.flake_archive_paths(progress_handler)?;
        for path in paths {
            let perm_path = nix_data_path.join(format!(
                "{}.{}",
                profile_id,
                path.file_name()
                    .expect("path has no file name")
                    .to_string_lossy()
            ));
            let mut nix_build = nix_gcroot_command(&path, &perm_path);

            let result = run_progress(&mut nix_build, Some(progress_handler), RunConfig::default());
            if let Err(e) = result {
                let msg = format!("failed to build flake archive: {}", e);
                progress_handler.error_with_message(msg.clone());
                return Err(UpError::Exec(msg));
            }

            built_paths.push(perm_path);
        }

        Ok(built_paths)
    }

    fn paths(
        &self,
        nix_data_path: &Path,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<Vec<PathBuf>, UpError> {
        let profile_id = self.profile_id()?;
        let profile_path = nix_data_path.join(&profile_id);

        let mut built_paths = Vec::new();
        built_paths.push(profile_path.clone());

        let paths = self.flake_archive_paths(progress_handler)?;
        for path in paths {
            let perm_path = nix_data_path.join(format!(
                "{}.{}",
                profile_id,
                path.file_name()
                    .expect("path has no file name")
                    .to_string_lossy()
            ));

            built_paths.push(perm_path);
        }

        Ok(built_paths)
    }

    fn flake_archive_paths(
        &self,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<Vec<PathBuf>, UpError> {
        match *self {
            Self::Flake(ref nixfile) => {
                let nixfile_parent_path = nixfile
                    .parent()
                    .expect("nixfile has no parent")
                    .to_path_buf();

                let mut nix_flake = nix_command("flake");
                nix_flake.arg("archive");
                nix_flake.arg("--json");
                nix_flake.arg("--no-write-lock-file");
                nix_flake.arg(nixfile_parent_path);

                let output = match get_command_output(&mut nix_flake, RunConfig::default()) {
                    Ok(output) => output,
                    Err(e) => {
                        let msg = format!("failed to archive nix flake: {}", e);
                        progress_handler.error_with_message(msg.clone());
                        return Err(UpError::Exec(msg));
                    }
                };

                if !output.status.success() {
                    let msg = format!(
                        "failed to archive nix flake: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    progress_handler.error_with_message(msg.clone());
                    return Err(UpError::Exec(msg));
                }

                let archive = String::from_utf8_lossy(&output.stdout);

                // Get all the /nix/store/... paths found in the archive
                let re = regex::Regex::new(r#""/nix/store/[^"]+""#).expect("invalid regex");
                let paths = re
                    .find_iter(&archive)
                    .map(|m| m.as_str().trim_matches('"').to_string())
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();

                Ok(paths)
            }
            _ => Ok(Vec::new()),
        }
    }
}

/// NixProfile is a structure that allows to deserialize a nix profile
/// so that we can extract the environment variables and paths from it.
#[derive(Debug, Deserialize)]
struct NixProfile {
    pub variables: HashMap<String, NixProfileVariable>,
}

impl NixProfile {
    fn from_file(profile_path: &PathBuf) -> Result<Self, UpError> {
        let file = match std::fs::File::open(profile_path) {
            Ok(file) => file,
            Err(e) => {
                return Err(UpError::Exec(format!("failed to open nix profile: {}", e)));
            }
        };

        let profile: NixProfile = match serde_json::from_reader(file) {
            Ok(profile) => profile,
            Err(e) => {
                return Err(UpError::Exec(format!("failed to parse nix profile: {}", e)));
            }
        };

        Ok(profile)
    }

    fn get_paths(&self) -> Vec<PathBuf> {
        ["pkgsHostTarget", "pkgsHostHost"]
            .iter()
            .filter_map(|key| self.variables.get(key.to_string().as_str()))
            .filter_map(|pkg| match pkg {
                NixProfileVariable::Array { value, .. } => Some(value),
                _ => None,
            })
            .flatten()
            .dedup()
            .map(|path| PathBuf::from(path).join("bin"))
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>()
    }

    fn get_cflags(&self) -> Option<String> {
        self.get_first_var(&["NIX_CFLAGS_COMPILE_FOR_TARGET", "NIX_CFLAGS_COMPILE"])
    }

    fn get_ldflags(&self) -> Option<String> {
        self.get_first_var(&["NIX_LDFLAGS_FOR_TARGET", "NIX_LDFLAGS"])
    }

    fn get_pkg_config_paths(&self) -> Vec<String> {
        match self.get_first_var(&["PKG_CONFIG_PATH_FOR_TARGET", "PKG_CONFIG_PATH"]) {
            Some(pkg_config_path) => pkg_config_path
                .split(':')
                .map(|path| path.to_string())
                .collect::<Vec<_>>(),
            None => Vec::new(),
        }
    }

    fn get_var(&self, key: &str) -> Option<String> {
        match self.variables.get(key) {
            Some(NixProfileVariable::Var { value, .. }) => Some(value.to_string()),
            Some(_) | None => None,
        }
    }

    fn get_first_var(&self, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(value) = self.get_var(key) {
                return Some(value);
            }
        }
        None
    }
}

/// NixProfileVariable is a structure representing one of the variables
/// in a nix profile, that can either be a string or an array of strings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NixProfileVariable {
    Var {
        // r#type: String,
        value: String,
    },
    Array {
        // r#type: String,
        value: Vec<String>,
    },
    Unknown {
        // r#type: String,
    },
}
