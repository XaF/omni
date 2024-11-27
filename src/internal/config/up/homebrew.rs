use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use duct::cmd;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils as cache_utils;
use crate::internal::cache::CacheManagerError;
use crate::internal::cache::HomebrewOperationCache;
use crate::internal::config;
use crate::internal::config::up::utils::get_command_output;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::utils::is_executable;
use crate::internal::config::ConfigValue;
use crate::internal::env::homebrew_prefix;
use crate::internal::user_interface::StringColor;
use crate::omni_warning;

static LOCAL_TAP: &str = "omni/local";
static BREW_UPDATED: OnceCell<bool> = OnceCell::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigHomebrew {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub install: Vec<HomebrewInstall>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub tap: Vec<HomebrewTap>,
}

impl UpConfigHomebrew {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let install = HomebrewInstall::from_config_value(config_value);
        let tap = HomebrewTap::from_config_value(config_value, &install);

        UpConfigHomebrew { install, tap }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.init("homebrew:".light_blue());
        progress_handler.progress("installing homebrew dependencies".to_string());

        let num_taps = self.tap.len();
        for (idx, tap) in self.tap.iter().enumerate() {
            let subhandler = progress_handler.subhandler(
                &format!(
                    "[{current:padding$}/{total:padding$}] ",
                    current = idx + 1,
                    total = num_taps,
                    padding = format!("{}", num_taps).len(),
                )
                .light_yellow(),
            );
            tap.up(options, &subhandler)?;
        }

        let num_installs = self.install.len();
        for (idx, install) in self.install.iter().enumerate() {
            let subhandler = progress_handler.subhandler(
                &format!(
                    "[{current:padding$}/{total:padding$}] ",
                    current = idx + 1,
                    total = num_installs,
                    padding = format!("{}", num_installs).len(),
                )
                .light_yellow(),
            );
            install.up(options, environment, &subhandler)?;
        }

        progress_handler.success_with_message(self.get_up_message());

        Ok(())
    }

    pub fn was_upped(&self) -> bool {
        self.tap.iter().any(|tap| tap.was_handled())
            || self.install.iter().any(|install| install.was_handled())
    }

    pub fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        for tap in &self.tap {
            if tap.was_handled() {
                tap.commit(options, env_version_id)?;
            }
        }

        for install in &self.install {
            if install.was_handled() {
                install.commit(options, env_version_id)?;
            }
        }

        Ok(())
    }

    fn get_up_message(&self) -> String {
        let count_taps: HashMap<HomebrewHandled, usize> = self
            .tap
            .iter()
            .map(|tap| tap.handling())
            .fold(HashMap::new(), |mut map, item| {
                *map.entry(item).or_insert(0) += 1;
                map
            });
        let handled_taps: Vec<String> = self
            .tap
            .iter()
            .filter_map(|tap| match tap.handling() {
                HomebrewHandled::Handled | HomebrewHandled::Updated | HomebrewHandled::Noop => {
                    Some(tap.name.clone())
                }
                _ => None,
            })
            .sorted()
            .collect();

        let count_installs: HashMap<HomebrewHandled, usize> = self
            .install
            .iter()
            .map(|install| install.handling())
            .fold(HashMap::new(), |mut map, item| {
                *map.entry(item).or_insert(0) += 1;
                map
            });
        let handled_installs: Vec<String> = self
            .install
            .iter()
            .filter_map(|install| match install.handling() {
                HomebrewHandled::Handled | HomebrewHandled::Updated | HomebrewHandled::Noop => {
                    Some(install.package_id())
                }
                _ => None,
            })
            .sorted()
            .collect();

        let mut messages = vec![];

        for (name, count, handled) in [
            ("tap", count_taps, handled_taps),
            ("formula", count_installs, handled_installs),
        ] {
            let count_handled = handled.len();
            if count_handled == 0 {
                continue;
            }

            let mut numbers = vec![];

            if let Some(count) = count.get(&HomebrewHandled::Handled) {
                numbers.push(format!("+{}", count).green());
            }

            if let Some(count) = count.get(&HomebrewHandled::Updated) {
                numbers.push(format!("^{}", count).yellow());
            }

            if let Some(count) = count.get(&HomebrewHandled::Noop) {
                numbers.push(format!("{}", count));
            }

            if numbers.is_empty() {
                continue;
            }

            messages.push(format!(
                "{} {}{} {}",
                numbers.join(", "),
                name,
                if count_handled > 1 { "s" } else { "" },
                format!("({})", handled.join(", ")).light_black().italic(),
            ));
        }

        if messages.is_empty() {
            "nothing done".to_string()
        } else {
            messages.join(" and ")
        }
    }

    pub fn down(&self, _progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        Ok(())
    }

    pub fn cleanup(progress_handler: &UpProgressHandler) -> Result<Option<String>, UpError> {
        let mut untapped = vec![];
        let mut uninstalled = vec![];

        progress_handler.init("homebrew:".light_blue());
        progress_handler.progress("checking for unused homebrew dependencies".to_string());

        let cache = HomebrewOperationCache::get();
        cache
            .cleanup(
                |install_name, install_version, is_cask, (idx, total)| {
                    let install =
                        HomebrewInstall::from_cache(install_name, install_version, is_cask);

                    let subhandler = progress_handler.subhandler(
                        &format!(
                            "[{current:padding$}/{total:padding$}] ",
                            current = idx + 1,
                            total = total,
                            padding = format!("{}", total).len(),
                        )
                        .light_yellow(),
                    );

                    match install.down(&subhandler) {
                        Err(err) => {
                            subhandler.error();
                            Err(CacheManagerError::Other(err.to_string()))
                        }
                        Ok(_) => {
                            uninstalled.push(install);
                            Ok(())
                        }
                    }
                },
                |tap_name, (idx, total)| {
                    let tap = HomebrewTap::from_name(tap_name);

                    let subhandler = progress_handler.subhandler(
                        &format!(
                            "[{current:padding$}/{total:padding$}] ",
                            current = idx + 1,
                            total = total,
                            padding = format!("{}", total).len(),
                        )
                        .light_yellow(),
                    );

                    match tap.down(&subhandler) {
                        Err(err) => {
                            // If the error is that the tap is still in use, we'll consider this a success
                            // and make it so omni does not own the tap installation anymore
                            if err != UpError::HomebrewTapInUse {
                                subhandler.error();
                                Err(CacheManagerError::Other(err.to_string()))
                            } else {
                                Ok(())
                            }
                        }
                        Ok(_) => {
                            untapped.push(tap);
                            Ok(())
                        }
                    }
                },
            )
            .map_err(|err| UpError::Cache(err.to_string()))?;

        let mut messages = Vec::new();

        if !untapped.is_empty() || !uninstalled.is_empty() {
            let handled_taps = untapped
                .iter()
                .filter(|tap| tap.was_handled())
                .collect::<Vec<_>>();
            let handled_taps_count = handled_taps.len();
            if handled_taps_count > 0 {
                messages.push(format!(
                    "{} tap{} {}",
                    format!("-{}", handled_taps_count).red(),
                    if handled_taps_count > 1 { "s" } else { "" },
                    format!(
                        "({})",
                        handled_taps
                            .iter()
                            .map(|tap| tap.name.clone())
                            .sorted()
                            .join(", ")
                    )
                    .light_black()
                    .italic(),
                ));
            }

            let handled_installs = uninstalled
                .iter()
                .filter(|install| install.was_handled())
                .collect::<Vec<_>>();
            let handled_installs_count = handled_installs.len();
            if handled_installs_count > 0 {
                messages.push(format!(
                    "{} formula{} {}",
                    format!("-{}", handled_installs_count).red(),
                    if handled_installs_count > 1 { "s" } else { "" },
                    format!(
                        "({})",
                        handled_installs
                            .iter()
                            .map(|install| install.name())
                            .sorted()
                            .join(", ")
                    )
                    .light_black()
                    .italic(),
                ));
            }
        }

        if messages.is_empty() {
            Ok(None)
        } else {
            Ok(Some(messages.join(" and ")))
        }
    }

    pub fn is_available(&self) -> bool {
        which::which("brew").is_ok()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
pub enum HomebrewHandled {
    Handled,
    Noop,
    Updated,
    Unhandled,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct HomebrewTap {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    upgrade: bool,

    #[serde(skip)]
    was_handled: OnceCell<HomebrewHandled>,
}

impl PartialOrd for HomebrewTap {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HomebrewTap {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl HomebrewTap {
    fn from_config_value(
        config_value: Option<&ConfigValue>,
        installs: &[HomebrewInstall],
    ) -> Vec<Self> {
        #[allow(clippy::mutable_key_type)]
        let mut taps = BTreeSet::new();

        if let Some(config_value) = config_value {
            if let Some(config_value) = config_value.as_table() {
                if let Some(parsed_taps) = config_value.get("tap") {
                    taps.extend(Self::parse_taps(parsed_taps));
                }
            }
        }

        for install in installs {
            // If the formula name is `a/b/c`, then really the formula
            // name is `c` and the tap is `a/b`. We can use this to
            // force-add the tap to the list of taps if it was not
            // explicitly defined in the configuration
            let split = install.name.split('/').collect::<Vec<_>>();
            if split.len() == 3 {
                let tap_name = format!("{}/{}", split[0], split[1]);
                taps.insert(Self::from_name(&tap_name));
            }
        }

        taps.into_iter().collect()
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
                upgrade: false,
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
        let mut upgrade = false;

        if let Some(tap_str) = config_value.as_str() {
            url = Some(tap_str.to_string());
        } else if let Some(config_value) = config_value.as_table() {
            if let Some(url_value) = config_value.get("url") {
                url = Some(url_value.as_str().unwrap().to_string());
            }

            if let Some(upgrade_value) = config_value.get("upgrade") {
                if let Some(upgrade_bool) = upgrade_value.as_bool_forced() {
                    upgrade = upgrade_bool;
                }
            }
        }

        Self {
            name,
            url,
            upgrade,
            was_handled: OnceCell::new(),
        }
    }

    fn from_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            url: None,
            upgrade: false,
            was_handled: OnceCell::new(),
        }
    }

    fn update_cache(&self, progress_handler: &dyn ProgressHandler) {
        progress_handler.progress("updating cache".to_string());

        if let Err(err) = HomebrewOperationCache::get().add_tap(&self.name, self.was_handled()) {
            progress_handler.progress(format!("failed to update cache: {}", err));
        } else {
            progress_handler.progress("updated cache".to_string());
        }
    }

    fn commit(&self, _options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        if self.was_handled() {
            if let Err(err) =
                HomebrewOperationCache::get().add_tap_required_by(env_version_id, &self.name)
            {
                return Err(UpError::Cache(err.to_string()));
            }
        }

        Ok(())
    }

    fn up(&self, options: &UpOptions, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        let progress_handler = progress_handler
            .subhandler(&format!("{} {}: ", "tap".underline(), self.name).light_yellow());

        let is_tapped = self.is_tapped();
        match self.tap(options, &progress_handler, is_tapped) {
            Ok(did_something) => {
                let (was_handled, message) = match (is_tapped, did_something) {
                    (true, true) => (HomebrewHandled::Updated, "updated".light_green()),
                    (true, false) => (HomebrewHandled::Noop, "up to date (cached)".light_black()),
                    (false, _) => (HomebrewHandled::Handled, "tapped".light_green()),
                };
                if self.was_handled.set(was_handled).is_err() {
                    unreachable!("failed to set was_handled (install: {})", self.name);
                }
                self.update_cache(&progress_handler);
                progress_handler.success_with_message(message);
                Ok(())
            }
            Err(err) => {
                progress_handler.error_with_message(err.to_string());
                Err(err)
            }
        }
    }

    fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        let progress_handler = progress_handler
            .subhandler(&format!("{} {}: ", "untap".underline(), self.name).light_yellow());

        if !self.is_tapped() {
            progress_handler.success_with_message("not currently tapped".light_black());
            return Ok(());
        }

        if cmd!("brew", "list", "--full-name")
            .pipe(cmd!("grep", "-q", format!("^{}/", self.name)))
            .run()
            .is_ok()
        {
            progress_handler.error_with_message("tap is still in use, skipping".light_black());
            return Err(UpError::HomebrewTapInUse);
        }

        if let Err(err) = self.untap(&progress_handler) {
            progress_handler.error();
            return Err(err);
        }

        if self.was_handled.set(HomebrewHandled::Handled).is_err() {
            unreachable!("failed to set was_handled (tap: {})", self.name);
        }

        progress_handler.success_with_message("untapped".light_green());

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

    fn upgrade_tap(&self, options: &UpOptions) -> bool {
        self.upgrade || options.upgrade || config(".").up_command.upgrade
    }

    fn update_tap(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<bool, UpError> {
        if !self.upgrade_tap(options) {
            progress_handler.progress("already tapped".light_black());
            return Ok(false);
        } else if options.read_cache && !HomebrewOperationCache::get().should_update_tap(&self.name)
        {
            progress_handler.progress("already up to date".light_black());
            return Ok(false);
        }

        progress_handler.progress("updating tap".to_string());

        // Get the path to the tap repository
        let mut brew_repo = TokioCommand::new("brew");
        brew_repo.arg("--repo");
        brew_repo.arg(&self.name);
        brew_repo.stdout(std::process::Stdio::piped());
        brew_repo.stderr(std::process::Stdio::piped());

        let output = get_command_output(&mut brew_repo, RunConfig::default());
        let brew_repo_path = match output {
            Err(err) => {
                let msg = format!("failed to get tap repository path: {}", err);
                progress_handler.error_with_message(msg.clone());
                return Err(UpError::Exec(msg));
            }
            Ok(output) if !output.status.success() => {
                let msg = format!(
                    "failed to get tap repository path: {}",
                    String::from_utf8(output.stderr)
                        .unwrap()
                        .replace('\n', " ")
                        .trim()
                );
                progress_handler.error_with_message(msg.clone());
                return Err(UpError::Exec(msg));
            }
            Ok(output) => {
                let output = String::from_utf8(output.stdout).unwrap().trim().to_string();
                PathBuf::from(output)
            }
        };

        // Now run `git pull` in the tap repository
        let mut git_pull = TokioCommand::new("git");
        git_pull.arg("pull");
        git_pull.current_dir(brew_repo_path);
        git_pull.stdout(std::process::Stdio::piped());
        git_pull.stderr(std::process::Stdio::piped());

        let output = get_command_output(&mut brew_repo, RunConfig::new().with_askpass());
        match output {
            Err(err) => {
                let msg = format!("git pull failed: {}", err);
                Err(UpError::Exec(msg))
            }
            Ok(output) if !output.status.success() => {
                let msg = format!(
                    "git pull failed: {}",
                    String::from_utf8(output.stderr)
                        .unwrap()
                        .replace('\n', " ")
                        .trim()
                );
                Err(UpError::Exec(msg))
            }
            Ok(output) => {
                let output = String::from_utf8(output.stdout).unwrap().trim().to_string();
                let output_lines = output.lines().collect::<Vec<&str>>();

                if output_lines.len() == 1 && output_lines[0].contains("Already up to date.") {
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
        }
    }

    fn tap(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
        is_tapped: bool,
    ) -> Result<bool, UpError> {
        let result = if is_tapped {
            self.update_tap(options, progress_handler)
        } else {
            let mut brew_tap = TokioCommand::new("brew");
            brew_tap.arg("tap");
            brew_tap.arg(&self.name);

            if let Some(url) = &self.url {
                brew_tap.arg(url);
            }

            brew_tap.stdout(std::process::Stdio::piped());
            brew_tap.stderr(std::process::Stdio::piped());

            match run_progress(&mut brew_tap, Some(progress_handler), RunConfig::default()) {
                Ok(_) => Ok(true),
                Err(err) => Err(err),
            }
        };

        match result {
            Ok(true) if options.write_cache => {
                // Update the cache
                if let Err(err) = HomebrewOperationCache::get().updated_tap(&self.name) {
                    return Err(UpError::Cache(err.to_string()));
                }

                result
            }
            Err(err) if !options.fail_on_upgrade => {
                progress_handler.progress(format!("failed to update: {}", err).red());
                Ok(false)
            }
            _ => result,
        }
    }

    fn untap(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        let mut brew_untap = TokioCommand::new("brew");
        brew_untap.arg("untap");
        brew_untap.arg(&self.name);
        brew_untap.stdout(std::process::Stdio::piped());
        brew_untap.stderr(std::process::Stdio::piped());

        run_progress(
            &mut brew_untap,
            Some(progress_handler),
            RunConfig::default(),
        )
    }

    fn was_handled(&self) -> bool {
        matches!(self.was_handled.get(), Some(HomebrewHandled::Handled))
    }

    fn handling(&self) -> HomebrewHandled {
        match self.was_handled.get() {
            Some(handled) => handled.clone(),
            None => HomebrewHandled::Unhandled,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum HomebrewInstallType {
    Formula,
    Cask,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HomebrewInstall {
    install_type: HomebrewInstallType,
    name: String,
    version: Option<String>,
    upgrade: bool,

    #[serde(skip)]
    was_handled: OnceCell<HomebrewHandled>,
}

impl Serialize for HomebrewInstall {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        match (self.install_type.clone(), self.upgrade, self.version()) {
            (HomebrewInstallType::Formula, false, None) => self.name.serialize(serializer),
            (HomebrewInstallType::Formula, false, Some(version)) => {
                let mut install = HashMap::new();
                install.insert(self.name.clone(), version);
                install.serialize(serializer)
            }
            (install_type, upgrade, version) => {
                let mut install = HashMap::new();

                let install_type = match install_type {
                    HomebrewInstallType::Formula => "formula",
                    HomebrewInstallType::Cask => "cask",
                };
                install.insert(install_type.to_string(), self.name.clone());

                if let Some(version) = &version {
                    install.insert("version".to_string(), version.clone());
                }
                install.insert("upgrade".to_string(), upgrade.to_string());

                install.serialize(serializer)
            }
        }
    }
}

impl HomebrewInstall {
    pub fn new_formula(name: &str) -> Self {
        Self {
            install_type: HomebrewInstallType::Formula,
            name: name.to_string(),
            version: None,
            upgrade: false,
            was_handled: OnceCell::new(),
        }
    }

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
                let mut upgrade = false;

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
                        if let Some(upgrade_value) = rest_of_config.get("upgrade") {
                            if let Some(upgrade_bool) = upgrade_value.as_bool_forced() {
                                upgrade = upgrade_bool;
                            }
                        }

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
                        upgrade,
                        was_handled: OnceCell::new(),
                    });
                }
            }
        } else if let Some(formula) = formulae.as_str() {
            installs.push(Self::new_formula(&formula));
        }

        installs
    }

    fn from_cache(name: &str, version: Option<&str>, is_cask: bool) -> Self {
        let install_type = if is_cask {
            HomebrewInstallType::Cask
        } else {
            HomebrewInstallType::Formula
        };

        Self {
            install_type,
            name: name.to_string(),
            version: version.map(|version| version.to_string()),
            upgrade: false,
            was_handled: OnceCell::new(),
        }
    }

    fn update_cache(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &dyn ProgressHandler,
    ) {
        progress_handler.progress("updating cache".to_string());

        if let Err(err) = HomebrewOperationCache::get().add_install(
            &self.name,
            self.version(),
            self.is_cask(),
            self.was_handled(),
        ) {
            progress_handler.progress(format!("failed to update cache: {}", err));
            return;
        }

        let mut bin_paths = self.bin_paths(options);
        if let Some(bin_path) = self.brew_bin_path(options) {
            bin_paths.push(bin_path);
        }

        if !bin_paths.is_empty() {
            for bin_path in bin_paths {
                environment.add_path(bin_path);
            }
        }

        progress_handler.progress("updated cache".to_string());
    }

    fn commit(&self, _options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        if self.was_handled() {
            if let Err(err) = HomebrewOperationCache::get().add_install_required_by(
                env_version_id,
                &self.name,
                self.version(),
                self.is_cask(),
            ) {
                return Err(UpError::Cache(err.to_string()));
            }
        }

        Ok(())
    }

    fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        let progress_handler = progress_handler.subhandler(
            &format!(
                "install {}{}: ",
                self.name,
                match &self.version {
                    Some(version) => format!(" ({})", version),
                    None => "".to_string(),
                }
            )
            .light_yellow(),
        );

        let installed = self.is_installed(options);
        if installed && self.version.is_some() {
            if self.was_handled.set(HomebrewHandled::Noop).is_err() {
                unreachable!("failed to set was_handled (install: {})", self.name);
            }
            self.update_cache(options, environment, &progress_handler);
            progress_handler.success_with_message("already installed".light_black());
            return Ok(());
        }

        match self.install(options, Some(&progress_handler), installed) {
            Ok(did_something) => {
                let (was_handled, message) = match (installed, did_something) {
                    (true, true) => (HomebrewHandled::Updated, "updated".light_green()),
                    (true, false) => (HomebrewHandled::Noop, "up to date (cached)".light_black()),
                    (false, _) => (HomebrewHandled::Handled, "installed".light_green()),
                };
                if self.was_handled.set(was_handled).is_err() {
                    unreachable!("failed to set was_handled (install: {})", self.name);
                }
                self.update_cache(options, environment, &progress_handler);
                progress_handler.success_with_message(message);
                Ok(())
            }
            Err(err) => {
                progress_handler.error_with_message(err.to_string());
                Err(err)
            }
        }
    }

    fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        let progress_handler = progress_handler.subhandler(
            &format!(
                "uninstall {}{}: ",
                self.name,
                match &self.version {
                    Some(version) => format!(" ({})", version),
                    None => "".to_string(),
                }
            )
            .light_yellow(),
        );

        let installed = self.is_installed(&UpOptions::new().cache_disabled());
        if !installed {
            progress_handler.success_with_message("not installed".light_black());
            return Ok(());
        }

        if let Err(err) = self.uninstall(&progress_handler) {
            progress_handler.error_with_message(err.to_string());
            return Err(err);
        }

        if self.is_in_local_tap() {
            let tapped_file = self.tapped_file().unwrap();
            if let Err(err) = std::fs::remove_file(tapped_file) {
                progress_handler.error_with_message(format!(
                    "failed to remove formula from local tap: {}",
                    err
                ));
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

                progress_handler.progress("removing local tap".to_string());

                run_progress(
                    &mut brew_untap,
                    Some(&progress_handler),
                    RunConfig::default(),
                )?;
            }
        }

        progress_handler.success_with_message("uninstalled".light_green());

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

    fn name(&self) -> String {
        self.name.clone()
    }

    fn version(&self) -> Option<String> {
        self.version.clone()
    }

    fn is_cask(&self) -> bool {
        self.install_type == HomebrewInstallType::Cask
    }

    fn is_installed(&self, options: &UpOptions) -> bool {
        let cache = HomebrewOperationCache::get();
        if options.read_cache
            && !cache.should_check_install(&self.name, self.version(), self.is_cask())
        {
            return true;
        }

        let mut brew_list = std::process::Command::new("brew");
        brew_list.arg("list");
        if self.is_cask() {
            brew_list.arg("--cask");
        } else {
            brew_list.arg("--formula");
        }
        brew_list.arg(self.package_id());
        brew_list.stdout(std::process::Stdio::null());
        brew_list.stderr(std::process::Stdio::null());

        if let Ok(output) = brew_list.output() {
            if output.status.success() {
                if options.write_cache {
                    // Update the cache
                    if let Err(err) =
                        cache.checked_install(&self.name, self.version(), self.is_cask())
                    {
                        omni_warning!(format!("failed to update cache: {}", err));
                    }
                }
                return true;
            }
        }

        false
    }

    fn brew_bin_path(&self, options: &UpOptions) -> Option<PathBuf> {
        if options.read_cache {
            if let Some(bin_path) = HomebrewOperationCache::get().homebrew_bin_path() {
                if !bin_path.is_empty() {
                    return Some(bin_path.into());
                }
            }
        }

        if let Some(brew_prefix) = homebrew_prefix() {
            let bin_path = PathBuf::from(brew_prefix).join("bin");
            if bin_path.exists() {
                if options.write_cache {
                    // Update the cache
                    if let Err(err) = HomebrewOperationCache::get()
                        .set_homebrew_bin_path(bin_path.to_string_lossy().to_string())
                    {
                        omni_warning!(format!("failed to update cache: {}", err));
                    }
                }

                return Some(bin_path);
            }
        }

        None
    }

    fn bin_paths_from_cask(&self) -> Vec<PathBuf> {
        if !self.is_cask() {
            return vec![];
        }

        // brew --prefix doesn't work for casks, so we can try to
        // check if there is any bin/ directory in the cask path
        let brew_prefix = match homebrew_prefix() {
            Some(prefix) => prefix,
            None => return vec![],
        };

        // Prepare the prefix path for the cask
        let bin_lookup_prefix = PathBuf::from(brew_prefix)
            .join("Caskroom")
            .join(self.package_id());

        // Generate the glob path we can use to search for the bin directory
        let glob_path = bin_lookup_prefix.join("**").join("bin");
        let glob_path = match glob_path.to_str() {
            Some(glob_path) => glob_path,
            None => return vec![],
        };

        // Search for the bin directory
        let entries = if let Ok(entries) = glob::glob(glob_path) {
            entries
        } else {
            return vec![];
        };

        let mut bin_paths = HashSet::new();
        for path in entries.into_iter().flatten() {
            if !path.is_dir() {
                continue;
            }

            // Get the relative path to the bin directory
            let prefix = format!("{}/", bin_lookup_prefix.to_string_lossy());
            let relpath = match path.strip_prefix(&prefix) {
                Ok(relpath) => relpath,
                Err(_) => continue,
            };

            // Check if any directories are starting with dot,
            // and if so, skip the directory
            if relpath
                .components()
                .any(|comp| comp.as_os_str().to_string_lossy().starts_with('.'))
            {
                continue;
            }

            // Get the canonical path to the bin directory
            let path = match path.canonicalize() {
                Ok(path) => path,
                Err(_) => continue,
            };

            // If the path is already in the set, skip it
            if bin_paths.contains(&path) {
                continue;
            }

            // Check if the directory contains any executable
            // files, i.e. files with the +x flag, and if not,
            // skip the directory
            let mut has_executables = false;
            if let Ok(files) = std::fs::read_dir(&path) {
                for entry in files.flatten() {
                    let filepath = entry.path();
                    let filetype = match entry.file_type() {
                        Ok(filetype) => filetype,
                        Err(_) => continue,
                    };

                    if !filetype.is_file() || !is_executable(&filepath) {
                        continue;
                    }

                    // We want to make sure it's binaries without dots
                    // in the filename, as those are usually not meant to
                    // be in the path
                    let filename = match filepath.file_name() {
                        Some(filename) => filename,
                        None => continue,
                    };
                    if filename.to_string_lossy().contains('.') {
                        continue;
                    }

                    has_executables = true;
                }
            }
            if !has_executables {
                continue;
            }

            bin_paths.insert(path);
        }

        bin_paths.into_iter().collect()
    }

    fn bin_paths_from_formula(&self) -> Vec<PathBuf> {
        if self.is_cask() {
            return vec![];
        }

        let mut brew_list = std::process::Command::new("brew");
        brew_list.arg("--prefix");
        brew_list.arg("--installed");
        brew_list.arg(self.package_id());
        brew_list.stdout(std::process::Stdio::piped());
        brew_list.stderr(std::process::Stdio::null());

        if let Ok(output) = brew_list.output() {
            if output.status.success() {
                let bin_path =
                    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim()).join("bin");
                let bin_path = if bin_path.exists() {
                    bin_path
                } else {
                    PathBuf::from("")
                };

                if !bin_path.to_string_lossy().is_empty() {
                    return vec![bin_path];
                }
            }
        }

        vec![]
    }

    fn bin_paths(&self, options: &UpOptions) -> Vec<PathBuf> {
        let cache = HomebrewOperationCache::get();
        if options.read_cache {
            if let Some(bin_paths) =
                cache.homebrew_install_bin_paths(&self.name, self.version(), self.is_cask())
            {
                return bin_paths.iter().map(PathBuf::from).collect();
            }
        }

        let bin_paths = if self.is_cask() {
            self.bin_paths_from_cask()
        } else {
            self.bin_paths_from_formula()
        };

        if options.write_cache {
            if let Err(err) = cache.set_homebrew_install_bin_paths(
                &self.name,
                self.version(),
                self.is_cask(),
                bin_paths
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect(),
            ) {
                omni_warning!(format!("failed to update cache: {}", err));
            }
        }

        bin_paths
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

    fn upgrade_install(&self, options: &UpOptions) -> bool {
        self.upgrade || options.upgrade || config(".").up_command.upgrade
    }

    fn install(
        &self,
        options: &UpOptions,
        progress_handler: Option<&dyn ProgressHandler>,
        installed: bool,
    ) -> Result<bool, UpError> {
        let cache = HomebrewOperationCache::get();

        if !installed {
            self.extract_package(options, progress_handler)?;
        } else if !self.upgrade_install(options) {
            if let Some(progress_handler) = progress_handler {
                progress_handler.progress("already installed".light_black())
            }

            return Ok(false);
        } else if options.read_cache
            && !cache.should_update_install(&self.name, self.version(), self.is_cask())
        {
            if let Some(progress_handler) = progress_handler {
                progress_handler.progress("already up to date".light_black())
            }

            return Ok(false);
        }

        let mut run_config = RunConfig::default();

        let mut brew_install = TokioCommand::new("brew");
        if installed {
            brew_install.arg("upgrade");
        } else {
            brew_install.arg("install");
        }
        if self.install_type == HomebrewInstallType::Cask {
            brew_install.arg("--cask");
            run_config.with_askpass();
        } else {
            brew_install.arg("--formula");
        }
        brew_install.arg(self.package_id());

        brew_install.env("HOMEBREW_NO_AUTO_UPDATE", "1");
        brew_install.env("HOMEBREW_NO_INSTALL_UPGRADE", "1");
        brew_install.stdout(std::process::Stdio::piped());
        brew_install.stderr(std::process::Stdio::piped());

        if let Some(progress_handler) = progress_handler {
            progress_handler.progress(if installed {
                "checking for upgrades".to_string()
            } else {
                "installing".to_string()
            })
        }

        match run_progress(&mut brew_install, progress_handler, run_config) {
            Ok(_) => {
                if options.write_cache {
                    // Update the cache
                    if let Err(err) =
                        cache.updated_install(&self.name, self.version(), self.is_cask())
                    {
                        return Err(UpError::Cache(err.to_string()));
                    }
                }

                Ok(true)
            }
            Err(err) => {
                if options.fail_on_upgrade {
                    Err(err)
                } else {
                    if let Some(progress_handler) = progress_handler {
                        progress_handler.progress(format!("failed to upgrade: {}", err).red())
                    }

                    Ok(false)
                }
            }
        }
    }

    fn uninstall(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        let mut brew_uninstall = TokioCommand::new("brew");
        brew_uninstall.arg("uninstall");
        if self.install_type == HomebrewInstallType::Cask {
            brew_uninstall.arg("--cask");
        } else {
            brew_uninstall.arg("--formula");
        }
        brew_uninstall.arg(self.package_id());

        brew_uninstall.stdout(std::process::Stdio::piped());
        brew_uninstall.stderr(std::process::Stdio::piped());

        progress_handler.progress("uninstalling".to_string());

        let result = run_progress(
            &mut brew_uninstall,
            Some(progress_handler),
            RunConfig::default(),
        );
        if result.is_ok() && self.was_handled.set(HomebrewHandled::Handled).is_err() {
            unreachable!("failed to set was_handled (install: {})", self.name);
        }
        result
    }

    fn extract_package(
        &self,
        options: &UpOptions,
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

        let cache = HomebrewOperationCache::get();
        if !options.read_cache || cache.should_update_homebrew() {
            let mut update_brew_cache = false;
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

                update_brew_cache = true;

                true
            });
            if !brew_updated {
                return Err(UpError::Exec("failed to update homebrew".to_string()));
            }

            if options.write_cache && update_brew_cache {
                // Update the cache
                if let Err(err) = cache.updated_homebrew() {
                    return Err(UpError::Cache(err.to_string()));
                }
            }
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
        matches!(self.was_handled.get(), Some(HomebrewHandled::Handled))
    }

    fn handling(&self) -> HomebrewHandled {
        match self.was_handled.get() {
            Some(handled) => handled.clone(),
            None => HomebrewHandled::Unhandled,
        }
    }
}
