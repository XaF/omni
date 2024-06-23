use std::env;
use std::process::exit;

mod internal;
use internal::command_loader;
use internal::commands::base::BuiltinCommand;
use internal::commands::loader::set_lookup_local_first;
use internal::commands::HookEnvCommand;
use internal::commands::HookInitCommand;
use internal::commands::HookUuidCommand;
use internal::config::ensure_bootstrap;
use internal::config::up::utils::handle_shims;
use internal::config::up::utils::AskPassRequest;
use internal::env::tmpdir_cleanup;
use internal::git::auto_update_async;
use internal::git::auto_update_on_command_not_found;
use internal::git::exec_update;
use internal::git::exec_update_and_log_on_error;
use internal::StringColor;

#[derive(Debug, Clone)]
struct MainArgs {
    only_check_exists: bool,
    lookup_local_commands_first: bool,
    args: Vec<String>,
}

impl MainArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_help_flag(true)
            .version(env!("CARGO_PKG_VERSION"))
            .arg(
                clap::Arg::new("help")
                    .long("help")
                    .short('h')
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("update")
                    .long("update")
                    .conflicts_with("args")
                    .conflicts_with("exists")
                    .conflicts_with("help")
                    .conflicts_with("update-and-log-on-error")
                    .conflicts_with("version")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("update-and-log-on-error")
                    .long("update-and-log-on-error")
                    .conflicts_with("args")
                    .conflicts_with("exists")
                    .conflicts_with("help")
                    .conflicts_with("update")
                    .conflicts_with("version")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("exists")
                    .long("exists")
                    .short('e')
                    .conflicts_with("help")
                    .conflicts_with("update")
                    .conflicts_with("update-and-log-on-error")
                    .conflicts_with("version")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("askpass")
                    .long("askpass")
                    .short('A')
                    .num_args(2)
                    .value_names(["prompt", "socket path"])
                    .conflicts_with("args")
                    .conflicts_with("exists")
                    .conflicts_with("help")
                    .conflicts_with("update")
                    .conflicts_with("update-and-log-on-error")
                    .conflicts_with("version"),
            )
            .arg(
                clap::Arg::new("local")
                    .long("local")
                    .short('l')
                    .conflicts_with("askpass")
                    .conflicts_with("exists")
                    .conflicts_with("help")
                    .conflicts_with("update")
                    .conflicts_with("update-and-log-on-error")
                    .conflicts_with("version")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("args")
                    .action(clap::ArgAction::Append)
                    .allow_hyphen_values(true),
            )
            .try_get_matches_from(&parse_argv);

        let matches = match matches {
            Ok(matches) => matches,
            Err(err) => match err.kind() {
                clap::error::ErrorKind::DisplayVersion => {
                    println!("omni version {}", env!("CARGO_PKG_VERSION"));
                    exit(0);
                }
                // clap::error::ErrorKind::DisplayHelp
                // | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                // unreachable!("help flag is disabled");
                // }
                _ => {
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
            },
        };

        if let Some(askpass) = matches.get_many::<String>("askpass") {
            let askpass = askpass.collect::<Vec<_>>();
            if askpass.len() != 2 {
                exit(1);
            }

            let prompt = askpass[0].as_str();
            let request = AskPassRequest::new(prompt);

            let socket_path = askpass[1].as_str();
            match request.send(socket_path) {
                Ok(password) => {
                    print!("{}", password);
                    exit(0);
                }
                Err(_err) => exit(1),
            }
        }

        if *matches.get_one::<bool>("update").unwrap_or(&false) {
            exec_update();
        } else if *matches
            .get_one::<bool>("update-and-log-on-error")
            .unwrap_or(&false)
        {
            exec_update_and_log_on_error();
        }

        let mut args: Vec<_> = matches
            .get_many::<String>("args")
            .map(|args| args.map(|arg| arg.to_string()).collect())
            .unwrap_or_default();

        if *matches.get_one::<bool>("help").unwrap_or(&false) {
            args.insert(0, "help".to_string());
        }

        if args.is_empty() {
            args.push("help".to_string());
        }

        Self {
            only_check_exists: *matches.get_one::<bool>("exists").unwrap_or(&false),
            lookup_local_commands_first: *matches.get_one::<bool>("local").unwrap_or(&false),
            args,
        }
    }
}

fn complete_omni_subcommand(argv: &[String]) {
    let comp_cword = env::var("COMP_CWORD")
        .map(|s| s.parse().unwrap_or(0) - 1)
        .unwrap_or(0);

    match command_loader(".").complete(comp_cword, argv.to_vec(), true) {
        Ok(_) => exit(0),
        Err(_) => exit(1),
    }
}

fn run_omni_subcommand(parsed: &MainArgs) {
    if parsed.args[0] == "hook" {
        // For hooks, we want a fast path that doesn't load all the commands;
        // we want to make sure that we don't add any extraneous delay to the
        // shell. We also don't check arguments more than we need to, so that
        // things can be handled faster.
        if parsed.args.len() > 1 {
            match parsed.args[1].as_ref() {
                "env" => {
                    let command = HookEnvCommand::new();
                    command.exec(parsed.args[2..].to_vec());
                    panic!("exec returned");
                }
                "uuid" => {
                    let command = HookUuidCommand::new();
                    command.exec(parsed.args[2..].to_vec());
                    panic!("exec returned");
                }
                "init" => {
                    let command = HookInitCommand::new();
                    command.exec(parsed.args[2..].to_vec());
                    panic!("exec returned");
                }
                _ => {}
            }
        }

        // If we didn't match any hooks, let's just exit on error
        eprintln!(
            "{} {} {}",
            "omni:".light_cyan(),
            "command not found:".red(),
            parsed.args.join(" ")
        );
        exit(1);
    }

    if parsed.lookup_local_commands_first {
        set_lookup_local_first();

        // Set an environment variable to let subcommands know that
        // we're in local mode
        env::set_var("OMNI_LOCAL_LOOKUP", "1");
    }

    if !parsed.only_check_exists {
        // Ensures that omni has been bootstrapped
        ensure_bootstrap();
    }

    let command_loader = command_loader(".");
    if let Some((omni_cmd, called_as, argv)) = command_loader.to_serve(&parsed.args) {
        if parsed.only_check_exists {
            exit(match argv.len() {
                0 => 0,
                _ => 1,
            });
        }

        auto_update_async(if omni_cmd.has_source() {
            Some(omni_cmd.source().into())
        } else {
            None
        });

        set_cleanup_handler();
        omni_cmd.exec(argv, Some(called_as));
        panic!("exec returned");
    }

    if parsed.only_check_exists {
        exit(1);
    }

    // We didn't find the command, so let's try to update synchronously
    if auto_update_on_command_not_found() {
        // If any updates were done, let's check again if we can serve the command
        if let Some((omni_cmd, called_as, argv)) = command_loader.to_serve(&parsed.args) {
            set_cleanup_handler();
            omni_cmd.exec(argv, Some(called_as));
            panic!("exec returned");
        }
    }

    eprintln!(
        "{} {} {}",
        "omni:".light_cyan(),
        "command not found:".red(),
        parsed.args.join(" ")
    );

    if let Some((omni_cmd, called_as, argv)) = command_loader.find_command(&parsed.args) {
        set_cleanup_handler();
        omni_cmd.exec(argv, Some(called_as));
        panic!("exec returned");
    }

    exit(1);
}

fn set_cleanup_handler() {
    ctrlc::set_handler(move || {
        tmpdir_cleanup();

        // Exit the process with a non-zero status code
        exit(130);
    })
    .expect("Error setting Ctrl-C handler");
}

fn main() {
    handle_shims();

    let args: Vec<String> = env::args().skip(1).collect();

    if !args.is_empty() && args[0] == "--complete" {
        complete_omni_subcommand(&args[1..]);
    }

    let main_args = MainArgs::parse(args.clone());
    run_omni_subcommand(&main_args);
}
