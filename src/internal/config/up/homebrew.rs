use duct::cmd;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::CacheObject;
use crate::internal::cache::HomebrewInstalled;
use crate::internal::cache::HomebrewOperationCache;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::ConfigValue;
use crate::internal::env::ENV;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_warning;

static LOCAL_TAP: &str = "omni/local";
static BREW_UPDATED: OnceCell<bool> = OnceCell::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigHomebrew {
    pub install: Vec<HomebrewInstall>,
    pub tap: Vec<HomebrewTap>,
}

impl UpConfigHomebrew {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let install = HomebrewInstall::from_config_value(config_value);
        let tap = HomebrewTap::from_config_value(config_value);

        UpConfigHomebrew { install, tap }
    }

    pub fn up(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        let desc = "install homebrew dependencies:".to_string().light_blue();
        let main_progress_handler = PrintProgressHandler::new(desc, progress);
        main_progress_handler.progress("".to_string());

        let num_taps = self.tap.len();
        for (idx, tap) in self.tap.iter().enumerate() {
            if let Err(err) = tap.up(progress, Some((idx + 1, num_taps))) {
                main_progress_handler.error();
                return Err(err);
            }
        }

        let num_installs = self.install.len();
        for (idx, install) in self.install.iter().enumerate() {
            if let Err(err) = install.up(progress, Some((idx + 1, num_installs))) {
                main_progress_handler.error();
                return Err(err);
            }
        }

        let num_handled_taps = self.tap.iter().filter(|tap| tap.was_handled()).count();
        let num_handled_installs = self
            .install
            .iter()
            .filter(|install| install.was_handled())
            .count();

        main_progress_handler.success_with_message(format!(
            "installed {} tap{} and {} formula{}",
            num_handled_taps,
            if num_handled_taps > 1 { "s" } else { "" },
            num_handled_installs,
            if num_handled_installs > 1 { "s" } else { "" },
        ));

        Ok(())
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        let workdir = workdir(".");
        let repo_id = workdir.id();
        if repo_id.is_none() {
            return Ok(());
        }
        let repo_id = repo_id.unwrap();

        let mut return_value = Ok(());

        if let Err(err) = HomebrewOperationCache::exclusive(|brew_cache| {
            let desc = "uninstall (unused) homebrew dependencies:"
                .to_string()
                .light_blue();
            let main_progress_handler = PrintProgressHandler::new(desc, progress);
            main_progress_handler.progress("".to_string());

            let mut updated = false;

            let mut to_uninstall = Vec::new();
            for (idx, install) in brew_cache.installed.iter_mut().enumerate().rev() {
                if install.required_by.contains(&repo_id) {
                    install.required_by.retain(|id| id != &repo_id);
                    updated = true;
                }
                if install.required_by.is_empty() && install.installed {
                    to_uninstall.push((idx, HomebrewInstall::from_cache(install)));
                }
            }

            let num_uninstalls = to_uninstall.len();
            for (idx, (rmidx, install)) in to_uninstall.iter().enumerate() {
                if let Err(err) = install.down(progress, Some((idx + 1, num_uninstalls))) {
                    main_progress_handler.error();
                    return_value = Err(err);
                    return updated;
                }
                brew_cache.installed.remove(*rmidx);
                updated = true;
            }

            let current_installed = brew_cache.installed.len();
            brew_cache
                .installed
                .retain(|install| !install.required_by.is_empty());
            if current_installed != brew_cache.installed.len() {
                updated = true;
            }

            let mut to_untap = Vec::new();
            for (idx, tap) in brew_cache.tapped.iter_mut().enumerate().rev() {
                if tap.required_by.contains(&repo_id) {
                    tap.required_by.retain(|id| id != &repo_id);
                    updated = true;
                }
                if tap.required_by.is_empty() && tap.tapped {
                    to_untap.push((idx, HomebrewTap::from_name(&tap.name)));
                }
            }

            let num_untaps = to_untap.len();
            for (idx, (rmidx, tap)) in to_untap.iter().enumerate() {
                if let Err(err) = tap.down(progress, Some((idx + 1, num_untaps))) {
                    if err != UpError::HomebrewTapInUse {
                        main_progress_handler.error();
                        return_value = Err(err);
                        return updated;
                    }
                } else {
                    brew_cache.tapped.remove(*rmidx);
                    updated = true;
                }
            }

            let current_tapped = brew_cache.tapped.len();
            brew_cache
                .tapped
                .retain(|tap| !tap.required_by.is_empty() || tap.tapped);
            if current_tapped != brew_cache.tapped.len() {
                updated = true;
            }

            if updated {
                let num_handled_taps = to_untap
                    .iter()
                    .filter(|(_idx, tap)| tap.was_handled())
                    .count();
                let num_handled_installs = to_uninstall
                    .iter()
                    .filter(|(_idx, install)| install.was_handled())
                    .count();

                main_progress_handler.success_with_message(format!(
                    "uninstalled {} tap{} and {} formula{}",
                    num_handled_taps,
                    if num_handled_taps > 1 { "s" } else { "" },
                    num_handled_installs,
                    if num_handled_installs > 1 { "s" } else { "" },
                ));

                true
            } else {
                main_progress_handler
                    .success_with_message("no homebrew dependencies to uninstall".to_string());

                false
            }
        }) {
            omni_warning!(format!("failed to update cache: {}", err));
        }

        return_value
    }

    pub fn is_available(&self) -> bool {
        if cmd!("command", "-v", "brew")
            .stdout_null()
            .stderr_null()
            .run()
            .is_ok()
        {
            return true;
        }
        false
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewTap {
    name: String,
    url: Option<String>,

    #[serde(skip)]
    was_handled: OnceCell<bool>,
}

impl HomebrewTap {
    fn from_config_value(config_value: Option<&ConfigValue>) -> Vec<Self> {
        let mut taps = Vec::new();

        if let Some(config_value) = config_value {
            if let Some(config_value) = config_value.as_table() {
                if let Some(parsed_taps) = config_value.get("tap") {
                    taps.extend(Self::parse_taps(parsed_taps));
                }
            }
        }

        taps
    }

    fn parse_taps(taps: &ConfigValue) -> Vec<Self> {
        let mut parsed_taps = Vec::new();

        if let Some(taps_array) = taps.as_array() {
            for config_value in taps_array {
                if let Some(tap) = Self::parse_tap(None, &config_value) {
                    parsed_taps.push(tap);
                }
            }
        } else if let Some(taps_hash) = taps.as_table() {
            for (tap_name, config_value) in taps_hash {
                parsed_taps.push(Self::parse_config(tap_name.to_string(), &config_value));
            }
        } else if taps.as_str().is_some() {
            if let Some(tap) = Self::parse_tap(None, taps) {
                parsed_taps.push(tap);
            }
        }

        parsed_taps
    }

    fn parse_tap(name: Option<String>, config_value: &ConfigValue) -> Option<Self> {
        if let Some(name) = name {
            return Some(Self::parse_config(name, config_value));
        }

        if let Some(tap_str) = config_value.as_str() {
            return Some(Self {
                name: tap_str.to_string(),
                url: None,
                was_handled: OnceCell::new(),
            });
        } else if let Some(tap_hash) = config_value.as_table() {
            if let Some(name) = tap_hash.get("repo") {
                if let Some(name) = name.as_str() {
                    return Some(Self::parse_config(name, config_value));
                }
                return None;
            }

            if tap_hash.len() == 1 {
                let (name, config_value) = tap_hash.iter().next().unwrap();
                return Some(Self::parse_config(name.to_string(), config_value));
            }
        }

        None
    }

    fn parse_config(name: String, config_value: &ConfigValue) -> Self {
        let mut url = None;

        if let Some(tap_str) = config_value.as_str() {
            url = Some(tap_str.to_string());
        } else if let Some(config_value) = config_value.as_table() {
            if let Some(url_value) = config_value.get("url") {
                url = Some(url_value.as_str().unwrap().to_string());
            }
        }

        Self {
            name,
            url,
            was_handled: OnceCell::new(),
        }
    }

    fn from_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            url: None,
            was_handled: OnceCell::new(),
        }
    }

    fn update_cache(&self, progress_handler: Option<&dyn ProgressHandler>) {
        let workdir = workdir(".");
        let workdir_id = workdir.id();
        if workdir_id.is_none() {
            return;
        }
        let workdir_id = workdir_id.unwrap();

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress("updating cache".to_string())
        }

        if let Err(err) = HomebrewOperationCache::exclusive(|brew_cache| {
            brew_cache.add_tap(&workdir_id, &self.name, self.was_handled())
        }) {
            if let Some(progress_handler) = progress_handler {
                progress_handler.progress(format!("failed to update cache: {}", err))
            }
        } else if let Some(progress_handler) = progress_handler {
            progress_handler.progress("updated cache".to_string())
        }
    }

    fn up(
        &self,
        main_progress: Option<(usize, usize)>,
        sub_progress: Option<(usize, usize)>,
    ) -> Result<(), UpError> {
        let progress_str = if let Some((current, total)) = sub_progress {
            let padding = format!("{}", total).len();
            format!(
                "[{:padding$}/{:padding$}] ",
                current,
                total,
                padding = padding,
            )
        } else {
            "".to_string()
        };

        let desc = format!(
            "  {}{} {}:",
            progress_str,
            "tap".to_string().underline(),
            self.name
        )
        .light_yellow();

        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, main_progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, main_progress))
        };
        let progress_handler: Option<&dyn ProgressHandler> = Some(progress_handler.as_ref());

        if self.is_tapped() {
            self.update_cache(progress_handler);
            if let Some(progress_handler) = progress_handler {
                progress_handler.success_with_message("already tapped".light_black())
            }
            return Ok(());
        }

        if let Err(err) = self.tap(progress_handler, true) {
            if let Some(progress_handler) = progress_handler {
                progress_handler.error();
            }
            return Err(err);
        }

        self.update_cache(progress_handler);
        if let Some(progress_handler) = progress_handler {
            progress_handler.success()
        }

        Ok(())
    }

    fn down(
        &self,
        main_progress: Option<(usize, usize)>,
        sub_progress: Option<(usize, usize)>,
    ) -> Result<(), UpError> {
        let progress_str = if let Some((current, total)) = sub_progress {
            let padding = format!("{}", total).len();
            format!(
                "[{:padding$}/{:padding$}] ",
                current,
                total,
                padding = padding,
            )
        } else {
            "".to_string()
        };

        let desc = format!(
            "  {}{} {}:",
            progress_str,
            "untap".to_string().underline(),
            self.name
        )
        .light_yellow();

        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, main_progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, main_progress))
        };
        let progress_handler: Option<&dyn ProgressHandler> = Some(progress_handler.as_ref());

        if !self.is_tapped() {
            if let Some(progress_handler) = progress_handler {
                progress_handler.success_with_message("not currently tapped".light_black())
            }
            return Ok(());
        }

        if cmd!("brew", "list", "--full-name")
            .pipe(cmd!("grep", "-q", format!("^{}/", self.name)))
            .run()
            .is_ok()
        {
            if let Some(progress_handler) = progress_handler {
                progress_handler
                    .error_with_message("tap is still in use, skipping".to_string().light_black())
            }
            return Err(UpError::HomebrewTapInUse);
        }

        if let Err(err) = self.tap(progress_handler, false) {
            if let Some(progress_handler) = progress_handler {
                progress_handler.error();
            }
            return Err(err);
        }

        if let Some(progress_handler) = progress_handler {
            progress_handler.success()
        }

        Ok(())
    }

    fn is_tapped(&self) -> bool {
        let mut brew_tap_list = std::process::Command::new("brew");
        brew_tap_list.arg("tap");
        brew_tap_list.stdout(std::process::Stdio::piped());
        brew_tap_list.stderr(std::process::Stdio::null());

        if let Ok(output) = brew_tap_list.output() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                return output_str
                    .lines()
                    .any(|line| line.trim() == self.name.as_str());
            }
        }

        false
    }

    fn tap(
        &self,
        progress_handler: Option<&dyn ProgressHandler>,
        tap: bool,
    ) -> Result<(), UpError> {
        let mut brew_tap = TokioCommand::new("brew");
        if tap {
            brew_tap.arg("tap");
        } else {
            brew_tap.arg("untap");
        }
        brew_tap.arg(&self.name);

        if let Some(url) = &self.url {
            brew_tap.arg(url);
        }

        brew_tap.stdout(std::process::Stdio::piped());
        brew_tap.stderr(std::process::Stdio::piped());

        let result = run_progress(&mut brew_tap, progress_handler, RunConfig::default());
        if result.is_ok() && self.was_handled.set(true).is_err() {
            unreachable!();
        }
        result
    }

    fn was_handled(&self) -> bool {
        *self.was_handled.get_or_init(|| false)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum HomebrewInstallType {
    Formula,
    Cask,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HomebrewInstall {
    install_type: HomebrewInstallType,
    name: String,
    version: Option<String>,

    #[serde(skip)]
    was_handled: OnceCell<bool>,
}

impl HomebrewInstall {
    fn from_config_value(config_value: Option<&ConfigValue>) -> Vec<Self> {
        // TODO: maybe support "alternate" packages with `or`

        let mut installs = Vec::new();

        if let Some(config_value) = config_value {
            if let Some(config_value) = config_value.as_table() {
                if let Some(formulae) = config_value.get("install") {
                    installs.extend(Self::parse_formulae(formulae));
                }
            } else {
                installs.extend(Self::parse_formulae(config_value));
            }
        }

        installs
    }

    fn parse_formulae(formulae: &ConfigValue) -> Vec<Self> {
        let mut installs = Vec::new();

        if let Some(formulae) = formulae.as_array() {
            for formula_config_value in formulae {
                let mut install_type = HomebrewInstallType::Formula;
                let mut version = None;
                let mut name = None;

                if let Some(formula_config) = formula_config_value.as_table() {
                    let mut rest_of_config = formula_config_value.clone();

                    if let Some(formula) = formula_config.get("formula") {
                        name = Some(formula.as_str().unwrap().to_string());
                    } else if let Some(cask) = formula_config.get("cask") {
                        install_type = HomebrewInstallType::Cask;
                        name = Some(cask.as_str().unwrap().to_string());
                    } else if formula_config.len() == 1 {
                        let (key, value) = formula_config.iter().next().unwrap();
                        name = Some(key.to_string());
                        rest_of_config = value.clone();
                    }

                    let parse_version = if rest_of_config.is_str() {
                        Some(rest_of_config)
                    } else {
                        rest_of_config.get("version")
                    };

                    if let Some(parse_version) = parse_version {
                        if let Some(parse_version) = parse_version.as_str() {
                            version = Some(parse_version.to_string());
                        } else if let Some(parse_version) = parse_version.as_integer() {
                            version = Some(parse_version.to_string());
                        } else if let Some(parse_version) = parse_version.as_float() {
                            version = Some(parse_version.to_string());
                        }
                    }
                } else if let Some(formula) = formula_config_value.as_str() {
                    name = Some(formula.to_string());
                }

                if let Some(name) = name {
                    installs.push(Self {
                        install_type,
                        name,
                        version,
                        was_handled: OnceCell::new(),
                    });
                }
            }
        } else if let Some(formula) = formulae.as_str() {
            installs.push(Self {
                install_type: HomebrewInstallType::Formula,
                name: formula.to_string(),
                version: None,
                was_handled: OnceCell::new(),
            });
        }

        installs
    }

    fn from_cache(cached: &HomebrewInstalled) -> Self {
        let install_type = if cached.cask {
            HomebrewInstallType::Cask
        } else {
            HomebrewInstallType::Formula
        };

        Self {
            install_type,
            name: cached.name.clone(),
            version: cached.version.clone(),
            was_handled: OnceCell::new(),
        }
    }

    fn update_cache(&self, progress_handler: Option<&dyn ProgressHandler>) {
        let workdir = workdir(".");
        let workdir_id = workdir.id();
        if workdir_id.is_none() {
            return;
        }
        let workdir_id = workdir_id.unwrap();

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress("updating cache".to_string())
        }

        let result = HomebrewOperationCache::exclusive(|brew_cache| {
            brew_cache.add_install(
                &workdir_id,
                &self.name,
                self.version.clone(),
                self.is_cask(),
                self.was_handled(),
            )
        });

        if let Err(err) = result {
            if let Some(progress_handler) = progress_handler {
                progress_handler.progress(format!("failed to update cache: {}", err))
            }
        } else if let Some(progress_handler) = progress_handler {
            progress_handler.progress("updated cache".to_string())
        }
    }

    fn up(
        &self,
        main_progress: Option<(usize, usize)>,
        sub_progress: Option<(usize, usize)>,
    ) -> Result<(), UpError> {
        let progress_str = if let Some((current, total)) = sub_progress {
            let padding = format!("{}", total).len();
            format!(
                "[{:padding$}/{:padding$}] ",
                current,
                total,
                padding = padding,
            )
        } else {
            "".to_string()
        };

        let version_hint = if let Some(version) = &self.version {
            format!(" ({})", version)
        } else {
            "".to_string()
        };
        let desc =
            format!("  {}install {}{}:", progress_str, self.name, version_hint).light_yellow();

        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, main_progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, main_progress))
        };
        let progress_handler: Option<&dyn ProgressHandler> = Some(progress_handler.as_ref());

        let installed = self.is_installed();
        if installed && self.version.is_some() {
            self.update_cache(progress_handler);
            if let Some(progress_handler) = progress_handler {
                progress_handler.success_with_message("already installed".light_black())
            }
            return Ok(());
        }

        if let Err(err) = self.install(progress_handler, installed) {
            if let Some(progress_handler) = progress_handler {
                progress_handler.error_with_message(err.to_string());
            }
            return Err(err);
        }

        self.update_cache(progress_handler);
        if let Some(progress_handler) = progress_handler {
            progress_handler.success_with_message(
                if installed { "up to date" } else { "installed" }
                    .to_string()
                    .light_green(),
            );
        }

        Ok(())
    }

    fn down(
        &self,
        main_progress: Option<(usize, usize)>,
        sub_progress: Option<(usize, usize)>,
    ) -> Result<(), UpError> {
        let progress_str = if let Some((current, total)) = sub_progress {
            let padding = format!("{}", total).len();
            format!(
                "[{:padding$}/{:padding$}] ",
                current,
                total,
                padding = padding,
            )
        } else {
            "".to_string()
        };

        let version_hint = if let Some(version) = &self.version {
            format!(" ({})", version)
        } else {
            "".to_string()
        };
        let desc =
            format!("  {}uninstall {}{}:", progress_str, self.name, version_hint).light_yellow();

        let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
            Box::new(SpinnerProgressHandler::new(desc, main_progress))
        } else {
            Box::new(PrintProgressHandler::new(desc, main_progress))
        };
        let progress_handler: Option<&dyn ProgressHandler> = Some(progress_handler.as_ref());

        let installed = self.is_installed();
        if !installed {
            if let Some(progress_handler) = progress_handler {
                progress_handler.success_with_message("not installed".light_black())
            }
            return Ok(());
        }

        if let Err(err) = self.uninstall(progress_handler) {
            if let Some(progress_handler) = progress_handler {
                progress_handler.error_with_message(err.to_string());
            }
            return Err(err);
        }

        if self.is_in_local_tap() {
            let tapped_file = self.tapped_file().unwrap();
            if let Err(err) = std::fs::remove_file(tapped_file) {
                if let Some(progress_handler) = progress_handler {
                    progress_handler.error_with_message(format!(
                        "failed to remove formula from local tap: {}",
                        err
                    ));
                }
                return Err(UpError::Exec(
                    "failed to remove formula from local tap".to_string(),
                ));
            }

            if cmd!("brew", "list", "--full-name")
                .pipe(cmd!("grep", "-q", format!("^{}/", LOCAL_TAP)))
                .run()
                .is_err()
            {
                let mut brew_untap = TokioCommand::new("brew");
                brew_untap.arg("untap");
                brew_untap.arg(LOCAL_TAP);
                brew_untap.stdout(std::process::Stdio::piped());
                brew_untap.stderr(std::process::Stdio::piped());

                if let Some(progress_handler) = progress_handler {
                    progress_handler.progress("removing local tap".to_string());
                }

                run_progress(&mut brew_untap, progress_handler, RunConfig::default())?;
            }
        }

        if let Some(progress_handler) = progress_handler {
            progress_handler.success_with_message("uninstalled".light_green());
        }

        Ok(())
    }

    fn package_id(&self) -> String {
        format!(
            "{}{}",
            self.name,
            if let Some(version) = &self.version {
                format!("@{}", version)
            } else {
                "".to_string()
            }
        )
    }

    fn is_cask(&self) -> bool {
        self.install_type == HomebrewInstallType::Cask
    }

    fn is_installed(&self) -> bool {
        let mut brew_list = std::process::Command::new("brew");
        brew_list.arg("list");
        brew_list.arg(self.package_id());
        brew_list.stdout(std::process::Stdio::null());
        brew_list.stderr(std::process::Stdio::null());

        if let Ok(output) = brew_list.output() {
            return output.status.success();
        }

        false
    }

    fn is_in_local_tap(&self) -> bool {
        if self.version.is_none() {
            return false;
        }

        if let Some(tapped_file) = self.tapped_file() {
            let tap_path = LOCAL_TAP.replace('/', "/homebrew-");
            let pkg_type = if self.is_cask() { "Cask" } else { "Formula" };
            let expected_end = format!("{}/{}/{}.rb", tap_path, pkg_type, self.package_id());
            return tapped_file.ends_with(&expected_end);
        }

        false
    }

    fn tapped_file(&self) -> Option<String> {
        let mut brew_list = std::process::Command::new("brew");
        brew_list.arg("formula");
        brew_list.arg(self.package_id());
        brew_list.stdout(std::process::Stdio::piped());
        brew_list.stderr(std::process::Stdio::null());

        if let Ok(output) = brew_list.output() {
            if let Ok(output) = String::from_utf8(output.stdout) {
                let output = output.trim();
                if !output.is_empty() {
                    return Some(output.to_string());
                }
            }
        }

        None
    }

    fn install(
        &self,
        progress_handler: Option<&dyn ProgressHandler>,
        installed: bool,
    ) -> Result<(), UpError> {
        if !installed {
            self.extract_package(progress_handler)?;
        }

        let mut brew_tap = TokioCommand::new("brew");
        if installed {
            brew_tap.arg("upgrade");
        } else {
            brew_tap.arg("install");
        }
        if self.install_type == HomebrewInstallType::Cask {
            brew_tap.arg("--cask");
        }
        brew_tap.arg(self.package_id());

        brew_tap.stdout(std::process::Stdio::piped());
        brew_tap.stderr(std::process::Stdio::piped());

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress(if installed {
                "checking for upgrades".to_string()
            } else {
                "installing".to_string()
            })
        }

        let result = run_progress(&mut brew_tap, progress_handler, RunConfig::default());
        if result.is_ok() && self.was_handled.set(true).is_err() {
            unreachable!();
        }
        result
    }

    fn uninstall(&self, progress_handler: Option<&dyn ProgressHandler>) -> Result<(), UpError> {
        let mut brew_tap = TokioCommand::new("brew");
        brew_tap.arg("uninstall");
        if self.install_type == HomebrewInstallType::Cask {
            brew_tap.arg("--cask");
        }
        brew_tap.arg(self.package_id());

        brew_tap.stdout(std::process::Stdio::piped());
        brew_tap.stderr(std::process::Stdio::piped());

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress("uninstalling".to_string())
        }

        let result = run_progress(&mut brew_tap, progress_handler, RunConfig::default());
        if result.is_ok() && self.was_handled.set(true).is_err() {
            unreachable!();
        }
        result
    }

    fn extract_package(
        &self,
        progress_handler: Option<&dyn ProgressHandler>,
    ) -> Result<(), UpError> {
        if self.version.is_none() {
            return Ok(());
        }

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress("checking for formula".to_string())
        }

        let mut brew_info = std::process::Command::new("brew");
        brew_info.arg("info");
        brew_info.arg(self.package_id());
        brew_info.stdout(std::process::Stdio::null());
        brew_info.stderr(std::process::Stdio::null());

        if let Ok(output) = brew_info.output() {
            if output.status.success() {
                if let Some(progress_handler) = progress_handler {
                    progress_handler.progress("formula available".to_string())
                }
                return Ok(());
            }
        }

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress("checking for local tap".to_string())
        }

        let local_tap_exists = cmd!("brew", "tap")
            .pipe(cmd!("grep", "-q", LOCAL_TAP))
            .run();
        if local_tap_exists.is_err() {
            let mut brew_tap_new = TokioCommand::new("brew");
            brew_tap_new.arg("tap-new");
            brew_tap_new.arg("--no-git");
            brew_tap_new.arg(LOCAL_TAP);
            brew_tap_new.stdout(std::process::Stdio::piped());
            brew_tap_new.stderr(std::process::Stdio::piped());

            if let Some(progress_handler) = progress_handler {
                progress_handler.progress("creating local tap".to_string())
            }

            run_progress(&mut brew_tap_new, progress_handler, RunConfig::default())?;
        }
        // else {
        // let mut brew_tap_formula_exists = std::process::Command::new("brew");
        // brew_tap_formula_exists.arg("formula");
        // brew_tap_formula_exists.arg(self.package_id());
        // brew_tap_formula_exists.stdout(std::process::Stdio::piped());
        // brew_tap_formula_exists.stderr(std::process::Stdio::null());

        // progress_handler.clone().map(|progress_handler| {
        // progress_handler.progress("checking for formula".to_string())
        // });

        // // Check if the output of the command is non-empty
        // if let Ok(output) = brew_tap_formula_exists.output() {
        // if output.status.success() {
        // progress_handler.clone().map(|progress_handler| {
        // progress_handler.progress("formula found, no need to extract".to_string())
        // });
        // return Ok(());
        // }
        // }
        // }

        let brew_updated = BREW_UPDATED.get_or_init(|| {
            if let Some(progress_handler) = progress_handler {
                progress_handler.progress("updating homebrew".to_string())
            }

            let mut brew_update = TokioCommand::new("brew");
            brew_update.arg("update");
            brew_update.env("HOMEBREW_NO_INSTALL_FROM_API", "1");
            brew_update.stdout(std::process::Stdio::piped());
            brew_update.stderr(std::process::Stdio::piped());

            let result = run_progress(&mut brew_update, progress_handler, RunConfig::default());
            if result.is_err() {
                return false;
            }

            true
        });
        if !brew_updated {
            return Err(UpError::Exec("failed to update homebrew".to_string()));
        }

        let mut brew_extract = TokioCommand::new("brew");
        brew_extract.arg("extract");
        brew_extract.arg("--version");
        brew_extract.arg(self.version.as_ref().unwrap());
        brew_extract.arg(&self.name);
        brew_extract.arg(LOCAL_TAP);
        brew_extract.stdout(std::process::Stdio::piped());
        brew_extract.stderr(std::process::Stdio::piped());

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress("extracting package".to_string())
        }

        run_progress(&mut brew_extract, progress_handler, RunConfig::default())
    }

    fn was_handled(&self) -> bool {
        *self.was_handled.get_or_init(|| false)
    }
}
