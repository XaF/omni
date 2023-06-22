use std::process::exit;

use regex::Regex;
use serde_yaml;

use crate::internal::commands::path::omnipath;
use crate::internal::config::config_loader;
use crate::internal::config::CommandSyntax;
use crate::internal::git::ORG_LOADER;
use crate::internal::user_interface::StringColor;
use crate::internal::CACHE;
use crate::internal::ENV;
use crate::omni_error;
use crate::omni_header;

#[derive(Debug, Clone)]
pub struct StatusCommand {}

impl StatusCommand {
    pub fn new() -> Self {
        Self {}
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
            arguments: vec![],
            options: vec![],
        })
    }

    pub fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    pub fn exec(&self, argv: Vec<String>) {
        if argv.len() > 0 {
            omni_error!("too many arguments");
            exit(1);
        }

        println!("{}", omni_header!());

        self.print_shell_integration();
        self.print_configuration();
        self.print_cache();
        self.print_orgs();
        self.print_path();

        exit(0);
    }

    pub fn autocompletion(&self) -> bool {
        false
    }

    pub fn autocomplete(&self, _comp_cword: usize, _argv: Vec<String>) {
        ()
    }

    fn print_shell_integration(&self) {
        println!("\n{}", format!("Shell integration").bold());
        let status = if let Some(_) = &ENV.omni_cmd_file {
            format!("loaded").light_green()
        } else {
            format!("not loaded").light_red()
        };
        println!("  {}", status);
    }

    fn print_configuration(&self) {
        let config_loader = config_loader(".");
        println!("\n{}", format!("Configuration").bold());

        let yaml_code = config_loader.raw_config.as_yaml();
        println!("{}", self.color_yaml(&yaml_code));

        println!("\n{}", format!("Loaded configuration files").bold());
        if config_loader.loaded_config_files.is_empty() {
            println!("  {}", format!("none").light_red());
        } else {
            for config_file in &config_loader.loaded_config_files {
                println!("  - {}", config_file);
            }
        }
    }

    fn print_cache(&self) {
        println!("\n{}", format!("Cache").bold());

        // Use serde_yaml to convert the cache to yaml
        let yaml_code = serde_yaml::to_string(&*CACHE).unwrap();
        println!("{}", self.color_yaml(&yaml_code));
    }

    fn print_orgs(&self) {
        println!("\n{}", format!("Git Orgs").bold());

        if ORG_LOADER.orgs.is_empty() {
            println!("  {}", format!("none").light_red());
        } else {
            for org in &ORG_LOADER.orgs {
                let mut org_str = format!("{}", org.config.handle);
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
        println!("\n{}", format!("Current omnipath").bold());

        let omnipath = omnipath();
        if omnipath.is_empty() {
            println!("  {}", format!("none").light_red());
        } else {
            for path in &omnipath {
                println!("  - {}", path);
            }
        }
    }

    fn color_yaml(&self, yaml_code: &str) -> String {
        let yaml_lines = &yaml_code.lines().collect::<Vec<&str>>()[1..];

        let pattern = r#"^(\s*)(\-\s*)?(("[^"]+"|[a-zA-Z0-9_\?!\-]+)\s*:)\s*"#;
        let regex_keys = Regex::new(pattern).unwrap();
        // Replace the keys by themselves, colored
        let yaml_lines = yaml_lines
            .iter()
            .map(|line| {
                if let Some(captures) = regex_keys.captures(line) {
                    let key = captures.get(3).unwrap().as_str();
                    let colored_key = key.to_string().light_cyan();
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
