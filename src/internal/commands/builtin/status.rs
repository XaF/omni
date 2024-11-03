use std::process::exit;

use once_cell::sync::OnceCell;
use regex::Regex;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::path::omnipath_entries;
use crate::internal::config::config;
use crate::internal::config::config_loader;
use crate::internal::config::utils::sort_serde_yaml;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::shell_integration_is_loaded;
use crate::internal::git::ORG_LOADER;
use crate::internal::user_interface::StringColor;
use crate::omni_error;
use crate::omni_header;

#[derive(Debug, Clone)]
struct StatusCommandArgs {
    single: bool,
    shell_integration: bool,
    config: bool,
    config_files: bool,
    worktree: bool,
    orgs: bool,
    path: bool,
}

impl StatusCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let args = [
            "shell-integration",
            "config",
            "config-files",
            "worktree",
            "orgs",
            "path",
        ];

        let command = args.iter().fold(
            clap::Command::new("")
                .disable_help_subcommand(true)
                .disable_version_flag(true),
            |command, &arg| {
                command.arg(
                    clap::Arg::new(arg)
                        .long(arg)
                        .action(clap::ArgAction::SetTrue),
                )
            },
        );
        let matches = command.try_get_matches_from(&parse_argv);

        let matches = match matches {
            Ok(matches) => matches,
            Err(err) => {
                match err.kind() {
                    clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                        HelpCommand::new().exec(vec!["status".to_string()]);
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
        };

        let options = args
            .iter()
            .map(|&key| {
                (
                    key.to_string(),
                    *matches.get_one::<bool>(key).unwrap_or(&false),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();

        let selected = options.values().filter(|&&selected| selected).count();

        if selected == 0 {
            return Self {
                single: false,
                shell_integration: true,
                config: false,
                config_files: true,
                worktree: true,
                orgs: true,
                path: true,
            };
        }

        Self {
            single: selected == 1,
            shell_integration: *options.get("shell-integration").unwrap(),
            config: *options.get("config").unwrap(),
            config_files: *options.get("config-files").unwrap(),
            worktree: *options.get("worktree").unwrap(),
            orgs: *options.get("orgs").unwrap(),
            path: *options.get("path").unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusCommand {
    cli_args: OnceCell<StatusCommandArgs>,
}

impl StatusCommand {
    pub fn new() -> Self {
        Self {
            cli_args: OnceCell::new(),
        }
    }

    fn cli_args(&self) -> &StatusCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    fn print_shell_integration(&self) {
        if !self.cli_args().shell_integration {
            return;
        }

        let prefix = if self.cli_args().single {
            "".to_string()
        } else {
            println!("\n{}", "Shell integration".bold());
            "  ".to_string()
        };

        let status = if shell_integration_is_loaded() {
            "loaded".light_green()
        } else {
            "not loaded".light_red()
        };
        println!("{}{}", prefix, status);
    }

    fn print_configuration(&self) {
        if !self.cli_args().config {
            return;
        }

        if !self.cli_args().single {
            println!("\n{}", "Configuration".bold());
        }

        let config = config(".");
        match serde_yaml::to_value(&config) {
            Ok(value) => {
                let sorted_value = sort_serde_yaml(&value);
                let yaml_code = serde_yaml::to_string(&sorted_value).unwrap();
                println!("{}", self.color_yaml(&yaml_code));
            }
            Err(err) => {
                omni_error!(format!("failed to serialize configuration: {}", err));
                exit(1);
            }
        }
    }

    fn print_configuration_files(&self) {
        if !self.cli_args().config_files {
            return;
        }

        let prefix = if self.cli_args().single {
            "".to_string()
        } else {
            println!("\n{}", "Loaded configuration files".bold());
            "  ".to_string()
        };

        let config_loader = config_loader(".");

        if config_loader.loaded_config_files.is_empty() {
            println!("{}{}", prefix, "none".light_red());
        } else {
            for config_file in &config_loader.loaded_config_files {
                println!("{}- {}", prefix, config_file);
            }
        }
    }

    fn print_worktree(&self) {
        if !self.cli_args().worktree {
            return;
        }

        let prefix = if self.cli_args().single {
            "".to_string()
        } else {
            println!("\n{}", "Worktree".bold());
            "  ".to_string()
        };

        let config = config(".");
        println!("{}{}", prefix, config.worktree());
    }

    fn print_orgs(&self) {
        if !self.cli_args().orgs {
            return;
        }

        let prefix = if self.cli_args().single {
            "".to_string()
        } else {
            println!("\n{}", "Git Orgs".bold());
            "  ".to_string()
        };

        if ORG_LOADER.is_empty() {
            println!("{}{}", prefix, "none".light_red());
        } else {
            for org in ORG_LOADER.printable_orgs() {
                let mut org_str = org.config.handle.to_string();
                if let Some(worktree) = &org.config.worktree {
                    org_str.push_str(&format!(" ({})", worktree).light_black());
                }

                if org.config.trusted {
                    org_str.push_str(&format!(" {}", "trusted").light_green().italic());
                } else {
                    org_str.push_str(&format!(" {}", "untrusted").light_red().italic());
                }

                println!("{}- {}", prefix, org_str);
            }
        }
    }

    fn print_path(&self) {
        if !self.cli_args().path {
            return;
        }

        let prefix = if self.cli_args().single {
            "".to_string()
        } else {
            println!("\n{}", "Current omnipath".bold());
            "  ".to_string()
        };

        let omnipath = omnipath_entries();
        if omnipath.is_empty() {
            println!("{}{}", prefix, "none".light_red());
        } else {
            for path in &omnipath {
                if let Some(package) = &path.package {
                    let mut pkg_string = format!("{} {}", "package:".light_cyan(), package);
                    if !path.path.is_empty() {
                        pkg_string.push_str(&format!(", {} {}", "path:".light_cyan(), path.path));
                    }
                    println!(
                        "{}- {}\n{}  {}",
                        prefix,
                        pkg_string,
                        prefix,
                        format!("({})", path.full_path).light_black()
                    );
                } else {
                    println!("{}- {}", prefix, path.path);
                }
            }
        }
    }

    fn color_yaml(&self, yaml_code: &str) -> String {
        let yaml_lines = &mut yaml_code.lines().collect::<Vec<&str>>();
        if yaml_lines[0] == "---" {
            // Remove the first line if it's "---"; as it's not very useful
            yaml_lines.remove(0);
        }

        let pattern = r#"^(\s*)(\-\s*)?(("[^"]+"|([^:]|:[^ ])+)\s*:)(\s+|$)"#;
        let regex_keys = Regex::new(pattern).unwrap();
        // Replace the keys by themselves, colored
        let yaml_lines = yaml_lines
            .iter()
            .map(|line| {
                if let Some(captures) = regex_keys.captures(line) {
                    let key = captures.get(3).unwrap().as_str();
                    let colored_key = key.light_cyan();
                    line.replace(key, &colored_key)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<String>>();

        if self.cli_args().single {
            return yaml_lines.join("\n");
        }

        let yaml_code = yaml_lines.join("\n  │ ");
        format!("  │ {}", yaml_code)
    }
}

impl BuiltinCommand for StatusCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["status".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Show the status of omni\n",
                "\n",
                "This will show the configuration that omni is loading when called ",
                "from the current directory."
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["--shell-integration".to_string()],
                    desc: Some("Show if the shell integration is loaded or not.".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--config".to_string()],
                    desc: Some(
                        "Show the configuration that omni is using for the current directory. This is not shown by default."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--config-files".to_string()],
                    desc: Some(
                        "Show the configuration files that omni is loading for the current directory."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--worktree".to_string()],
                    desc: Some(
                        "Show the default worktree."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--orgs".to_string()],
                    desc: Some(
                        "Show the organizations."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--path".to_string()],
                    desc: Some(
                        "Show the current omnipath."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        if self.cli_args.set(StatusCommandArgs::parse(argv)).is_err() {
            unreachable!();
        }

        if !self.cli_args().single {
            println!("{}", omni_header!());
        }

        self.print_shell_integration();
        self.print_configuration();
        self.print_configuration_files();
        self.print_worktree();
        self.print_orgs();
        self.print_path();

        exit(0);
    }

    fn autocompletion(&self) -> bool {
        true
    }

    fn autocomplete(&self, _comp_cword: usize, argv: Vec<String>) -> Result<(), ()> {
        for arg in &[
            "--shell-integration",
            "--config",
            "--config-files",
            "--worktree",
            "--orgs",
            "--path",
        ] {
            if !argv.contains(&arg.to_string()) {
                println!("{}", arg);
            }
        }

        Ok(())
    }
}
