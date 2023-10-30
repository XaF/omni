use std::env;
use std::process::exit;

mod internal;
use internal::command_loader;
use internal::commands::HookEnvCommand;
use internal::commands::HookInitCommand;
use internal::commands::HookUuidCommand;
use internal::git::auto_path_update;
use internal::StringColor;

fn complete_omni_subcommand(argv: &[String]) {
    let comp_cword = env::var("COMP_CWORD")
        .map(|s| s.parse().unwrap_or(0) - 1)
        .unwrap_or(0);

    let command_loader = command_loader(".");
    command_loader.complete(comp_cword, argv.to_vec(), true);
    exit(0);
}

fn run_omni_subcommand(argv: &[String]) {
    let mut argv = if argv.is_empty() {
        vec!["help".to_owned()]
    } else {
        argv.to_vec()
    };

    if argv[0] == "hook" {
        // For hooks, we want a fast path that doesn't load all the commands;
        // we want to make sure that we don't add any extraneous delay to the
        // shell. We also don't check arguments more than we need to, so that
        // things can be handled faster.
        if argv.len() > 1 {
            match argv[1].as_ref() {
                "env" => {
                    let command = HookEnvCommand::new();
                    command.exec(argv[2..].to_vec());
                    panic!("exec returned");
                }
                "uuid" => {
                    let command = HookUuidCommand::new();
                    command.exec(argv[2..].to_vec());
                    panic!("exec returned");
                }
                "init" => {
                    let command = HookInitCommand::new();
                    command.exec(argv[2..].to_vec());
                    panic!("exec returned");
                }
                _ => {}
            }
        }

        // If we didn't match any hooks, let's just exit on error
        eprintln!(
            "{} {} {}",
            "omni:".to_string().light_cyan(),
            "command not found:".to_string().red(),
            argv.join(" ")
        );
        exit(1);
    } else if argv.len() == 1 && (argv[0] == "--version" || argv[0] == "-v") {
        println!("omni version {}", env!("CARGO_PKG_VERSION"));
        exit(0);
    } else if argv[0] == "--help" || argv[0] == "-h" {
        argv[0] = "help".to_owned();
    }

    // Update the path if necessary
    auto_path_update();

    let command_loader = command_loader(".");
    if let Some((omni_cmd, called_as, argv)) = command_loader.to_serve(&argv) {
        omni_cmd.exec(argv, Some(called_as));
        panic!("exec returned");
    }

    eprintln!(
        "{} {} {}",
        "omni:".to_string().light_cyan(),
        "command not found:".to_string().red(),
        argv.join(" ")
    );

    if let Some((omni_cmd, called_as, argv)) = command_loader.find_command(&argv) {
        omni_cmd.exec(argv, Some(called_as));
        panic!("exec returned");
    }

    exit(1);
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if !args.is_empty() && args[0] == "--complete" {
        complete_omni_subcommand(&args[1..]);
    }

    run_omni_subcommand(&args);
}
