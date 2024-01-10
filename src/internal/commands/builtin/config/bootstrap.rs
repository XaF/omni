use std::collections::HashMap;
use std::collections::HashSet;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;

use itertools::Itertools;
use once_cell::sync::OnceCell;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::builtin::TidyGitRepo;
use crate::internal::commands::utils::abs_path;
use crate::internal::commands::utils::file_auto_complete;
use crate::internal::config::global_config;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigExtendOptions;
use crate::internal::config::ConfigExtendStrategy;
use crate::internal::config::ConfigLoader;
use crate::internal::config::ConfigValue;
use crate::internal::config::OrgConfig;
use crate::internal::config::SyntaxOptArg;
use crate::internal::env::shell_integration_is_loaded;
use crate::internal::env::user_home;
use crate::internal::env::Shell;
use crate::internal::git::format_path_with_template;
use crate::internal::git::full_git_url_parse;
use crate::internal::git::Org;
use crate::internal::user_interface::StringColor;
use crate::omni_error;
use crate::omni_info;
use crate::omni_warning;

#[derive(Debug, Clone)]
struct ConfigBootstrapCommandArgs {
    options: ConfigBootstrapOptions,
}

impl ConfigBootstrapCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("worktree")
                    .long("worktree")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("repo-path-format")
                    .long("repo-path-format")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("organizations")
                    .long("organizations")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("shell")
                    .long("shell")
                    .action(clap::ArgAction::SetTrue),
            )
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["config".to_string(), "bootstrap".to_string()]);
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

        if *matches.get_one::<bool>("help").unwrap_or(&false) {
            HelpCommand::new().exec(vec!["config".to_string(), "bootstrap".to_string()]);
            exit(1);
        }

        let mut worktree = *matches.get_one::<bool>("worktree").unwrap_or(&false);
        let mut repo_path_format = *matches
            .get_one::<bool>("repo-path-format")
            .unwrap_or(&false);
        let mut organizations = *matches.get_one::<bool>("organizations").unwrap_or(&false);
        let mut shell = *matches.get_one::<bool>("shell").unwrap_or(&false);

        // Default to all options if none is specified
        if !worktree && !repo_path_format && !organizations && !shell {
            worktree = true;
            repo_path_format = true;
            organizations = true;
            shell = true;
        }

        let mut binding = ConfigBootstrapOptions::new();
        let options = binding
            .worktree(worktree)
            .repo_path_format(repo_path_format)
            .organizations(organizations)
            .shell(shell);

        Self {
            options: options.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBootstrapCommand {
    cli_args: OnceCell<ConfigBootstrapCommandArgs>,
}

impl ConfigBootstrapCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &ConfigBootstrapCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    pub fn name(&self) -> Vec<String> {
        vec!["config".to_string(), "bootstrap".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Bootstraps the configuration of omni\n",
                "\n",
                "This will walk you through setting up the initial configuration to ",
                "use omni, such as setting up the worktree, format to use when cloning ",
                "repositories, and setting up initial organizations.\n",
            )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "--worktree".to_string(),
                    desc: Some("Bootstrap the main worktree location".to_string()),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--repo-path-format".to_string(),
                    desc: Some("Bootstrap the repository path format".to_string()),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--organizations".to_string(),
                    desc: Some("Bootstrap the organizations".to_string()),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--shell".to_string(),
                    desc: Some("Bootstrap the shell integration".to_string()),
                    required: false,
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if self
            .cli_args
            .set(ConfigBootstrapCommandArgs::parse(argv))
            .is_err()
        {
            unreachable!();
        }

        match config_bootstrap(Some(self.cli_args().options.clone())) {
            Ok(true) => {
                omni_info!("configuration updated");
            }
            Ok(false) => {}
            Err(err) => {
                omni_error!(format!("{}", err));
                exit(1);
            }
        }

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        true
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) -> Result<(), ()> {
        println!("--organizations");
        println!("--repo-path-format");
        println!("--shell");
        println!("--worktree");

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBootstrapOptions {
    default: bool,
    worktree: bool,
    repo_path_format: bool,
    organizations: bool,
    shell: bool,
}

impl ConfigBootstrapOptions {
    fn new() -> Self {
        Self {
            default: true,
            worktree: true,
            repo_path_format: true,
            organizations: true,
            shell: true,
        }
    }

    fn worktree(&mut self, worktree: bool) -> &mut Self {
        self.default = false;
        self.worktree = worktree;
        self
    }

    fn repo_path_format(&mut self, repo_path_format: bool) -> &mut Self {
        self.default = false;
        self.repo_path_format = repo_path_format;
        self
    }

    fn organizations(&mut self, organizations: bool) -> &mut Self {
        self.default = false;
        self.organizations = organizations;
        self
    }

    fn shell(&mut self, shell: bool) -> &mut Self {
        self.default = false;
        self.shell = shell;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigBootstrap {
    #[serde(skip_serializing_if = "String::is_empty")]
    worktree: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    repo_path_format: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    org: Vec<OrgConfig>,
}

pub fn config_bootstrap(options: Option<ConfigBootstrapOptions>) -> Result<bool, String> {
    let options = options.unwrap_or(ConfigBootstrapOptions::new());

    if options.worktree || options.repo_path_format || options.organizations {
        let worktree = if options.worktree {
            let (worktree, continue_bootstrap) = question_worktree();
            if !continue_bootstrap {
                return Ok(false);
            }
            worktree
        } else {
            "".to_string()
        };

        let repo_path_format = if options.repo_path_format {
            let (repo_path_format, continue_bootstrap) =
                question_repo_path_format(worktree.clone());
            if !continue_bootstrap {
                return Ok(false);
            }
            repo_path_format
        } else {
            "".to_string()
        };

        let orgs = if options.organizations {
            let (orgs, continue_bootstrap) = question_org(&worktree);
            if !continue_bootstrap {
                return Ok(false);
            }
            orgs
        } else {
            vec![]
        };

        let config = ConfigBootstrap {
            worktree,
            repo_path_format,
            org: orgs,
        };

        if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
            // Dump our config object as yaml
            let yaml = serde_yaml::to_string(&config);

            // Now get a ConfigValue object from the yaml
            let new_config_value = match yaml {
                Ok(yaml) => ConfigValue::from_str(&yaml),
                Err(err) => {
                    omni_error!(format!("failed to serialize configuration: {}", err));
                    return false;
                }
            };

            // Apply it over the existing configuration
            config_value.extend(
                new_config_value,
                ConfigExtendOptions::new()
                    .with_strategy(ConfigExtendStrategy::Replace)
                    .with_transform(false),
                vec![],
            );

            // And return true to save the configuration
            true
        }) {
            return Err(format!("Failed to update user configuration: {}", err));
        }
    }

    if options.shell {
        if shell_integration_is_loaded() {
            if options.default {
                // If the shell integration is already setup, no need to do anything else
                return Ok(true);
            } else {
                omni_info!("shell integration detected in this shell");
                omni_info!(format!(
                    "still proceeding as requested through {}",
                    "--shell".light_cyan()
                ));
            }
        }

        // We reach here only if we're missing the shell integration
        let current_shell = Shell::current();
        match current_shell {
            Shell::Unknown(_) | Shell::Posix => {
                omni_warning!(format!(
                    "omni does not provide a shell integration for your shell ({})",
                    current_shell.to_str().light_cyan(),
                ));
                omni_warning!("you can still use omni, but dynamic environment and easy");
                omni_warning!("navigation will not be available");
                return Ok(true);
            }
            _ => {}
        }

        let (rc_file, continue_bootstrap) = question_rc_file(&current_shell);
        if !continue_bootstrap {
            return Ok(false);
        }

        match std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(rc_file.clone())
        {
            Ok(mut file) => {
                let hook = current_shell.hook_init_command();

                // Check if the hook is already in the file
                let mut line_number = 0;
                let reader = BufReader::new(&file);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            line_number += 1;
                            if line.trim() == hook {
                                omni_info!(format!(
                                    "omni hook already present in {}",
                                    rc_file.to_string_lossy().light_blue(),
                                ));
                                return Ok(true);
                            }
                        }
                        Err(err) => {
                            return Err(format!(
                                "Failed to read from {}: {}",
                                rc_file.to_string_lossy(),
                                err
                            ));
                        }
                    }
                }

                // Check if we need to add an extra new line
                let ends_with_newline = if line_number > 0 {
                    let mut buf = [0; 1];
                    file.seek(std::io::SeekFrom::End(-1)).unwrap();
                    file.read_exact(&mut buf).unwrap();
                    buf[0] == b'\n'
                } else {
                    false
                };

                // If we get here, we have to write the hook at the end of the file
                let mut content = String::new();
                if line_number > 0 {
                    content.push('\n');
                    if !ends_with_newline {
                        content.push('\n');
                    }
                }
                content.push_str("# omni shell integration\n");
                content.push_str(&hook);
                content.push('\n');

                if let Err(err) = file.write_all(content.as_bytes()) {
                    return Err(format!(
                        "Failed to write to {}: {}",
                        rc_file.to_string_lossy(),
                        err
                    ));
                }

                omni_info!(format!(
                    "omni hook added to {}; remember to reload your shell",
                    rc_file.to_string_lossy().light_blue(),
                ));
            }
            Err(err) => {
                return Err(format!(
                    "Failed to open {}: {}",
                    rc_file.to_string_lossy(),
                    err
                ));
            }
        }
    }

    Ok(true)
}

fn question_worktree() -> (String, bool) {
    let global_config = global_config();

    let default_worktree = PathBuf::from(global_config.worktree.clone());
    let default_worktree = if let Ok(suffix) = default_worktree.strip_prefix(user_home()) {
        PathBuf::from("~").join(suffix)
    } else {
        default_worktree
    }
    .to_string_lossy()
    .to_string();

    let question = requestty::Question::input("config_worktree")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("What is the directory where you usually put your projects?")
        .auto_complete(|p, _| file_auto_complete(p))
        .default(default_worktree)
        .validate(|path, _| {
            if path.is_empty() {
                return Err("You need to provide a value for the worktree".to_string());
            }

            let path_obj = PathBuf::from(path);
            let canonicalized = abs_path(path_obj);
            if canonicalized.exists() && !canonicalized.is_dir() {
                return Err("The worktree must be a directory".to_string());
            }
            Ok(())
        })
        .build();

    let worktree = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::String(path) => {
                let path_obj = PathBuf::from(path.clone());
                let canonicalized = abs_path(path_obj);
                if !canonicalized.is_dir() {
                    omni_warning!(
                        format!(
                            "directory {} does not exist, but will be created upon cloning",
                            path.clone().light_cyan(),
                        ),
                        ""
                    );
                }
                path
            }
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {:?}", err).red());
            return ("".to_string(), false);
        }
    };

    (worktree, true)
}

fn question_repo_path_format(worktree: String) -> (String, bool) {
    let global_config = global_config();
    let current_repo_path_format = global_config.repo_path_format.clone();

    let mut choices = vec![
        (
            "%{host}/%{org}/%{repo}",
            true,
            "github.com/xaf/omni".to_string(),
        ),
        ("%{org}/%{repo}", true, "xaf/omni".to_string()),
        ("%{repo}", true, "omni".to_string()),
    ];

    let mut default = 0;
    if !current_repo_path_format.is_empty() {
        let mut found = false;

        for (index, (format, _joinpath, _example)) in choices.iter_mut().enumerate() {
            if current_repo_path_format == *format {
                default = index;
                found = true;
                break;
            }
        }

        if !found {
            let git_url = full_git_url_parse("https://github.com/xaf/omni").unwrap();
            let example =
                format_path_with_template(&worktree, &git_url, current_repo_path_format.clone());
            let example_str = example.to_string_lossy().to_string();

            choices.insert(
                0,
                (
                    &current_repo_path_format,
                    false,
                    format!("e.g. {}", example_str),
                ),
            );
        }
    }

    let custom = choices.len();
    choices.push((
        "custom",
        false,
        "use the variables to write your own format".to_string(),
    ));

    let qchoices: Vec<_> = choices
        .iter()
        .map(|(format, joinpath, example)| {
            let example = if *joinpath {
                let path = PathBuf::from(&worktree).join(example);
                format!("e.g. {}", path.to_string_lossy())
            } else {
                example.to_string()
            };
            format!("{} {}", format, format!("({})", example).light_black())
        })
        .collect();

    let question = requestty::Question::select("config_repo_path_format")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("How do you structure your projects inside your worktree?")
        .choices(qchoices)
        .default(default)
        .transform(|selected, _, backend| {
            // Let's stop at the first parenthesis we encounter
            let selected = selected.text.split('(').next().unwrap_or(&selected.text);
            write!(backend, "{}", selected.cyan())
        })
        .build();

    let repo_path_format = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ListItem(item) => match item.index {
                idx if idx == custom => {
                    let question = requestty::Question::input("config_repo_path_format_custom")
                        .ask_if_answered(true)
                        .on_esc(requestty::OnEsc::Terminate)
                        .message("Which custom format do you wish to use?")
                        .default("%{host}/%{org}/%{repo}")
                        .validate(|format, _| {
                            if format.is_empty() {
                                return Err("You need to provide a format".light_red());
                            }

                            // Check that at least %{repo} exists
                            if !format.contains("%{repo}") {
                                return Err("The format must contain %{repo}"
                                    .to_string()
                                    .light_red());
                            }

                            // Check if any %{..} variable that is not repo, org or host
                            // exists, as other variables are not supported
                            let regex = Regex::new(r"%\{([^}]+)\}").unwrap();
                            for capture in regex.captures_iter(format) {
                                let var = capture.get(1).unwrap().as_str();
                                if var != "repo" && var != "org" && var != "host" {
                                    return Err(format!(
                                        "The format contains an unknown variable: %{{{}}}",
                                        var
                                    )
                                    .to_string()
                                    .light_red());
                                }
                            }

                            Ok(())
                        })
                        .build();

                    match requestty::prompt_one(question) {
                        Ok(answer) => match answer {
                            requestty::Answer::String(format) => format,
                            _ => unreachable!(),
                        },
                        Err(err) => {
                            println!("{}\x1B[0K", format!("[✘] {:?}", err).red());
                            return ("".to_string(), false);
                        }
                    }
                }
                _ => choices[item.index].0.to_string(),
            },
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {:?}", err).red());
            return ("".to_string(), false);
        }
    };

    (repo_path_format, true)
}

fn question_org(worktree: &str) -> (Vec<OrgConfig>, bool) {
    // Now that we have a worktree, we can list the repositories in there
    // and identify the organizations that the user has, so we can offer
    // them to be setup as trusted (or not) organizations.

    let mut worktrees = HashSet::new();
    worktrees.insert(PathBuf::from(worktree));

    let repositories = TidyGitRepo::list_repositories(worktrees);

    let mut orgs_map = HashMap::new();
    let mut hosts = HashSet::new();
    for repository in repositories {
        let origin_url = repository.origin_url;
        if let Ok(git_url) = full_git_url_parse(&origin_url) {
            let mut org = git_url.clone();

            // First we get the entry that's considering the host and
            // the org, but not the repo
            if org.git_suffix {
                org.path = org
                    .path
                    .strip_suffix(".git")
                    .unwrap_or(org.path.as_ref())
                    .to_string();
            }
            org.git_suffix = false;

            org.path = org
                .path
                .strip_suffix(format!("/{}", org.name).as_str())
                .unwrap_or(org.path.as_ref())
                .to_string();
            org.name = "".to_string();

            let org_str = org.to_string();
            let org_count = orgs_map.entry(org_str).or_insert(0);
            *org_count += 1;

            // Then we get the entry that's considering the host only
            org.path = "".to_string();
            let host_str = org.to_string();
            hosts.insert(host_str.clone());
            let host_count = orgs_map.entry(host_str.clone()).or_insert(0);
            *host_count += 1;

            // And now we strip the user and protocol if any, and add another host entry
            org.user = None;
            org.scheme_prefix = false;
            let stripped_host_str = org.to_string();
            if stripped_host_str != host_str {
                hosts.insert(stripped_host_str.clone());
                let stripped_host_count = orgs_map.entry(stripped_host_str).or_insert(0);
                *stripped_host_count += 1;
            }
        }
    }

    // Sort the map by value
    let mut orgs: Vec<_> = orgs_map
        .clone()
        .into_iter()
        .map(|(handle, count)| (if hosts.contains(&handle) { 1 } else { 2 }, count, handle))
        .sorted()
        .rev()
        .map(|(_, count, handle)| (count, handle))
        .collect();

    // If there are any organizations, already in the configuration,
    // prepend them to the list of organizations above
    let global_config = global_config();
    let current_orgs = global_config.org.clone();
    let mut selected_orgs = HashSet::new();
    for org in current_orgs.iter().rev() {
        if let Ok(org) = Org::new(org.clone()) {
            let count = *orgs_map.get(&org.config.handle).unwrap_or(&0);
            orgs.retain(|x| x.1 != org.config.handle);
            orgs.insert(0, (count, org.config.handle.clone()));
            selected_orgs.insert(org.config.handle.clone());

            if org.owner.is_none() {
                hosts.insert(org.config.handle.clone());
            }
        }
    }

    // If there are no organizations, we can just return early
    if orgs.is_empty() {
        return (vec![], true);
    }

    // Prepare the choices
    let orgs_choices: Vec<_> = orgs
        .iter()
        .map(|(count, org)| {
            (
                format!(
                    "{} {}",
                    org,
                    format!(
                        "({} repositor{})",
                        count,
                        if *count == 1 { "y" } else { "ies" },
                    )
                    .light_black(),
                ),
                selected_orgs.contains(org),
            )
        })
        .collect();

    // Now prepare a multi-select to offer the organizations to be added for easy
    // cloning and navigation
    let question = requestty::Question::multi_select("config_org")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("Which organizations should be added to your configuration?")
        .choices_with_default(orgs_choices)
        .transform(|selected, _, backend| {
            write!(
                backend,
                "{} organization{}",
                selected.len(),
                if selected.len() == 1 { "" } else { "s" }
            )
        })
        .should_loop(false)
        .page_size(7)
        .build();

    let selected_orgs: Vec<String> = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ListItems(items) => items
                .iter()
                .map(|item| orgs[item.index].1.clone())
                .collect(),
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {:?}", err).red());
            return (vec![], false);
        }
    };

    // If there are no selected organizations, we can just return early
    if selected_orgs.is_empty() {
        return (vec![], true);
    }

    // Now do a multi-select to know which organizations should be trusted
    let question = requestty::Question::multi_select("config_org_trusted")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("Which organizations should be trusted?")
        .choices_with_default(
            selected_orgs
                .iter()
                .map(|org| {
                    (
                        format!(
                            "{}{}",
                            org,
                            if hosts.contains(org) {
                                // Unicode warning sign
                                " \u{26A0}\u{FE0F}  (broad trust)".light_black()
                            } else {
                                "".to_string()
                            }
                        ),
                        global_config
                            .org
                            .iter()
                            .any(|x| x.handle == *org && x.trusted),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .transform(|selected, _, backend| {
            write!(
                backend,
                "{} organization{}",
                selected.len(),
                if selected.len() == 1 { "" } else { "s" }
            )
        })
        .should_loop(false)
        .page_size(7)
        .build();

    let trusted_orgs: Vec<String> = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ListItems(items) => items
                .iter()
                .map(|item| selected_orgs[item.index].clone())
                .collect(),
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {:?}", err).red());
            return (vec![], false);
        }
    };

    // Let the user order the organizations in the order they want, as the
    // order of the organizations is important when cloning repositories,
    // the first organization that has the repository will be used.
    let question = requestty::Question::order_select("select_how_to_clone")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("In which order should the organizations be checked for repositories?")
        .choices(selected_orgs.clone())
        .transform(|_selected, _, backend| write!(backend, "\u{2714}\u{FE0F}"))
        .build();

    let ordered_orgs: Vec<String> = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ListItems(items) => items
                .iter()
                .map(|item| selected_orgs[item.index].clone())
                .collect(),
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {:?}", err).red());
            return (vec![], false);
        }
    };

    let current_orgs_worktrees: HashMap<String, String> = current_orgs
        .iter()
        .filter(|org| org.worktree.is_some())
        .map(|org| (org.handle.clone(), org.worktree.clone().unwrap()))
        .collect();
    let orgs_config: Vec<OrgConfig> = ordered_orgs
        .iter()
        .map(|org| {
            let trusted = trusted_orgs.contains(org);
            let worktree = current_orgs_worktrees.get(org);
            OrgConfig {
                handle: org.clone(),
                trusted,
                worktree: worktree.cloned(),
            }
        })
        .collect();

    (orgs_config, true)
}

fn question_rc_file(current_shell: &Shell) -> (PathBuf, bool) {
    let default_rc_file = current_shell.default_rc_file();
    let default_rc_file = if let Ok(suffix) = default_rc_file.strip_prefix(user_home()) {
        PathBuf::from("~").join(suffix)
    } else {
        default_rc_file
    }
    .to_string_lossy()
    .to_string();

    omni_info!("omni requires a shell integration to provide some of its features");

    let question = requestty::Question::input("integration_rc_file")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message(format!(
            "Where is the RC file of your shell ({}) to load the integration?",
            current_shell.to_str(),
        ))
        .auto_complete(|p, _| file_auto_complete(p))
        .default(default_rc_file)
        .validate(|path, _| {
            if path.is_empty() {
                return Err("You need to provide a value for the rc_file"
                    .to_string()
                    .light_red());
            }

            let path_obj = PathBuf::from(path);
            let canonicalized = abs_path(path_obj);

            if canonicalized.exists() {
                // Check if the path is a file
                if !canonicalized.is_file() {
                    return Err("The provided path must be a file".light_red());
                }

                // Check if the file is writeable
                match canonicalized.metadata() {
                    Ok(metadata) => {
                        if metadata.permissions().readonly() {
                            return Err("The file must be writeable".light_red());
                        }
                    }
                    Err(err) => return Err(err.light_red()),
                }

                return Ok(());
            }

            // Make sure the directory in which the file is exists, or
            // create it if it doesn't
            if let Some(parent) = canonicalized.parent() {
                if !parent.exists() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        return Err(format!(
                            "Failed to create directory {}: {}",
                            parent.to_string_lossy(),
                            err
                        )
                        .light_red());
                    }
                }
            }

            // Create the file if it doesn't exist
            if !canonicalized.exists() {
                if let Err(err) = std::fs::File::create(&canonicalized) {
                    return Err(format!(
                        "Failed to create file {}: {}",
                        canonicalized.to_string_lossy(),
                        err
                    )
                    .light_red());
                }
            }

            Ok(())
        })
        .build();

    let rc_file = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::String(path) => {
                let path_obj = PathBuf::from(path.clone());

                // No need for extra validation, as we have done it above
                abs_path(path_obj)
            }
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {:?}", err).red());
            return (PathBuf::new(), false);
        }
    };

    (rc_file, true)
}
