use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;

use duct::cmd;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::CacheObject;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::parser::EnvOperationEnum;
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpConfigNix {
    /// List of nix packages to install.
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,

    /// Path to a nix file to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nixfile: Option<String>,

    /// Path to the nix profile that was configured.
    #[serde(skip)]
    pub profile_path: OnceCell<PathBuf>,
}

impl UpConfigNix {
    pub fn new_from_packages(packages: Vec<String>) -> Self {
        UpConfigNix {
            packages,
            nixfile: None,
            profile_path: OnceCell::new(),
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
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        if let Some(config_value) = config_value {
            if let Some(table) = config_value.as_table() {
                if let Some(nixfile) = table.get("file") {
                    if let Some(nixfile) = nixfile.as_str_forced() {
                        return UpConfigNix {
                            packages: Vec::new(),
                            nixfile: Some(nixfile.to_string()),
                            profile_path: OnceCell::new(),
                        };
                    }
                }

                if let Some(packages) = table.get("packages") {
                    if let Some(pkg_array) = packages.as_array() {
                        return UpConfigNix {
                            packages: pkg_array
                                .iter()
                                .filter_map(|v| v.as_str_forced())
                                .collect::<Vec<_>>(),
                            nixfile: None,
                            profile_path: OnceCell::new(),
                        };
                    }
                }
            } else if let Some(pkg_array) = config_value.as_array() {
                return UpConfigNix {
                    packages: pkg_array
                        .iter()
                        .filter_map(|v| v.as_str_forced())
                        .collect::<Vec<_>>(),
                    nixfile: None,
                    profile_path: OnceCell::new(),
                };
            } else if let Some(nixfile) = config_value.as_str_forced() {
                return UpConfigNix {
                    packages: Vec::new(),
                    nixfile: Some(nixfile.to_string()),
                    profile_path: OnceCell::new(),
                };
            }
        }

        UpConfigNix::default()
    }

    pub fn up(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
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

        let nixfile = if let Some(nixfile) = &self.nixfile {
            Some(abs_path(nixfile))
        } else if !self.packages.is_empty() {
            None
        } else {
            let mut nixfile = None;

            for file in &["shell.nix", "default.nix"] {
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

        if let Some(nixfile) = &nixfile {
            // Check if path is in current dir
            if !nixfile.starts_with(&wd_root) {
                return Err(UpError::Exec(format!(
                    "file {} is not in work directory",
                    nixfile.display()
                )));
            }
        }

        // Prepare the progress handler
        progress_handler.init(
            format!(
                "nix ({}):",
                nixfile.as_ref().map_or("packages".to_string(), |nixfile| {
                    match nixfile.file_name() {
                        Some(file_name) => file_name.to_string_lossy().to_string(),
                        None => "nixfile".to_string(),
                    }
                })
            )
            .light_blue(),
        );

        // Generate a profile id either by hashing the list of packages
        // or the contents of the nix file.
        let mut config_hasher = blake3::Hasher::new();
        let profile_suffix = if let Some(nixfile) = &nixfile {
            // Load the file contents into the hasher
            match std::fs::read(nixfile) {
                Ok(contents) => config_hasher.update(&contents),
                Err(e) => {
                    let msg = format!("failed to read nix file: {}", e);
                    return Err(UpError::Exec(msg));
                }
            };

            match nixfile.file_name() {
                Some(file_name) => file_name.to_string_lossy().to_string(),
                None => "nixfile".to_string(),
            }
        } else {
            // Get a hash of the sorted, unique list of packages
            let mut packages = BTreeSet::new();
            packages.extend(self.packages.iter());
            let packages = packages.iter().join("\n");
            config_hasher.update(packages.as_bytes());

            "pkgs".to_string()
        };
        let profile_hash = config_hasher.finalize().to_hex()[..16].to_string();
        let profile_id = format!("profile-{}-{}", profile_suffix, profile_hash);

        // Generate the full path to the permanent nix profile
        let data_path = match wd.data_path() {
            Some(data_path) => data_path,
            None => {
                let msg = format!("failed to get data path for {}", current_dir().display());
                progress_handler.error_with_message(msg.clone());
                return Err(UpError::Exec(msg));
            }
        };
        let nix_path = data_path.join("nix");
        let profile_path = nix_path.join(profile_id);

        // If the profile already exists, we don't need to do anything
        if profile_path.exists() && options.read_cache {
            if let Err(e) = self.profile_path.set(profile_path) {
                omni_warning!(format!("failed to save nix profile path: {:?}", e));
            }

            self.update_cache(progress_handler)?;

            progress_handler.success_with_message("already configured".light_black());
            return Ok(());
        }

        // We want to generate a nix profile from either the package or the nix file;
        // we do that first in a temporary directory so that if any issue happens, we
        // don't overwrite the permanent file for now, protecting against an unexpected
        // garbage collection.
        //
        // We want to call:
        //    nix --extra-experimental-features "nix-command flakes" \
        //          print-dev-env --profile "$tmp_profile" --impure $expression
        //
        // Where $expression is either:
        //  - For a nixfile:
        //      --file "$nixfile"
        //  - For a nixfile with attribute:
        //      --expr "(import ${nixfile} {}).${attribute}"
        //      Where attribute is a string that can be either: "devShell", "shell", "buildInputs", etc.
        //  - For a list of packages:
        //      --expr "with import <nixpkgs> {}; mkShell { buildInputs = [ $packages ]; }"
        //  - For multiple files: (to verify)
        //      --expr "with import <nixpkgs> {}; mkShell { buildInputs = [ (import ./file1.nix {}) (import ./file2.nix {}) ]; }"

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

        progress_handler.progress("preparing nix environment".to_string());

        let mut nix_print_dev_env = TokioCommand::new("nix");
        nix_print_dev_env.stdout(std::process::Stdio::piped());
        nix_print_dev_env.stderr(std::process::Stdio::piped());
        nix_print_dev_env.arg("--extra-experimental-features");
        nix_print_dev_env.arg("nix-command flakes");
        nix_print_dev_env.arg("print-dev-env");
        nix_print_dev_env.arg("--verbose");
        nix_print_dev_env.arg("--print-build-logs");
        nix_print_dev_env.arg("--profile");
        nix_print_dev_env.arg(&tmp_profile);
        nix_print_dev_env.arg("--impure");

        let packages_message;
        if let Some(nixfile) = &nixfile {
            nix_print_dev_env.arg("--file");
            nix_print_dev_env.arg(nixfile);
            packages_message = format!(
                "packages from {}",
                nixfile
                    .strip_prefix(&wd_root)
                    .unwrap_or(nixfile)
                    .display()
                    .light_yellow()
            );
        } else if !self.packages.is_empty() {
            let mut context = tera::Context::new();
            context.insert("packages", &self.packages.join(" "));
            let tmpl = r#"with import <nixpkgs> {}; mkShell { buildInputs = [ {{ packages }} ]; }"#;
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
            packages_message = format!(
                "{} package{}",
                self.packages.len().to_string().light_yellow(),
                if self.packages.len() > 1 { "s" } else { "" },
            );
        } else {
            unreachable!();
        }
        progress_handler.progress(format!("installing {}", packages_message));

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

        // Now we want to build the nix profile and add it to the gcroots
        // so that the packages don't get garbage collected.
        //
        // We want to call:
        //      nix --extra-experimental-features "nix-command flakes" \
        //          build --out-link "$perm_profile" "$tmp_profile"
        //
        // And we will put the permanent profile in the data directory
        // of the work directory, so that its life is tied to the work
        // directory cache.

        progress_handler.progress("protecting dependencies with gcroots".to_string());

        let mut nix_build = TokioCommand::new("nix");
        nix_build.arg("--extra-experimental-features");
        nix_build.arg("nix-command flakes");
        nix_build.arg("build");
        nix_build.arg("--print-out-paths");
        nix_build.arg("--out-link");
        nix_build.arg(&profile_path);
        nix_build.arg(&tmp_profile);
        nix_build.stdout(std::process::Stdio::piped());
        nix_build.stderr(std::process::Stdio::piped());

        let result = run_progress(&mut nix_build, Some(progress_handler), RunConfig::default());
        if let Err(e) = result {
            let msg = format!("failed to build nix profile: {}", e);
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        if let Err(e) = self.profile_path.set(profile_path) {
            omni_warning!(format!("failed to save nix profile path: {:?}", e));
        }

        self.update_cache(progress_handler)?;

        // We are all done, the temporary directory will be removed automatically,
        // so we don't need to worry about it, hence removing our temporary profile
        // for us.
        progress_handler.success_with_message(format!("installed {}", packages_message));

        Ok(())
    }

    pub fn down(&self, _progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        // At the end of the 'down' operation, the work directory cache will be
        // wiped. Cleaning dependencies for nix just means removing the gcroots
        // file, so we don't need to do anything here since it will be removed
        // with the cache directory.

        Ok(())
    }

    fn update_cache(&self, progress_handler: &dyn ProgressHandler) -> Result<(), UpError> {
        // Get the nix profile path
        let profile_path = match self.profile_path.get() {
            Some(profile_path) => profile_path,
            None => {
                return Err(UpError::Exec("nix profile path not set".to_string()));
            }
        };

        // Load it into a nix profile struct
        let profile = match NixProfile::from_file(profile_path) {
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

        let wd = workdir(".");
        let wd_id = match wd.id() {
            Some(wd_id) => wd_id,
            None => {
                return Err(UpError::Exec(format!(
                    "failed to get work directory id for {}",
                    current_dir().display()
                )));
            }
        };

        progress_handler.progress("updating cache".to_string());

        if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| {
            if !paths.is_empty() {
                up_env.add_paths(&wd_id, profile.get_paths());
            }

            if let Some(cflags) = cflags {
                up_env.add_env_var_operation(&wd_id, "CFLAGS", &cflags, EnvOperationEnum::Suffix);
                up_env.add_env_var_operation(&wd_id, "CPPFLAGS", &cflags, EnvOperationEnum::Suffix);
            }

            if let Some(ldflags) = ldflags {
                up_env.add_env_var_operation(&wd_id, "LDFLAGS", &ldflags, EnvOperationEnum::Suffix);
            }

            for pkg_config_path in pkg_config_paths.iter().rev() {
                up_env.add_env_var_operation(
                    &wd_id,
                    "PKG_CONFIG_PATH",
                    pkg_config_path,
                    EnvOperationEnum::Prepend,
                );
            }

            true
        }) {
            progress_handler.progress(format!("failed to update cache: {}", err));
        }

        progress_handler.progress("cache updated".to_string());

        Ok(())
    }

    pub fn is_available(&self) -> bool {
        if cmd!("command", "-v", "nix")
            .stdout_null()
            .stderr_null()
            .run()
            .is_ok()
        {
            return true;
        }
        false
    }

    pub fn was_upped(&self) -> bool {
        self.profile_path.get().is_some()
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        match self.profile_path.get() {
            Some(profile_path) => vec![profile_path.clone()],
            None => vec![],
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
        #[allow(dead_code)]
        r#type: String,
        value: String,
    },
    Array {
        #[allow(dead_code)]
        r#type: String,
        value: Vec<String>,
    },
    Unknown {
        #[allow(dead_code)]
        r#type: String,
    },
}
