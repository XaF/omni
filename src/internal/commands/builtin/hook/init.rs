use std::env;
use std::path::PathBuf;
use std::process::exit;

use lazy_static::lazy_static;
use serde::Serialize;
use shell_escape::escape;
use tera::Context;
use tera::Tera;

use crate::internal::commands::HelpCommand;
use crate::internal::config::global_config;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::user_interface::StringColor;
use crate::omni_error;

lazy_static! {
    static ref CURRENT_EXE: PathBuf = {
        let current_exe = std::env::current_exe();
        if current_exe.is_err() {
            omni_error!("failed to get current executable path", "hook init");
            exit(1);
        }
        current_exe.unwrap()
    };
}

#[derive(Debug, Clone)]
struct HookInitCommandArgs {
    shell: String,
    aliases: Vec<String>,
    command_aliases: Vec<InitHookAlias>,
}

impl HookInitCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .arg(clap::Arg::new("shell").action(clap::ArgAction::Set))
            .arg(
                clap::Arg::new("aliases")
                    .short('a')
                    .long("alias")
                    .action(clap::ArgAction::Append),
            )
            .arg(
                clap::Arg::new("command_aliases")
                    .short('c')
                    .long("command-alias")
                    .number_of_values(2)
                    .action(clap::ArgAction::Append),
            )
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
            match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec(vec!["hook".to_string()]);
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

        let shell = if let Some(shell) = matches.get_one::<String>("shell") {
            shell.to_string()
        } else {
            let mut shell = env::var("SHELL").unwrap_or("bash".to_string());
            if shell.contains('/') {
                shell = shell.split('/').last().unwrap().to_string();
            }
            shell
        };

        // Load aliases from the configuration first
        let config = global_config();
        let mut aliases: Vec<String> = vec![];
        let mut command_aliases: Vec<InitHookAlias> = vec![];
        for alias in config.shell_aliases.aliases.iter() {
            match alias.target.as_ref() {
                Some(target) => {
                    command_aliases.push(InitHookAlias::new(alias.alias.clone(), target.clone()));
                }
                None => aliases.push(alias.alias.clone()),
            }
        }

        // Then add the ones from the command line
        aliases.extend(
            if let Some(aliases) = matches.get_many::<String>("aliases").clone() {
                aliases
                    .into_iter()
                    .map(|arg| arg.to_string())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            },
        );

        command_aliases.extend(
            if let Some(command_aliases) = matches.get_many::<String>("command_aliases").clone() {
                command_aliases
                    .into_iter()
                    .map(|arg| arg.to_string())
                    .collect::<Vec<_>>()
                    .chunks(2)
                    .map(|chunk| InitHookAlias::new(chunk[0].clone(), chunk[1].clone()))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            },
        );

        Self {
            shell,
            aliases,
            command_aliases,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct InitHookAlias {
    alias: String,
    command: String,
    command_size: usize,
    full_command: String,
}

impl InitHookAlias {
    fn new(alias: String, command: String) -> Self {
        // Use shell split for command
        let command_vec = shell_words::split(&command)
            .unwrap_or_else(|err| {
                omni_error!(
                    format!("failed to parse alias command '{}': {}", command, err),
                    "hook init"
                );
                exit(1);
            })
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let full_command = format!("omni {}", command);

        Self {
            alias,
            command: shell_words::quote(&command).to_string(),
            command_size: command_vec.len(),
            full_command: shell_words::quote(&full_command).to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HookInitCommand {}

impl HookInitCommand {
    pub fn new() -> Self {
        Self {}
    }

    pub fn name(&self) -> Vec<String> {
        vec!["hook".to_string(), "init".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
        Some(
            concat!(
            "Hook used to initialize the shell\n",
            "\n",
            "The \x1B[1m\x1B[4minit\x1B[0m hook will provide you with the command to run to ",
            "initialize omni in your shell. You can specify which shell you wish to load it ",
            "for by specifying either one of \x1B[1mzsh\x1B[0m, \x1B[1mbash\x1B[0m, or ",
            "\x1B[1mfish\x1B[0m as optional parameter. If no argument is specified, the login ",
            "shell, as provided by the \x1B[3mSHELL\x1B[0m environment variable, will be used. ",
            "You can load omni in your shell by using \x1B[1meval \"$(omni hook init YOURSHELL)",
            "\"\x1B[0m for bash or zsh, or \x1B[1momni hook init fish | source\x1B[0m for fish.\n",
            "\n",
            "The \x1B[1minit\x1B[0m hook supports the \x1B[1m--alias <alias>\x1B[0m ",
            "option, which adds an alias to the omni command with autocompletion support. It ",
            "also supports the \x1B[1m--command-alias <alias> <subcommand>\x1B[0m option, which ",
            "adds an alias to the specified omni subcommand with autocompletion support.",
        )
            .to_string(),
        )
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![
                SyntaxOptArg {
                    name: "--alias <alias>".to_string(),
                    desc: Some(
                        "Create an alias for the omni command with autocompletion support."
                            .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "--command-alias <alias> <subcommand>".to_string(),
                    desc: Some(
                        concat!(
                            "Create an alias for the specified omni subcommand with autocompletion ",
                            "support. The <subcommand> argument can be any omni subcommand, including ",
                            "custom subcommands.",
                        )
                        .to_string(),
                    ),
                    required: false,
                },
                SyntaxOptArg {
                    name: "shell".to_string(),
                    desc: Some(
                        "Which shell to initialize omni for. Can be one of bash, zsh or fish."
                            .to_string(),
                    ),
                    required: false,
                },
            ],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        let args = HookInitCommandArgs::parse(argv);

        match args.shell.as_str() {
            "bash" => dump_integration(
                args,
                include_bytes!("../../../../../shell_integration/omni.bash.tmpl"),
            ),
            "zsh" => dump_integration(
                args,
                include_bytes!("../../../../../shell_integration/omni.zsh.tmpl"),
            ),
            "fish" => dump_integration(
                args,
                include_bytes!("../../../../../shell_integration/omni.fish.tmpl"),
            ),
            _ => {
                omni_error!(
                    format!(
                        "invalid shell '{}', omni only supports bash, zsh and fish",
                        args.shell
                    ),
                    "hook init"
                );
                exit(1);
            }
        }
        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {
        exit(0);
    }
}

fn dump_integration(args: HookInitCommandArgs, integration: &[u8]) {
    let integration = String::from_utf8_lossy(integration).to_string();

    let mut context = Context::new();
    context.insert(
        "OMNI_BIN",
        &escape(std::borrow::Cow::Borrowed(CURRENT_EXE.to_str().unwrap())),
    );
    context.insert("OMNI_ALIASES", &args.aliases);
    context.insert("OMNI_COMMAND_ALIASES", &args.command_aliases);

    let result = Tera::one_off(&integration, &context, false)
        .expect("failed to render integration template");

    println!("{}", result);
}
