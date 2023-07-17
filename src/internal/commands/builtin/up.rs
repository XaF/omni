use std::collections::HashMap;
use std::process::exit;
use std::str::FromStr;

use clap;
use imara_diff::intern::InternedInput;
use imara_diff::{diff, Algorithm, UnifiedDiffBuilder};
use once_cell::sync::OnceCell;
use time::OffsetDateTime;

use crate::internal::cache::Cache;
use crate::internal::cache::TrustedRepositories;
use crate::internal::cache::UpEnvironment;
use crate::internal::cache::UpEnvironments;
use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::Command;
use crate::internal::config::config;
use crate::internal::config::config_loader;
use crate::internal::config::up::UpConfig;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigExtendStrategy;
use crate::internal::config::ConfigLoader;
use crate::internal::config::ConfigValue;
use crate::internal::config::SyntaxOptArg;
use crate::internal::git::ORG_LOADER;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::CACHE;
use crate::internal::ENV;
use crate::omni_error;
use crate::omni_info;
use crate::omni_warning;

#[derive(Debug, Clone)]
struct UpCommandArgs {
    update_repository: bool,
    update_user_config: UpCommandArgsUpdateUserConfigOptions,
    trust: UpCommandArgsTrustOptions,
}

impl UpCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_flag(true)
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("help")
                    .short('h')
                    .long("help")
                    .action(clap::ArgAction::SetTrue),
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
                    .default_missing_value("yes")
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
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            let err_str = format!("{}", err);
            let err_str = err_str
                .split('\n')
                .take_while(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let err_str = err_str.trim_start_matches("error: ");
            omni_error!(err_str);
            exit(1);
        }

        let matches = matches.unwrap();

        if *matches.get_one::<bool>("help").unwrap_or(&false) {
            HelpCommand::new().exec(vec!["up".to_string()]);
            exit(1);
        }

        let update_user_config =
            if let Some(update_user_config) = matches.get_one::<String>("update-user-config") {
                update_user_config
                    .to_lowercase()
                    .parse::<UpCommandArgsUpdateUserConfigOptions>()
                    .unwrap()
            } else {
                UpCommandArgsUpdateUserConfigOptions::Ask
            };

        let trust = if let Some(trust) = matches.get_one::<String>("trust") {
            trust
                .to_lowercase()
                .parse::<UpCommandArgsTrustOptions>()
                .unwrap()
        } else {
            UpCommandArgsTrustOptions::Check
        };

        Self {
            update_repository: *matches
                .get_one::<bool>("update-repository")
                .unwrap_or(&false),
            update_user_config: update_user_config,
            trust: trust,
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
            arguments: vec![],
            options: vec![
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
                },
                SyntaxOptArg {
                    name: "--trust".to_string(),
                    desc: Some(
                        "Define how to trust the repository (always/yes/no) to run the command"
                            .to_string(),
                    ),
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["Git commands".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if let Err(_) = self.cli_args.set(UpCommandArgs::parse(argv)) {
            unreachable!();
        }

        let git = git_env(".");
        if !git.in_repo() {
            omni_error!("can only be run from a git repository");
            exit(1);
        }
        let git_root = git.root().unwrap();

        // Switch directory to the git repository root so it can
        // be assumed that all up commands will be ran from there
        // (e.g. custom commands, bundler commands, etc.)
        if let Err(err) = std::env::set_current_dir(&git_root) {
            omni_error!(format!(
                "failed to change directory {}: {}",
                format!("({})", git_root).light_black(),
                format!("{}", err).red()
            ));
            exit(1);
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
        let config_loader = config_loader(".");
        if self.is_up() && self.should_suggest_config() {
            if let Some(git_repo_config) = config_loader.raw_config.select_label("git_repo") {
                if let Some(suggestion) = git_repo_config.get("suggest_config") {
                    if suggestion.is_table() {
                        suggest_config = Some(suggestion);
                    }
                }
            }
        }

        let mut env_vars = None;
        if self.is_up() {
            if !config.env.is_empty() {
                env_vars = Some(config.env.clone());
            }
        }

        let has_up_config = !(up_config.is_none() || !up_config.clone().unwrap().has_steps());
        if !has_up_config && suggest_config.is_none() && env_vars.is_none() {
            omni_info!(format!(
                "No {} configuration found, nothing to do.",
                "up".to_string().italic(),
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

        // No matter what's happening after, we want a clean cache for that
        // repository, as we're rebuilding the up environment from scratch
        UpConfig::clear_cache();

        // If there are environment variables to set, do it
        if env_vars.is_some() {
            if let Err(err) = Cache::exclusive(|cache| {
                let git_env = git_env(".");
                let repo_id = git_env.id();
                if repo_id.is_none() {
                    return false;
                }
                let repo_id = repo_id.unwrap();

                // Update the repository up cache
                let mut up_env = HashMap::new();
                if let Some(up_cache) = &cache.up_environments {
                    up_env = up_cache.env.clone();
                }

                if !up_env.contains_key(&repo_id) {
                    up_env.insert(repo_id.clone(), UpEnvironment::new());
                }
                let repo_up_env = up_env.get_mut(&repo_id).unwrap();

                repo_up_env.env_vars = env_vars.unwrap().clone();

                cache.up_environments = Some(UpEnvironments {
                    env: up_env.clone(),
                    updated_at: OffsetDateTime::now_utc(),
                });

                true
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
            } else {
                if let Err(err) = up_config.down() {
                    omni_error!(format!("issue while tearing repo down: {}", err));
                }
            }
        }

        if suggest_config.is_some() {
            self.suggest_config(suggest_config.unwrap());
        }

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        true
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {
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

    // fn is_down(&self) -> bool {
    // self.subcommand() == "down"
    // }

    fn trust(&self) -> bool {
        match self.cli_args().trust {
            UpCommandArgsTrustOptions::Always => return self.add_trust(),
            UpCommandArgsTrustOptions::Yes => return true,
            UpCommandArgsTrustOptions::No => return false,
            UpCommandArgsTrustOptions::Check => {}
        }

        let git = git_env(".");
        for org in ORG_LOADER.orgs() {
            if org.config.trusted && org.hosts_repo(&git.origin().unwrap()) {
                return true;
            }
        }

        let repo_id = git.id();
        if repo_id.is_some() {
            if let Some(trusted_repos) = &CACHE.trusted_repositories {
                if trusted_repos
                    .repositories
                    .contains(&repo_id.clone().unwrap())
                {
                    return true;
                }
            }
        }

        if !ENV.interactive_shell {
            return false;
        }

        let mut choices = vec![('y', "Yes, this time (and ask me everytime)"), ('n', "No")];

        let repo_mention = if repo_id.is_some() {
            choices.insert(0, ('a', "Yes, always (add to trusted repositories)"));
            format!("The repository {}", repo_id.clone().unwrap().light_blue())
        } else {
            "This repository".to_string()
        };
        omni_info!(format!(
            "{} is not in your trusted repositories.",
            repo_mention
        ));
        omni_info!(format!(
            "{} repositories in your organizations are automatically trusted.",
            "Tip:".to_string().bold()
        ));

        let question = requestty::Question::expand("trust_repo")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message(format!(
                "Do you want to run {} for this repository?",
                format!("omni {}", self.subcommand()).light_yellow(),
            ))
            .choices(choices)
            .default('y')
            .build();

        return match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::ExpandItem(expanditem) => match expanditem.key {
                    'y' => true,
                    'n' => false,
                    'a' => self.add_trust(),
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            },
            Err(err) => {
                println!("{}", format!("[✘] {:?}", err).red());
                false
            }
        };
    }

    fn add_trust(&self) -> bool {
        let git = git_env(".");
        let repo_id = git.id();
        if repo_id.is_some() {
            let updated = Cache::exclusive(|cache| {
                let mut trusted = vec![];
                let repo_id = repo_id.clone().unwrap();
                if let Some(trusted_repos) = &cache.trusted_repositories {
                    trusted.extend(trusted_repos.repositories.clone());
                }
                if !trusted.contains(&repo_id) {
                    trusted.push(repo_id);
                    cache.trusted_repositories = Some(TrustedRepositories::new(trusted));
                    return true;
                }
                false
            });
            if let Err(err) = updated {
                omni_error!(format!("Unable to update cache: {:?}", err.to_string()));
                return false;
            }
        } else {
            omni_error!("Unable to get repository id");
            return false;
        }
        return true;
    }

    fn should_suggest_config(&self) -> bool {
        match self.cli_args().update_user_config {
            UpCommandArgsUpdateUserConfigOptions::Yes => true,
            UpCommandArgsUpdateUserConfigOptions::Ask => ENV.interactive_shell,
            UpCommandArgsUpdateUserConfigOptions::No => false,
        }
    }

    fn suggest_config(&self, suggest_config: ConfigValue) {
        if !self.should_suggest_config() {
            return;
        }

        let mut any_change_to_apply = false;
        let mut any_change_applied = false;

        let result = ConfigLoader::edit_main_user_config_file(|config_value| {
            let before = config_value.clone();

            let mut suggest_config = suggest_config.clone();
            suggest_config.add_label("suggest_config");
            let mut after = config_value.clone();
            after.extend(
                suggest_config.clone(),
                ConfigExtendStrategy::Default,
                vec![],
            );

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
                "omni".to_string().underline()
            ));
            eprintln!("  {}", diff.replace("\n", "\n  "));

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
                    ConfigExtendStrategy::Default,
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
                            ConfigExtendStrategy::Default,
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
        config.path_repo_updates.update(&repo_id)
    }
}

fn color_diff(diff: &str) -> String {
    diff.lines()
        .map(|line| {
            let line = line.to_string();
            if line.starts_with("+") {
                line.green()
            } else if line.starts_with("-") {
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
