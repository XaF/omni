use std::env;
use std::path::PathBuf;
use std::process::exit;

use lazy_static::lazy_static;
use serde::Serialize;
use shell_escape::escape;
use tera::Context;
use tera::Tera;

use crate::internal::commands::HelpCommand;
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
struct InitHookArgs {
    shell: String,
    aliases: Vec<String>,
    command_aliases: Vec<InitHookAlias>,
}

impl InitHookArgs {
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
            if shell.contains("/") {
                shell = shell.split("/").last().unwrap().to_string();
            }
            shell
        };

        let aliases: Vec<String> =
            if let Some(aliases) = matches.get_many::<String>("aliases").clone() {
                aliases
                    .into_iter()
                    .map(|arg| arg.to_string())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

        let command_aliases: Vec<InitHookAlias> =
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
            };

        Self {
            shell: shell,
            aliases: aliases,
            command_aliases: command_aliases,
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
            alias: alias,
            command: shell_words::quote(&command).to_string(),
            command_size: command_vec.len(),
            full_command: shell_words::quote(&full_command).to_string(),
        }
    }
}

pub fn init_hook(argv: Vec<String>) {
    let args = InitHookArgs::parse(argv);

    match args.shell.as_str() {
        "bash" => dump_integration(
            args,
            include_bytes!("../../../shell_integration/omni.bash.tmpl"),
        ),
        "zsh" => dump_integration(
            args,
            include_bytes!("../../../shell_integration/omni.zsh.tmpl"),
        ),
        "fish" => dump_integration(
            args,
            include_bytes!("../../../shell_integration/omni.fish.tmpl"),
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
}

fn dump_integration(args: InitHookArgs, integration: &[u8]) {
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
