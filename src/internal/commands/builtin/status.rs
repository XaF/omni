use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::IsTerminal;
use std::process::exit;

use regex::Regex;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::path::omnipath_entries;
use crate::internal::commands::Command;
use crate::internal::config::config;
use crate::internal::config::config_loader;
use crate::internal::config::parser::ParseArgsValue;
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

impl From<BTreeMap<String, ParseArgsValue>> for StatusCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let flags = [
            "shell_integration",
            "config",
            "config_files",
            "worktree",
            "orgs",
            "path",
        ];

        let flag_values: HashMap<String, bool> = flags
            .iter()
            .map(|flag| {
                let flag_name = flag.to_string();
                let value = match args.get(&flag_name) {
                    Some(ParseArgsValue::SingleBoolean(Some(value))) => *value,
                    _ => unreachable!("no value for flag {}", flag),
                };
                (flag.to_string(), value)
            })
            .collect();

        let selected = flag_values.values().filter(|&&selected| selected).count();
        let none_selected = selected == 0;

        let single = selected == 1;
        let shell_integration = *flag_values.get("shell_integration").unwrap() || none_selected;
        let config = *flag_values.get("config").unwrap();
        let config_files = *flag_values.get("config_files").unwrap() || none_selected;
        let worktree = *flag_values.get("worktree").unwrap() || none_selected;
        let orgs = *flag_values.get("orgs").unwrap() || none_selected;
        let path = *flag_values.get("path").unwrap() || none_selected;

        Self {
            single,
            shell_integration,
            config,
            config_files,
            worktree,
            orgs,
            path,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusCommand {}

impl StatusCommand {
    pub fn new() -> Self {
        Self {}
    }

    fn print_shell_integration(&self, args: &StatusCommandArgs) {
        if !args.shell_integration {
            return;
        }

        let prefix = if args.single {
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

    fn print_configuration(&self, args: &StatusCommandArgs) {
        if !args.config {
            return;
        }

        if !args.single {
            println!("\n{}", "Configuration".bold());
        }

        let config = config(".");
        match serde_yaml::to_value(&config) {
            Ok(value) => {
                let sorted_value = sort_serde_yaml(&value);
                let yaml_code = serde_yaml::to_string(&sorted_value).unwrap();
                println!("{}", self.color_yaml(&yaml_code, args.single));
            }
            Err(err) => {
                omni_error!(format!("failed to serialize configuration: {}", err));
                exit(1);
            }
        }
    }

    fn print_configuration_files(&self, args: &StatusCommandArgs) {
        if !args.config_files {
            return;
        }

        let prefix = if args.single {
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

    fn print_worktree(&self, args: &StatusCommandArgs) {
        if !args.worktree {
            return;
        }

        let prefix = if args.single {
            "".to_string()
        } else {
            println!("\n{}", "Worktree".bold());
            "  ".to_string()
        };

        let config = config(".");
        println!("{}{}", prefix, config.worktree());
    }

    fn print_orgs(&self, args: &StatusCommandArgs) {
        if !args.orgs {
            return;
        }

        let prefix = if args.single {
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

    fn print_path(&self, args: &StatusCommandArgs) {
        if !args.path {
            return;
        }

        let prefix = if args.single {
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

    fn color_yaml(&self, yaml_code: &str, single: bool) -> String {
        let yaml_lines = &mut yaml_code.lines().collect::<Vec<&str>>();
        if yaml_lines[0] == "---" {
            // Remove the first line if it's "---"; as it's not very useful
            yaml_lines.remove(0);
        }

        // If the output is not a terminal, return the yaml as is;
        // this is required as usually we do not colorize when
        // both stdout and stderr are not terminals
        if !std::io::stdout().is_terminal() {
            return yaml_lines.join("\n");
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

        if single {
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
        let command = Command::Builtin(self.clone_boxed());
        let args = StatusCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        if !args.single {
            println!("{}", omni_header!());
        }

        self.print_shell_integration(&args);
        self.print_configuration(&args);
        self.print_configuration_files(&args);
        self.print_worktree(&args);
        self.print_orgs(&args);
        self.print_path(&args);

        exit(0);
    }
}
