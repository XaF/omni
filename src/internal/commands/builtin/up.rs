use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use blake3::Hasher as Blake3Hasher;

use git_url_parse::GitUrl;
use imara_diff::intern::InternedInput;
use imara_diff::{diff, Algorithm, UnifiedDiffBuilder};
use once_cell::sync::OnceCell;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::CacheObject;
use crate::internal::cache::RepositoriesCache;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::Command;
use crate::internal::config::config;
use crate::internal::config::config_loader;
use crate::internal::config::flush_config;
use crate::internal::config::global_config;
use crate::internal::config::up::run_progress;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::ProgressHandler;
use crate::internal::config::up::SpinnerProgressHandler;
use crate::internal::config::up::UpConfig;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigExtendOptions;
use crate::internal::config::ConfigLoader;
use crate::internal::config::ConfigValue;
use crate::internal::config::SyntaxOptArg;
use crate::internal::git::format_path;
use crate::internal::git::package_path_from_git_url;
use crate::internal::git::path_entry_config;
use crate::internal::git::safe_git_url_parse;
use crate::internal::git::ORG_LOADER;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::internal::workdir::add_trust;
use crate::internal::workdir::is_trusted_or_ask;
use crate::internal::workdir_or_init;
use crate::internal::ENV;
use crate::omni_error;
use crate::omni_info;
use crate::omni_warning;

#[derive(Debug, Clone)]
struct UpCommandArgs {
    clone_suggested: UpCommandArgsCloneSuggestedOptions,
    trust: UpCommandArgsTrustOptions,
    update_repository: bool,
    update_user_config: UpCommandArgsUpdateUserConfigOptions,
}

impl UpCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("bootstrap")
                    .long("bootstrap")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("clone-suggested")
                    .long("clone-suggested")
                    .num_args(0..=1)
                    .action(clap::ArgAction::Set)
                    .default_missing_value("ask")
                    .value_parser(clap::builder::PossibleValuesParser::new([
                        "yes", "ask", "no",
                    ])),
            )
            .arg(
                clap::Arg::new("trust")
                    .long("trust")
                    .num_args(0..=1)
                    .action(clap::ArgAction::Set)
                    .default_missing_value("yes")
                    .value_parser(clap::builder::PossibleValuesParser::new([
                        "always", "yes", "no",
                    ])),
            )
            .arg(
                clap::Arg::new("update-repository")
                    .long("update-repository")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("update-user-config")
                    .long("update-user-config")
                    .num_args(0..=1)
                    .action(clap::ArgAction::Set)
                    .default_missing_value("ask")
                    .value_parser(clap::builder::PossibleValuesParser::new([
                        "yes", "ask", "no",
                    ])),
            )
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["up".to_string()]);
                }
                clap::error::ErrorKind::DisplayVersion => {
                    unreachable!("version flag is disabled");
                }
                _ => {
                    let err_str = format!("{}", err);
                    let err_str = err_str
                        .split('\n')
                        .take_while(|line| !line.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let err_str = err_str.trim_start_matches("error: ");
                    omni_error!(err_str);
                }
            }
            exit(1);
        }

        let matches = matches.unwrap();

        let bootstrap = *matches.get_one::<bool>("bootstrap").unwrap_or(&false);

        let clone_suggested =
            if let Some(clone_suggested) = matches.get_one::<String>("clone-suggested") {
                clone_suggested
                    .to_lowercase()
                    .parse::<UpCommandArgsCloneSuggestedOptions>()
                    .unwrap()
            } else if bootstrap {
                UpCommandArgsCloneSuggestedOptions::Ask
            } else {
                UpCommandArgsCloneSuggestedOptions::No
            };

        let trust = if let Some(trust) = matches.get_one::<String>("trust") {
            trust
                .to_lowercase()
                .parse::<UpCommandArgsTrustOptions>()
                .unwrap()
        } else {
            UpCommandArgsTrustOptions::Check
        };

        let update_user_config =
            if let Some(update_user_config) = matches.get_one::<String>("update-user-config") {
                update_user_config
                    .to_lowercase()
                    .parse::<UpCommandArgsUpdateUserConfigOptions>()
                    .unwrap()
            } else if bootstrap {
                UpCommandArgsUpdateUserConfigOptions::Ask
            } else {
                UpCommandArgsUpdateUserConfigOptions::No
            };

        Self {
            clone_suggested,
            trust,
            update_repository: *matches
                .get_one::<bool>("update-repository")
                .unwrap_or(&false),
            update_user_config,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum UpCommandArgsCloneSuggestedOptions {
    Yes,
    Ask,
    No,
}

impl FromStr for UpCommandArgsCloneSuggestedOptions {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "yes" => Ok(Self::Yes),
            "ask" => Ok(Self::Ask),
            "no" => Ok(Self::No),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum UpCommandArgsTrustOptions {
    Always,
    Yes,
    No,
    Check,
}

impl FromStr for UpCommandArgsTrustOptions {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "always" => Ok(Self::Always),
            "yes" => Ok(Self::Yes),
            "no" => Ok(Self::No),
            "check" => Ok(Self::Check),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum UpCommandArgsUpdateUserConfigOptions {
    Yes,
    Ask,
    No,
}

impl FromStr for UpCommandArgsUpdateUserConfigOptions {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "yes" => Ok(Self::Yes),
            "ask" => Ok(Self::Ask),
            "no" => Ok(Self::No),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpCommand {
    cli_args: OnceCell<UpCommandArgs>,
}

impl UpCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    pub fn new_command() -> Command {
        Command::BuiltinUp(Self::new())
    }

    fn cli_args(&self) -> &UpCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    pub fn name(&self) -> Vec<String> {
        vec!["up".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![vec!["down".to_string()]]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Sets up or tear down a repository depending on its \x1B[3mup\x1B[0m ",
                "configuration",
            )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "--bootstrap".to_string(),
                    desc: Some(
                        concat!(
                            "Same as using \x1B[1m--update-user-config --clone-suggested\x1B[0m; if ",
                            "any of the options are directly provided, they will take precedence over ",
                            "the default values of the options",
                        )
                        .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--clone-suggested".to_string(),
                    desc: Some(
                        concat!(
                            "Whether we should clone suggested repositories found in the configuration ",
                            "of the repository if any (yes/ask/no) ",
                            "\x1B[90m(default: no)\x1B[0m",
                        )
                        .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--trust".to_string(),
                    desc: Some(
                        "Define how to trust the repository (always/yes/no) to run the command"
                            .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--update-repository".to_string(),
                    desc: Some(
                        concat!(
                            "Whether we should update the repository before running the command; ",
                            "if the repository is already up to date, the rest of the process will ",
                            "be skipped \x1B[90m(default: no)\x1B[0m",
                        )
                        .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--update-user-config".to_string(),
                    desc: Some(
                        concat!(
                            "Whether we should handle suggestions found in the configuration of ",
                            "the repository if any (yes/ask/no); When using \x1B[3mup\x1B[0m, the ",
                            "\x1B[3msuggest_config\x1B[0m configuration will be copied to the home ",
                            "directory of the user to be loaded on every omni call ",
                            "\x1B[90m(default: no)\x1B[0m",
                        )
                        .to_string(),
                    ),
                    required: false,
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["Git commands".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if self.cli_args.set(UpCommandArgs::parse(argv)).is_err() {
            unreachable!();
        }

        let wd = workdir(".");
        if let Some(wd_root) = wd.root() {
            // Switch directory to the work directory root so it can
            // be assumed that all up commands will be ran from there
            // (e.g. custom commands, bundler commands, etc.)
            if let Err(err) = std::env::set_current_dir(wd_root) {
                omni_error!(format!(
                    "failed to change directory {}: {}",
                    format!("({})", wd_root).light_black(),
                    format!("{}", err).red()
                ));
                exit(1);
            }
        }

        if !self.update_repository() {
            // Nothing more to do if we tried updating and the
            // repo was already up to date
            exit(0);
        }

        let config = config(".");
        let up_config = config.up.clone();
        if let Some(up_config) = up_config.clone() {
            if up_config.has_errors() {
                for error in up_config.errors() {
                    omni_warning!(error);
                }
            }
        }

        let mut suggest_config = None;
        let mut suggest_config_updated = false;
        let config_loader = config_loader(".");
        if self.is_up() {
            if let Some(git_repo_config) = config_loader.raw_config.select_label("git_repo") {
                if let Some(suggestion) = git_repo_config.get("suggest_config") {
                    if suggestion.is_table() {
                        if self.should_suggest_config() {
                            suggest_config = Some(suggestion);
                        } else if let Some(wd_id) = wd.id() {
                            let suggest_config_fingerprint = fingerprint(&suggestion);
                            if !RepositoriesCache::get().check_fingerprint(
                                &wd_id,
                                "suggest_config",
                                suggest_config_fingerprint,
                            ) {
                                if self.auto_bootstrap() {
                                    suggest_config = Some(suggestion);
                                } else {
                                    suggest_config_updated = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut suggest_clone = false;
        let mut suggest_clone_updated = false;
        if self.is_up() {
            if self.should_suggest_clone() {
                suggest_clone = true;
            } else if let Some(wd_id) = wd.id() {
                let suggest_clone_fingerprint = match config.suggest_clone.repositories.len() {
                    0 => 0,
                    _ => fingerprint(&config.suggest_clone.repositories),
                };
                if !RepositoriesCache::get().check_fingerprint(
                    &wd_id,
                    "suggest_clone",
                    suggest_clone_fingerprint,
                ) {
                    if self.auto_bootstrap() {
                        suggest_clone = true;
                    } else {
                        suggest_clone_updated = true;
                    }
                }
            }
        }

        let mut env_vars = None;
        if self.is_up() && !config.env.is_empty() {
            env_vars = Some(config.env.clone());
        }

        if self.is_down() && (!wd.in_workdir() || !wd.has_id()) {
            omni_info!(format!("Outside of a work directory, nothing to do."));
            exit(0);
        }

        let has_up_config = up_config.is_some() && up_config.clone().unwrap().has_steps();
        let has_clone_suggested = !config.suggest_clone.repositories.is_empty();
        if !has_up_config
            && suggest_config.is_none()
            && (!has_clone_suggested || !suggest_clone)
            && env_vars.is_none()
        {
            omni_info!(format!(
                "No {} configuration found, nothing to do.",
                "up".italic(),
            ));
            UpConfig::clear_cache();
            exit(0);
        }

        let trust = self.trust();
        if !trust {
            omni_info!(format!(
                "Skipped running {} for this repository.",
                format!("omni {}", self.subcommand()).bold(),
            ));
            exit(0);
        }

        // If we get here, we're about to run the command, so make sure we
        // have a workdir id
        if let Err(err) = workdir_or_init(".") {
            omni_error!(format!("{}", err));
            exit(1);
        }

        // No matter what's happening after, we want a clean cache for that
        // repository, as we're rebuilding the up environment from scratch
        UpConfig::clear_cache();

        // If there are environment variables to set, do it
        if let Some(env_vars) = env_vars.clone() {
            if let Err(err) = UpEnvironmentsCache::exclusive(|up_env| {
                let wd = workdir(".");
                if let Some(workdir_id) = wd.id() {
                    up_env.set_env_vars(&workdir_id, env_vars.clone())
                } else {
                    false
                }
            }) {
                omni_warning!(format!("failed to update cache: {}", err));
            } else {
                omni_info!(format!("Repository environment configured"));
            }
        }

        // If it has an up configuration, handle it
        if has_up_config {
            let up_config = up_config.unwrap();
            if self.is_up() {
                if let Err(err) = up_config.up() {
                    omni_error!(format!("issue while setting repo up: {}", err));
                }
            } else if let Err(err) = up_config.down() {
                omni_error!(format!("issue while tearing repo down: {}", err));
            }
        }

        if let Some(suggested) = suggest_config {
            self.suggest_config(suggested);
        }

        if suggest_clone {
            self.suggest_clone();
        }

        if let Some(wd_id) = wd.id() {
            if suggest_config_updated || suggest_clone_updated {
                omni_info!(format!(
                    "configuration suggestions for {} have an update",
                    wd_id.light_blue(),
                ));
                omni_info!(format!(
                    "run {} to get the latest suggestions",
                    "omni up --bootstrap".light_yellow(),
                ));
            }
        }

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        true
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {
        println!("--bootstrap");
        println!("--clone-suggested");
        println!("--trust");
        println!("--update-repository");
        println!("--update-user-config");
        exit(0);
    }

    fn subcommand(&self) -> String {
        std::env::var("OMNI_SUBCOMMAND").unwrap_or("up".to_string())
    }

    fn is_up(&self) -> bool {
        self.subcommand() == "up"
    }

    fn is_down(&self) -> bool {
        self.subcommand() == "down"
    }

    fn trust(&self) -> bool {
        match self.cli_args().trust {
            UpCommandArgsTrustOptions::Always => return add_trust("."),
            UpCommandArgsTrustOptions::Yes => return true,
            UpCommandArgsTrustOptions::No => return false,
            UpCommandArgsTrustOptions::Check => {}
        }

        is_trusted_or_ask(
            ".",
            format!(
                "Do you want to run {} for this directory?",
                format!("omni {}", self.subcommand()).light_yellow(),
            ),
        )
    }

    fn auto_bootstrap(&self) -> bool {
        let gconfig = global_config();
        gconfig.up_command.auto_bootstrap
    }

    fn should_suggest_config(&self) -> bool {
        match self.cli_args().update_user_config {
            UpCommandArgsUpdateUserConfigOptions::Yes => true,
            UpCommandArgsUpdateUserConfigOptions::Ask => ENV.interactive_shell,
            UpCommandArgsUpdateUserConfigOptions::No => false,
        }
    }

    fn should_suggest_clone(&self) -> bool {
        match self.cli_args().clone_suggested {
            UpCommandArgsCloneSuggestedOptions::Yes => true,
            UpCommandArgsCloneSuggestedOptions::Ask => ENV.interactive_shell,
            UpCommandArgsCloneSuggestedOptions::No => false,
        }
    }

    fn suggest_config(&self, suggest_config: ConfigValue) {
        if !self.should_suggest_config() && !self.auto_bootstrap() {
            return;
        }

        let mut any_change_to_apply = false;
        let mut any_change_applied = false;

        let result = ConfigLoader::edit_main_user_config_file(|config_value| {
            let before = config_value.clone();

            let mut suggest_config = suggest_config.clone();
            suggest_config.add_label("suggest_config");
            let mut after = config_value.clone();
            after.extend(suggest_config.clone(), ConfigExtendOptions::new(), vec![]);

            // Get the yaml representation of the before and after config
            let before_yaml = before.as_yaml();
            let after_yaml = after.as_yaml();

            // Prepare the unified diff
            let input = InternedInput::new(before_yaml.as_str(), after_yaml.as_str());
            let diff = diff(
                Algorithm::Histogram,
                &input,
                UnifiedDiffBuilder::new(&input),
            );

            if diff.is_empty() {
                // No diff, nothing to do!
                return false;
            }
            any_change_to_apply = true;

            // If we got there, there is a diff, so color the lines
            let diff = color_diff(&diff);

            omni_info!("The current repository is suggesting configuration changes.");
            omni_info!(format!(
                "The following is going to be changed in your {} configuration:",
                "omni".underline()
            ));
            eprintln!("  {}", diff.replace('\n', "\n  "));

            if self.cli_args().update_user_config == UpCommandArgsUpdateUserConfigOptions::Yes {
                *config_value = after;
                any_change_applied = true;
                return true;
            }

            let choices = vec![
                ('y', "Yes, apply the changes"),
                ('n', "No, skip the changes"),
                ('s', "Split (choose which sections to apply)"),
            ];
            let question = requestty::Question::expand("apply_all_suggestions")
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message("Do you want to apply the changes?")
                .choices(choices)
                .default('y')
                .build();

            match requestty::prompt_one(question) {
                Ok(answer) => match answer {
                    requestty::Answer::ExpandItem(expanditem) => match expanditem.key {
                        'y' => {
                            *config_value = after;
                            any_change_applied = true;
                            true
                        }
                        'n' => false,
                        's' => {
                            let after =
                                self.suggest_config_split(before.clone(), suggest_config.clone());
                            if after != before {
                                *config_value = after;
                                any_change_applied = true;
                                true
                            } else {
                                false
                            }
                        }
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                },
                Err(err) => {
                    println!("{}", format!("[✘] {:?}", err).red());
                    false
                }
            }
        });

        if let Ok(_result) = result {
            if any_change_to_apply {
                if any_change_applied {
                    omni_info!("Updated user configuration");
                } else {
                    omni_info!("Skipped updating user configuration");
                }
            }
        } else {
            omni_error!(format!("Unable to update user configuration: {:?}", result));
        }

        if let Err(err) = RepositoriesCache::exclusive(|repos| match workdir(".").id() {
            Some(wd_id) => {
                repos.update_fingerprint(&wd_id, "suggest_config", fingerprint(&suggest_config))
            }
            None => false,
        }) {
            omni_warning!(format!("failed to update cache: {}", err));
        }
    }

    fn suggest_config_split(
        &self,
        before: ConfigValue,
        suggest_config: ConfigValue,
    ) -> ConfigValue {
        // We can consider this unwrap safe, since we checked the value before
        let table = suggest_config.as_table().unwrap();
        let keys = table.keys().collect::<Vec<_>>();

        let before_yaml = before.as_yaml();

        let mut choices = vec![];
        let mut split_suggestions = vec![];
        for key in keys.iter() {
            if let Some(key_suggest_config) = suggest_config.select_keys(vec![key.to_string()]) {
                let mut after = before.clone();
                after.extend(
                    key_suggest_config.clone(),
                    ConfigExtendOptions::new(),
                    vec![],
                );

                // Get the yaml representation of the specific change
                let after_yaml = after.as_yaml();

                // Prepare the unified diff
                let input = InternedInput::new(before_yaml.as_str(), after_yaml.as_str());
                let diff = diff(
                    Algorithm::Histogram,
                    &input,
                    UnifiedDiffBuilder::new(&input),
                );

                if diff.is_empty() {
                    // No diff, nothing to do!
                    continue;
                }

                choices.push((color_diff(&diff), true));
                split_suggestions.push(key_suggest_config.clone());
            }
        }

        let question = requestty::Question::multi_select("select_suggestions")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message("Which changes do you want to apply?")
            .transform(|selected, _, backend| {
                write!(backend, "{} selected", format!("{}", selected.len()).bold())
            })
            .choices_with_default(choices)
            .should_loop(false)
            .build();

        let mut after = before.clone();
        match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::ListItems(items) => {
                    for item in items {
                        after.extend(
                            split_suggestions[item.index].clone(),
                            ConfigExtendOptions::new(),
                            vec![],
                        );
                    }
                }
                _ => unreachable!(),
            },
            Err(err) => {
                println!("{}", format!("[✘] {:?}", err).red());
            }
        };

        after
    }

    fn suggest_clone(&self) {
        if !self.should_suggest_clone() && !self.auto_bootstrap() {
            return;
        }

        let wd = workdir(".");
        if let Some(wd_id) = wd.id() {
            let config = config(".");
            if !config.suggest_clone.repositories.is_empty() {
                if let Err(err) = RepositoriesCache::exclusive(|repos| {
                    repos.update_fingerprint(
                        &wd_id,
                        "suggest_clone",
                        fingerprint(&config.suggest_clone.repositories),
                    )
                }) {
                    omni_warning!(format!("failed to update cache: {}", err));
                }
            }
        }

        let to_clone = self.suggest_clone_from_config(".");
        if to_clone.is_empty() {
            return;
        }

        let cloned = self.suggest_clone_recursive(to_clone.clone(), HashSet::new());
        if cloned.is_empty() {
            return;
        }

        let current_exe = std::env::current_exe();
        if current_exe.is_err() {
            omni_error!("failed to get current executable path");
            exit(1);
        }
        let current_exe = current_exe.unwrap();

        let mut any_error = false;
        for repo in cloned {
            let repo_clone_path = if repo.clone_as_package {
                repo.package_path.unwrap()
            } else {
                repo.clone_path
            };

            let path_entry = path_entry_config(repo_clone_path.to_str().unwrap());
            if !path_entry.is_valid() {
                continue;
            }

            let location = match path_entry.package {
                Some(ref package) => format!("{}:{}", "package".underline(), package.light_cyan()),
                None => path_entry.as_string().light_cyan(),
            };

            omni_info!(format!(
                "running {} in {}",
                "omni up".light_yellow(),
                location,
            ));

            let mut omni_up_command = std::process::Command::new(current_exe.clone());
            omni_up_command.arg("up");
            omni_up_command.arg("--bootstrap");
            omni_up_command.arg("--clone-suggested=no");
            omni_up_command.current_dir(repo_clone_path);
            omni_up_command.env_remove("OMNI_FORCE_UPDATE");
            omni_up_command.env("OMNI_SKIP_UPDATE", "1");

            let child = omni_up_command.spawn();
            match child {
                Ok(mut child) => {
                    let status = child.wait();
                    match status {
                        Ok(status) => {
                            if !status.success() {
                                any_error = true;
                            }
                        }
                        Err(err) => {
                            omni_error!(format!("failed to wait on process: {}", err));
                        }
                    }
                }
                Err(err) => {
                    omni_error!(format!("failed to spawn process: {}", err));
                }
            }
        }

        omni_info!("done!".light_green());
        if any_error {
            omni_error!("some errors occurred!".light_red());
            exit(1);
        }
    }

    fn suggest_clone_from_config(&self, path: &str) -> HashSet<RepositoryToClone> {
        let git_env = git_env(path);
        let repo_id = git_env.id();
        if repo_id.is_none() {
            omni_error!("Unable to get repository id for path {}", path);
            return HashSet::new();
        }
        let repo_id = repo_id.unwrap();

        let config = config(path);

        let mut to_clone = HashSet::new();
        for repo_config in config.suggest_clone.repositories.iter() {
            let mut repo = None;

            for org in ORG_LOADER.orgs.iter() {
                if let (Some(clone_url), Some(clone_path)) = (
                    org.get_repo_git_url(&repo_config.handle),
                    org.get_repo_path(&repo_config.handle),
                ) {
                    repo = Some(RepositoryToClone {
                        suggested_by: vec![repo_id.clone()],
                        clone_url: clone_url.clone(),
                        clone_path,
                        package_path: package_path_from_git_url(&clone_url),
                        clone_args: repo_config.args.clone(),
                        clone_as_package: repo_config.clone_as_package(),
                    });
                    break;
                }
            }

            if repo.is_none() {
                if let Ok(clone_url) = safe_git_url_parse(&repo_config.handle) {
                    if clone_url.scheme.to_string() != "file"
                        && !clone_url.name.is_empty()
                        && clone_url.owner.is_some()
                        && clone_url.host.is_some()
                    {
                        let worktree = config.worktree();
                        repo = Some(RepositoryToClone {
                            suggested_by: vec![repo_id.clone()],
                            clone_url: clone_url.clone(),
                            clone_path: format_path(&worktree, &clone_url),
                            package_path: package_path_from_git_url(&clone_url),
                            clone_args: repo_config.args.clone(),
                            clone_as_package: repo_config.clone_as_package(),
                        });
                    }
                }
            }

            if repo.is_none() {
                omni_warning!(format!(
                    "Unable to determine repository path for {}",
                    repo_config.handle
                ));
                continue;
            }

            let repo = repo.unwrap();

            if repo.clone_path.exists() || repo.package_path.as_ref().map_or(false, |p| p.exists())
            {
                // Skip repository if it already exists
                continue;
            }

            to_clone.insert(repo);
        }

        to_clone
    }

    fn suggest_clone_recursive(
        &self,
        to_clone: HashSet<RepositoryToClone>,
        skipped: HashSet<RepositoryToClone>,
    ) -> HashSet<RepositoryToClone> {
        if to_clone.is_empty() {
            // No repositories to clone, we can end here
            return HashSet::new();
        }

        let (mut to_clone, new_skipped) = self.suggest_clone_ask(to_clone);
        if to_clone.is_empty() {
            // End here if there are no repositories to clone after asking
            return HashSet::new();
        }
        let to_clone_this_round = to_clone.clone();
        let skipped: HashSet<_> = skipped.union(&new_skipped).cloned().collect();

        let mut new_suggest_clone = HashSet::<RepositoryToClone>::new();
        let mut cloned = HashSet::new();

        let total = to_clone.len();
        for (idx, repo) in to_clone.iter_mut().enumerate() {
            let desc = format!("cloning {}:", repo.clone_url.light_cyan()).light_blue();
            let progress = Some((idx + 1, total));
            let progress_handler: Box<dyn ProgressHandler> = if ENV.interactive_shell {
                Box::new(SpinnerProgressHandler::new(desc, progress))
            } else {
                Box::new(PrintProgressHandler::new(desc, progress))
            };
            let progress_handler: Option<&dyn ProgressHandler> = Some(progress_handler.as_ref());

            let mut cmd_args = vec!["git".to_string(), "clone".to_string()];
            cmd_args.push(repo.clone_url.to_string());
            let repo_clone_path = if repo.clone_as_package {
                if let Some(package_path) = &repo.package_path {
                    package_path.clone()
                } else {
                    omni_error!(format!(
                        "Unable to determine package path for {}; skipping",
                        repo.clone_url.light_green()
                    ));
                    continue;
                }
            } else {
                repo.clone_path.clone()
            };
            cmd_args.push(repo_clone_path.to_string_lossy().to_string());
            cmd_args.extend(repo.clone_args.clone());

            let mut cmd = TokioCommand::new(&cmd_args[0]);
            cmd.args(&cmd_args[1..]);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let result = run_progress(&mut cmd, progress_handler, RunConfig::default());
            if result.is_ok() {
                if let Some(handler) = progress_handler {
                    handler.progress("cloned".to_string())
                }

                cloned.insert(repo.clone());

                let new_to_clone =
                    self.suggest_clone_from_config(repo_clone_path.to_str().unwrap());
                let mut num_suggested = 0;
                for new_repo in new_to_clone {
                    if skipped.contains(&new_repo)
                        || cloned.contains(&new_repo)
                        || to_clone_this_round.contains(&new_repo)
                    {
                        // Skip repository if it was already skipped
                        continue;
                    }

                    num_suggested += 1;

                    if let Some(existing_repo) = new_suggest_clone.take(&new_repo) {
                        let mut suggested_by = existing_repo.suggested_by.clone();
                        suggested_by.extend(new_repo.suggested_by.clone());

                        let args = if existing_repo.clone_args.is_empty()
                            || new_repo.clone_args.is_empty()
                        {
                            vec![]
                        } else {
                            existing_repo.clone_args.clone()
                        };

                        new_suggest_clone.insert(RepositoryToClone {
                            suggested_by,
                            clone_url: existing_repo.clone_url.clone(),
                            clone_path: existing_repo.clone_path.clone(),
                            package_path: package_path_from_git_url(&existing_repo.clone_url),
                            clone_args: args,
                            clone_as_package: existing_repo.clone_as_package
                                && new_repo.clone_as_package,
                        });
                    } else {
                        new_suggest_clone.insert(new_repo.clone());
                    }
                }

                let msg = if num_suggested > 0 {
                    format!(
                        "cloned {}",
                        format!("({} suggested)", num_suggested).light_black()
                    )
                } else {
                    "cloned".to_string()
                };
                if let Some(handler) = progress_handler {
                    handler.success_with_message(msg)
                }
            } else if let Err(err) = result {
                if let Some(handler) = progress_handler {
                    handler.error_with_message(format!("failed: {}", err))
                }
            }
        }

        // Call it recursively so we make sure we clone all the required repositories
        cloned.extend(self.suggest_clone_recursive(new_suggest_clone, skipped));

        cloned
    }

    fn suggest_clone_ask(
        &self,
        to_clone: HashSet<RepositoryToClone>,
    ) -> (Vec<RepositoryToClone>, HashSet<RepositoryToClone>) {
        // Convert from HashSet to Vec but keep content as objects, and sort by clone_url
        let to_clone = {
            let mut to_clone = to_clone.into_iter().collect::<Vec<_>>();
            to_clone.sort_by(|a, b| a.clone_url.to_string().cmp(&b.clone_url.to_string()));
            to_clone
        };

        omni_info!("The following repositories are being suggested to clone:");
        for repo in to_clone.iter() {
            omni_info!(format!(
                "- {} {} {}",
                if repo.clone_as_package {
                    "📦".to_string()
                } else {
                    "🌳".to_string()
                },
                repo.clone_url.light_green(),
                format!(
                    "{} {}{}",
                    "(from".light_black(),
                    repo.suggested_by
                        .iter()
                        .map(|x| x.light_blue())
                        .collect::<Vec<_>>()
                        .join(", "),
                    ")".light_black(),
                )
                .to_string()
                .italic(),
            ));
        }

        if self.cli_args().clone_suggested == UpCommandArgsCloneSuggestedOptions::Yes {
            return (to_clone.clone(), HashSet::new());
        }

        let mut choices = vec![
            ('y', "Yes, clone the suggested repositories"),
            ('p', "Yes, clone the suggested repositories as packages 📦"),
            (
                'w',
                "Yes, clone the suggested repositories in the worktree 🌳",
            ),
            ('n', "No, do not clone the suggested repositories"),
        ];
        if to_clone.len() > 1 {
            choices.push(('s', "Split (choose which repositories to clone)"));
        }

        let question = requestty::Question::expand("clone_suggested_repositories")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message("Do you want to clone the suggested repositories?")
            .choices(choices)
            .default('y')
            .build();

        match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::ExpandItem(expanditem) => match expanditem.key {
                    'y' => {
                        return (to_clone.clone(), HashSet::new());
                    }
                    'p' => {
                        let clone_as_package = to_clone
                            .clone()
                            .into_iter()
                            .map(|repo| {
                                let mut repo = repo.clone();
                                repo.clone_as_package = true;
                                repo
                            })
                            .collect::<Vec<_>>();
                        return (clone_as_package, HashSet::new());
                    }
                    'w' => {
                        let clone_in_worktree = to_clone
                            .clone()
                            .into_iter()
                            .map(|repo| {
                                let mut repo = repo.clone();
                                repo.clone_as_package = false;
                                repo
                            })
                            .collect::<Vec<_>>();
                        return (clone_in_worktree, HashSet::new());
                    }
                    'n' => {
                        return (vec![], HashSet::new());
                    }
                    's' => {}
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            },
            Err(err) => {
                println!("{}", format!("[✘] {:?}", err).red());
                return (vec![], HashSet::new());
            }
        }

        // If we get here, we want to split the repositories
        let mut choices = vec![];
        for repo in to_clone.iter() {
            choices.push((repo.clone_url.to_string(), true));
        }

        let question = requestty::Question::multi_select("select_repositories_to_clone")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message("Which repositories do you want to clone?")
            .transform(|selected, _, backend| {
                write!(backend, "{} selected", format!("{}", selected.len()).bold())
            })
            .choices_with_default(choices)
            .should_loop(false)
            .build();

        let mut selected_to_clone = vec![];
        match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::ListItems(items) => {
                    for item in items {
                        selected_to_clone.push(to_clone[item.index].clone());
                    }
                }
                _ => unreachable!(),
            },
            Err(err) => {
                println!("{}", format!("[✘] {:?}", err).red());
            }
        };

        // If we get here and we have any repository selected, we want to ask
        // how to clone them (package or regular cloning in the working directory)
        if !selected_to_clone.is_empty() {
            let mut choices = vec![];
            for repo in selected_to_clone.iter() {
                if repo.clone_as_package {
                    choices.push(repo.clone_url.to_string());
                }
            }
            let marker_initial_pos = choices.len();
            choices.push(
                "↑ clone as 📦 / ↓ clone in 🌳 (move this or repositories using <space>)"
                    .to_string()
                    .light_black(),
            );
            for repo in selected_to_clone.iter() {
                if !repo.clone_as_package {
                    choices.push(repo.clone_url.to_string());
                }
            }

            let question = requestty::Question::order_select("select_how_to_clone")
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message("How do you want to clone the repositories?")
                .transform(|selected, _, backend| {
                    let marker_pos = selected
                        .iter()
                        .position(|item| item.initial_index() == marker_initial_pos);
                    let count_pkg = marker_pos.unwrap_or(0);
                    let count_wt = selected.len() - count_pkg - 1;
                    write!(
                        backend,
                        "{} as 📦, {} in 🌳",
                        format!("{}", count_pkg).bold(),
                        format!("{}", count_wt).bold(),
                    )
                })
                .choices(choices)
                .build();

            match requestty::prompt_one(question) {
                Ok(answer) => match answer {
                    requestty::Answer::ListItems(items) => {
                        let mut clone_as_package = true;
                        for item in &items {
                            if item.index == marker_initial_pos {
                                clone_as_package = false;
                                continue;
                            }

                            let item_index = if item.index < marker_initial_pos {
                                item.index
                            } else {
                                // Account for the marker
                                item.index - 1
                            };

                            selected_to_clone[item_index].clone_as_package = clone_as_package;
                        }
                    }
                    _ => unreachable!(),
                },
                Err(err) => {
                    println!("{}", format!("[✘] {:?}", err).red());
                    // If we get here, we want to cancel the cloning entirely
                    selected_to_clone = vec![];
                }
            };
        }

        let skipped = to_clone
            .into_iter()
            .filter(|repo| !selected_to_clone.contains(repo))
            .collect::<HashSet<_>>();

        (selected_to_clone, skipped)
    }

    fn update_repository(&self) -> bool {
        if !self.cli_args().update_repository {
            return true;
        }

        let git_env = git_env(".");
        let repo_id = git_env.id();
        if repo_id.is_none() {
            omni_error!("Unable to get repository id");
            exit(1);
        }
        let repo_id = repo_id.unwrap();

        let config = config(".");
        let updated = config.path_repo_updates.update(&repo_id);

        if updated {
            flush_config(".");
        }

        updated
    }
}

#[derive(Debug, Eq, Clone)]
struct RepositoryToClone {
    suggested_by: Vec<String>,
    clone_url: GitUrl,
    clone_path: PathBuf,
    package_path: Option<PathBuf>,
    clone_args: Vec<String>,
    clone_as_package: bool,
}

impl Hash for RepositoryToClone {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.clone_path.hash(state);
    }
}

impl PartialEq for RepositoryToClone {
    fn eq(&self, other: &Self) -> bool {
        self.clone_path == other.clone_path
    }
}

fn color_diff(diff: &str) -> String {
    diff.lines()
        .map(|line| {
            let line = line.to_string();
            if line.starts_with('+') {
                line.green()
            } else if line.starts_with('-') {
                line.red()
            } else if line.starts_with("@@") {
                line.light_black()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fingerprint<T: Serialize>(value: &T) -> u64 {
    let string = serde_yaml::to_string(value);
    if string.is_err() {
        return 0;
    }
    let string = string.unwrap();

    let mut hasher = Blake3Hasher::new();
    hasher.update(string.as_bytes());
    let hash_bytes = hasher.finalize();
    let hash_u64 = u64::from_le_bytes(hash_bytes.as_bytes()[..8].try_into().unwrap());

    hash_u64
}
