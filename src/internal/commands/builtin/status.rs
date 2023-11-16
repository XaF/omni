use std::process::exit;

use once_cell::sync::OnceCell;
use regex::Regex;

use crate::internal::commands::builtin::HelpCommand;
use crate::internal::commands::path::omnipath_entries;
use crate::internal::config::config;
use crate::internal::config::config_loader;
use crate::internal::config::CommandSyntax;
use crate::internal::git::ORG_LOADER;
use crate::internal::user_interface::StringColor;
use crate::internal::ENV;
use crate::omni_error;
use crate::omni_header;

#[derive(Debug, Clone)]
struct StatusCommandArgs {}

impl StatusCommandArgs {
    fn parse(argv: Vec<String>) -> Self {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let matches = clap::Command::new("")
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .try_get_matches_from(&parse_argv);

        if let Err(err) = matches {
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

        let matches = matches.unwrap();

        if *matches.get_one::<bool>("help").unwrap_or(&false) {
            HelpCommand::new().exec(vec!["status".to_string()]);
            exit(1);
        }

        Self {}
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

    #[allow(dead_code)]
    fn cli_args(&self) -> &StatusCommandArgs {
        self.cli_args.get_or_init(|| {
            omni_error!("command arguments not initialized");
            exit(1);
        })
    }

    pub fn name(&self) -> Vec<String> {
        vec!["status".to_string()]
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    pub fn help(&self) -> Option<String> {
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

    pub fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            usage: None,
            parameters: vec![],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if self.cli_args.set(StatusCommandArgs::parse(argv)).is_err() {
            unreachable!();
        }

        println!("{}", omni_header!());

        self.print_shell_integration();
        self.print_configuration();
        self.print_worktree();
        self.print_orgs();
        self.print_path();

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {}

    fn print_shell_integration(&self) {
        println!("\n{}", "Shell integration".to_string().bold());
        let status = if ENV.omni_cmd_file.is_some() {
            "loaded".to_string().light_green()
        } else {
            "not loaded".to_string().light_red()
        };
        println!("  {}", status);
    }

    fn print_configuration(&self) {
        let config_loader = config_loader(".");
        println!("\n{}", "Configuration".to_string().bold());

        let yaml_code = config_loader.raw_config.as_yaml();
        println!("{}", self.color_yaml(&yaml_code));

        println!("\n{}", "Loaded configuration files".to_string().bold());
        if config_loader.loaded_config_files.is_empty() {
            println!("  {}", "none".to_string().light_red());
        } else {
            for config_file in &config_loader.loaded_config_files {
                println!("  - {}", config_file);
            }
        }
    }

    fn print_worktree(&self) {
        println!("\n{}", "Worktree".to_string().bold());

        let config = config(".");
        println!("  {}", config.worktree());
    }

    fn print_orgs(&self) {
        println!("\n{}", "Git Orgs".to_string().bold());

        if ORG_LOADER.orgs.is_empty() {
            println!("  {}", "none".to_string().light_red());
        } else {
            for org in &ORG_LOADER.orgs {
                let mut org_str = org.config.handle.to_string();
                if let Some(worktree) = &org.config.worktree {
                    org_str.push_str(&format!(" ({})", worktree).light_black());
                }

                if org.config.trusted {
                    org_str.push_str(&format!(" {}", "trusted").light_green().italic());
                } else {
                    org_str.push_str(&format!(" {}", "untrusted").light_red().italic());
                }

                println!("  - {}", org_str);
            }
        }
    }

    fn print_path(&self) {
        println!("\n{}", "Current omnipath".to_string().bold());

        let omnipath = omnipath_entries();
        if omnipath.is_empty() {
            println!("  {}", "none".to_string().light_red());
        } else {
            for path in &omnipath {
                if let Some(package) = &path.package {
                    let mut pkg_string = format!("{} {}", "package:".light_cyan(), package);
                    if !path.path.is_empty() {
                        pkg_string.push_str(&format!(", {} {}", "path:".light_cyan(), path.path));
                    }
                    println!(
                        "  - {}\n    {}",
                        pkg_string,
                        format!("({})", path.full_path).light_black()
                    );
                } else {
                    println!("  - {}", path.path);
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

        let yaml_code = yaml_lines.join("\n  │ ");
        format!("  │ {}", yaml_code)
    }
}
