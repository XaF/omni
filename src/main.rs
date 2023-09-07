use std::env;
use std::process::exit;

use mimalloc::MiMalloc;

mod internal;
use internal::command_loader;
use internal::dynenv::update_dynamic_env;
use internal::dynenv::DynamicEnvExportMode;
use internal::env::determine_shell;
use internal::git::auto_path_update;
use internal::hooks::init_hook;
use internal::hooks::uuid_hook;
use internal::StringColor;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

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
                    let shell_type = if argv.len() > 2 {
                        argv[2].clone()
                    } else {
                        determine_shell()
                    };
                    let export_mode = match shell_type.as_ref() {
                        "posix" | "bash" | "zsh" => DynamicEnvExportMode::Posix,
                        "fish" => DynamicEnvExportMode::Fish,
                        _ => {
                            eprintln!(
                                "{} {} {}",
                                "omni:".to_string().light_cyan(),
                                "invalid export mode:".to_string().red(),
                                argv[2]
                            );
                            exit(1);
                        }
                    };
                    update_dynamic_env(export_mode.clone());
                    exit(0);
                }
                "uuid" => {
                    uuid_hook();
                    exit(0);
                }
                "init" => {
                    let shell = if argv.len() > 2 {
                        argv[2].clone()
                    } else {
                        let mut shell = env::var("SHELL").unwrap_or("bash".to_string());
                        if shell.contains("/") {
                            shell = shell.split("/").last().unwrap().to_string();
                        }
                        shell
                    };
                    init_hook(&shell);
                    exit(0);
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
