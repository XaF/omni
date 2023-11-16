use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command as ProcessCommand;

use lazy_static::lazy_static;
use semver::{Prerelease, Version};
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;

use tokio::process::Command as TokioCommand;

use crate::internal::config::config;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::ConfigLoader;
use crate::internal::ConfigValue;
use crate::internal::ENV;
use crate::omni_error;

lazy_static! {
    static ref RELEASE_ARCH: String = {
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            _ => std::env::consts::ARCH,
        };
        arch.to_string()
    };

    static ref RELEASE_OS: String = {
        let os = match std::env::consts::OS {
            "macos" => "darwin",
            _ => std::env::consts::OS,
        };
        os.to_string()
    };

    static ref CURRENT_VERSION: Version = {
        let mut version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        if !version.pre.is_empty() {
            // Check if it starts with `rc` or `beta` or `alpha`, in which case
            // we wanna keep them, otherwise we consider we're at the version,
            // as otherwise semver would consider `1.0.0-5-xxxx` < `1.0.0`
            if !(version.pre.starts_with("rc")
                || version.pre.starts_with("beta")
                || version.pre.starts_with("alpha"))
            {
                // Clear prerelease
                version.pre = Prerelease::EMPTY;
            }
        }
        version
    };

    static ref INSTALLED_WITH_BREW: bool = {
        // Get the path of the current binary
        let current_exe = std::env::current_exe().expect("Failed to get current exe path");

        // Get the homebrew prefix, either through the HOMEBREW_PREFIX env var
        // if available, or by calling `brew --prefix`; if both fail, we're not
        // installed with homebrew since... probably no homebrew.
        let homebrew_prefix = std::env::var("HOMEBREW_PREFIX")
            .ok()
            .or_else(|| {
                let output = std::process::Command::new("brew")
                    .arg("--prefix")
                    .output()
                    .ok()?;
                String::from_utf8(output.stdout).ok()
            });

        if let Some(homebrew_prefix) = homebrew_prefix {
            // Check if the current binary is in the homebrew prefix
            current_exe.starts_with(format!("{}/", homebrew_prefix))
        } else {
            false
        }
    };
}

pub fn self_update() {
    // Check if OMNI_SKIP_SELF_UPDATE is set
    if let Some(skip_self_update) = std::env::var_os("OMNI_SKIP_SELF_UPDATE") {
        if !skip_self_update.to_str().unwrap().is_empty() {
            return;
        }
    }

    let config = config(".");
    if config.path_repo_updates.self_update.do_not_check() {
        return;
    }

    if let Some(omni_release) = OmniRelease::latest() {
        omni_release.check_and_update();
    }
}

#[derive(Debug, Deserialize)]
struct OmniRelease {
    version: String,
    binaries: Vec<OmniReleaseBinary>,
}

impl OmniRelease {
    fn latest() -> Option<Self> {
        let json_url =
            "https://raw.githubusercontent.com/XaF/homebrew-omni/main/Formula/resources/omni.json";

        let response = reqwest::blocking::get(json_url);
        if let Err(_err) = response {
            return None;
        }
        let mut response = response.unwrap();

        let mut content = String::new();
        response
            .read_to_string(&mut content)
            .expect("Failed to read response");

        let json: Result<OmniRelease, _> = serde_json::from_str(content.as_str());
        if let Err(err) = json {
            dbg!("Failed to parse latest release: {:?}", err);
            return None;
        }
        let json = json.unwrap();

        Some(json)
    }

    fn is_newer(&self) -> bool {
        match Version::parse(self.version.as_str()) {
            Ok(version) => version > *CURRENT_VERSION,
            Err(_err) => {
                omni_error!(format!("Failed to parse release version: {}", self.version));
                false
            }
        }
    }

    fn compatible_binary(&self) -> Option<&OmniReleaseBinary> {
        self.binaries
            .iter()
            .find(|&binary| binary.os == *RELEASE_OS && binary.arch == *RELEASE_ARCH)
    }

    fn check_and_update(&self) {
        let config = config(".");

        let desc = format!("{} update:", "omni".light_cyan()).light_blue();
        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, None))
        } else {
            Box::new(PrintProgressHandler::new(desc, None))
        };

        progress_handler.progress("Checking for updates".to_string());

        if !self.is_newer() {
            progress_handler.success_with_message("already up-to-date".light_black());
            return;
        }

        if config.path_repo_updates.self_update.is_false() {
            progress_handler.success_with_message(format!(
                "{} version {} is available",
                "omni:".light_cyan(),
                self.version.light_blue(),
            ));
            return;
        }

        if config.path_repo_updates.self_update.is_ask() {
            progress_handler.hide();

            let question = requestty::Question::expand("do_you_want_to_update")
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message(format!(
                    "{} version {} is available; {}",
                    "omni:".light_cyan(),
                    self.version.light_blue(),
                    "do you want to install it?".yellow(),
                ))
                .choices(vec![
                    ('a', "Yes, always (update without asking in the future)"),
                    ('y', "Yes, this time (and ask me everytime)"),
                    ('n', "No"),
                    ('x', "No, never (skip without asking in the future)"),
                ])
                .default('y')
                .build();

            if !match requestty::prompt_one(question) {
                Ok(answer) => match answer {
                    requestty::Answer::ExpandItem(expanditem) => match expanditem.key {
                        'a' => self.edit_config_file_self_update(true),
                        'y' => true,
                        'n' => false,
                        'x' => self.edit_config_file_self_update(false),
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                },
                Err(err) => {
                    println!("{}", format!("[✘] {:?}", err).red());
                    return;
                }
            } {
                return;
            }

            progress_handler.show();
        }

        let updated = if *INSTALLED_WITH_BREW {
            self.brew_upgrade(progress_handler.as_ref())
        } else {
            self.download(progress_handler.as_ref())
        };

        if updated.is_err() {
            progress_handler.error_with_message("Failed to update".to_string());
            return;
        }
        let updated = updated.unwrap();

        if updated {
            progress_handler
                .success_with_message(format!("updated to version {}", self.version).light_green());

            // Replace current process with the new binary
            ProcessCommand::new(std::env::current_exe().unwrap())
                .args(std::env::args().skip(1))
                .env("OMNI_SKIP_SELF_UPDATE", "1")
                .exec();

            panic!("Failed to replace current process with the new binary");
        } else {
            progress_handler.success_with_message("already up-to-date".light_black());
        }
    }

    fn edit_config_file_self_update(&self, self_update: bool) -> bool {
        if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
            let insert_value = if self_update { "true" } else { "false" };

            if let Some(config_path) = config_value.get_as_table_mut("path_repo_updates") {
                config_path.insert(
                    "self_update".to_string(),
                    ConfigValue::from_str(insert_value),
                );
            } else if let Some(config_value_table) = config_value.as_table_mut() {
                config_value_table.insert(
                    "path_repo_updates".to_string(),
                    ConfigValue::from_str(format!("self_update: {}", insert_value).as_str()),
                );
            } else {
                *config_value = ConfigValue::from_str(
                    format!("path_repo_updates:\n  self_update: {}", insert_value).as_str(),
                );
            }

            true
        }) {
            omni_error!(format!("failed to update configuration file: {:?}", err,));
        }

        self_update
    }

    fn brew_upgrade(&self, progress_handler: &dyn ProgressHandler) -> io::Result<bool> {
        progress_handler.progress("updating with homebrew".to_string());

        let mut brew_upgrade = TokioCommand::new("brew");
        brew_upgrade.arg("upgrade");
        brew_upgrade.arg("xaf/omni/omni");
        brew_upgrade.stdout(std::process::Stdio::piped());
        brew_upgrade.stderr(std::process::Stdio::piped());

        let run = run_progress(
            &mut brew_upgrade,
            Some(progress_handler),
            RunConfig::default(),
        );
        if let Err(err) = run {
            return Err(io::Error::new(io::ErrorKind::Other, err.to_string()));
        }

        Ok(true)
    }

    fn download(&self, progress_handler: &dyn ProgressHandler) -> io::Result<bool> {
        let binary = self.compatible_binary();
        if binary.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "no compatible binary found for {} {}",
                    *RELEASE_OS, *RELEASE_ARCH,
                ),
            ));
        }
        let binary = binary.unwrap();

        // Prepare a temporary directory to download the assets
        progress_handler.progress("preparing download".to_string());
        let tmp_dir = tempfile::Builder::new().prefix("omni_update.").tempdir()?;

        // Prepare the path to the tar.gz
        let archive_name = Path::new(binary.url.as_str()).file_name();
        if archive_name.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to get archive name",
            ));
        }
        let archive_name = archive_name.unwrap();
        let tarball_path = tmp_dir.path().join(archive_name);

        // Download tar.gz to the temp directory
        progress_handler.progress(format!("downloading: {}", binary.url));
        let response = reqwest::blocking::get(binary.url.as_str());
        if response.is_err() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("failed to download: {:?}", response),
            ));
        }
        let mut response = response.unwrap();

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(tarball_path.as_path())?;
        io::copy(&mut response, &mut file)?;

        // Check the sha256
        progress_handler.progress("checking archive integrity (sha256)".to_string());
        let mut hasher = Sha256::new();
        let mut tarball_file = std::fs::File::open(&tarball_path)?;
        std::io::copy(&mut tarball_file, &mut hasher)?;
        let sha256 = format!("{:x}", hasher.finalize());
        if sha256 != binary.sha256 {
            // Hashes don't match, something went wrong
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "hashes don't match: expected {}, got {}",
                    binary.sha256, sha256
                ),
            ));
        }

        // Extract the archive in the temp directory
        progress_handler.progress("extracting binary".to_string());
        tarball_file.seek(SeekFrom::Start(0))?;
        let tar = flate2::read::GzDecoder::new(tarball_file);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(tmp_dir.path())?;

        // Replace current binary with new binary
        progress_handler.progress("updating in-place".to_string());
        let new_binary = tmp_dir.path().join("omni");
        self_replace::self_replace(new_binary)?;

        progress_handler.progress("done".to_string());
        Ok(true)
    }
}

#[derive(Debug, Deserialize)]
struct OmniReleaseBinary {
    os: String,
    arch: String,
    url: String,
    sha256: String,
}
