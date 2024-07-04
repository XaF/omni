use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::config::up::utils::get_command_output;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::dynenv::update_dynamic_env_for_command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PathCommandHelpParser {
    #[serde(rename = "argparse")]
    PythonArgParse,
}

impl PathCommandHelpParser {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "argparse" => Some(Self::PythonArgParse),
            _ => None,
        }
    }

    // pub fn to_str(&self) -> &str {
    // match self {
    // Self::PythonArgParse => "argparse",
    // }
    // }

    pub fn call_and_parse(&self, source: &str) -> Option<ParsedCommandHelp> {
        let mut help_cmd = TokioCommand::new(source);
        help_cmd.arg("--help");
        help_cmd.stdout(std::process::Stdio::piped());
        help_cmd.stderr(std::process::Stdio::piped());

        update_dynamic_env_for_command(source);
        let output = get_command_output(&mut help_cmd, RunConfig::new());
        update_dynamic_env_for_command(".");

        match output {
            Err(err) => {
                // TODO: remove DEBUG
                eprintln!(
                    "Failed to get help for command: {}, error: {:?}",
                    source, err
                );
                None
            }
            Ok(output) if !output.status.success() => {
                // TODO: remove DEBUG
                let msg = format!(
                    "--help failed: {}",
                    String::from_utf8(output.stderr)
                        .unwrap()
                        .replace('\n', " ")
                        .trim()
                );
                eprintln!("{}", msg);
                None
            }
            Ok(output) => {
                let output = String::from_utf8(output.stdout).unwrap().to_string();
                self.parse(&output)
            }
        }
    }

    pub fn parse(&self, help: &str) -> Option<ParsedCommandHelp> {
        match self {
            Self::PythonArgParse => argparse_help_parser(help),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedCommandHelp {
    pub desc: Option<String>,
    pub epilog: Option<String>,
    pub syntax: Option<CommandSyntax>,
}

fn get_main_param(param: &str) -> &str {
    // Get characters until the first space or comma
    let mut chars = param.chars();
    let end = chars
        .position(|c| c == ' ' || c == ',')
        .unwrap_or(param.len());
    &param[..end]
}

fn check_param_required_from_usage(main_param: &str, usage: &str) -> bool {
    let mut found = false;
    let mut block_count = 0;

    let usage_chars_addressable = usage.chars().collect::<Vec<char>>();
    for (i, c) in usage.chars().enumerate() {
        if block_count < 2 && usage[i..].starts_with(main_param) {
            let end = i + main_param.len();

            // Get the first character after the parameter
            let prev_char = usage_chars_addressable.get(i - 1).copied().unwrap_or(' ');
            let next_char = usage_chars_addressable.get(end).copied().unwrap_or(' ');

            // Check if char is alnum or dash or underscore
            let is_prev_separator =
                !(prev_char.is_alphanumeric() || prev_char == '-' || prev_char == '_');
            let is_next_separator =
                !(next_char.is_alphanumeric() || next_char == '-' || next_char == '_');

            // Check if the next char is not a continuation of the parameter
            if is_prev_separator && is_next_separator {
                found = true;
                break;
            }
        }

        if c == '[' {
            block_count += 1;
        } else if c == ']' {
            block_count -= 1;
        }
    }

    found && block_count == 0
}

fn argparse_help_parser(help: &str) -> Option<ParsedCommandHelp> {
    let mut usage = None;
    let mut desc = None;
    let mut parameters: Vec<SyntaxOptArg> = vec![];
    let mut epilog = None;

    // To read the usage line
    let mut read_usage_indent = None;

    // To read the description and epilog
    let mut read_desc = false;
    let mut read_epilog = false;

    // To read positionals and options
    const PARAMS_INDENT: &str = "  ";
    let mut read_positionals = false;
    let mut read_options = false;
    let mut current_opt_arg = None;

    for line in help.lines() {
        if line.starts_with("usage: ") {
            // Get the "rest" of the line
            let line_without_prefix = line.trim_start_matches("usage: ");

            // The first word until either a [, - or a capital letter following
            // a space is the command name, so we want to get rid of that, let's
            // find that character if any
            let chars = line_without_prefix.chars().collect::<Vec<char>>();
            let start_of_parameters = chars
                .iter()
                .enumerate()
                .find(|(i, c)| {
                    *i > 0
                        && (**c == '[' || **c == '-' || c.is_ascii_uppercase())
                        && chars[i - 1] == ' '
                })
                .map(|(i, _)| i)
                .unwrap_or(0);

            // Set the usage to the line without the prefix, we will append to
            // this if there are more lines
            let line_without_prefix = line_without_prefix[start_of_parameters..]
                .trim()
                .to_string();
            usage = Some(line_without_prefix);

            // We are now reading the usage, so if nothing else matches,
            // we know indented lines being read are part of the usage
            let start_of_parameters = start_of_parameters + "usage: ".len();
            read_usage_indent = Some(" ".repeat(start_of_parameters));

            // Continue to the next line
            continue;
        } else if line.starts_with("positional arguments:") {
            read_options = false;
            read_positionals = true;
            read_desc = false;
            read_epilog = false;

            // Continue to the next line
            continue;
        } else if line.starts_with("options:") {
            read_positionals = false;
            read_options = true;
            read_desc = false;
            read_epilog = false;

            // Continue to the next line
            continue;
        } else if !line.is_empty() && !line.starts_with(" ") && !line.ends_with(":") {
            if desc.is_none() && parameters.is_empty() {
                read_desc = true;
            } else if epilog.is_none() {
                read_epilog = true;
            }
        }

        if let Some(usage_indent) = &read_usage_indent {
            // If we are reading the usage, we will append to the usage
            // until we find a line that doesn't start with a space
            if let Some(line) = line.strip_prefix(usage_indent) {
                if let Some(usage) = usage.as_mut() {
                    usage.push_str(" ");
                    usage.push_str(line.trim());

                    // Continue to the next line
                    continue;
                } else {
                    read_usage_indent = None;
                }
            } else {
                read_usage_indent = None;
            }
        }

        if read_options || read_positionals {
            if let Some(line) = line.strip_prefix(PARAMS_INDENT) {
                let is_param = !line.is_empty() && (read_positionals && !line.starts_with(" "))
                    || (read_options && line.starts_with("-"));

                // Check if we're reading a new parameter by checking if the first
                // character is a dash or a space
                if is_param {
                    if let Some(parameter) = current_opt_arg.take() {
                        parameters.push(parameter);
                    }

                    // The description will start after two spaces, so we split
                    // the line into two parts to get the parameter and the description
                    let mut parts = line.splitn(2, "  ");
                    let name = parts.next().unwrap().trim();
                    let desc = parts.next().and_then(|s| Some(s.trim().to_string()));

                    // Figure out if the parameter is required by checking if there is
                    // any parameter in usage that starts with ' [<param>'
                    let main_param = get_main_param(name);
                    let required = match usage {
                        Some(ref usage) => check_param_required_from_usage(main_param, usage),
                        None => read_positionals, // Default to positional arguments being required
                    };

                    // Store the parameter so we can append to the description
                    current_opt_arg = Some(SyntaxOptArg::new(name.to_string(), desc, required));

                    // Continue to the next line
                    continue;
                } else if let Some(parameter) = current_opt_arg.as_mut() {
                    let append = line.trim();
                    if !append.is_empty() {
                        let mut desc = match parameter.desc {
                            Some(ref desc) => format!("{} ", desc),
                            None => String::new(),
                        };
                        desc.push_str(append);

                        parameter.desc = Some(desc.to_string());
                    }

                    // Continue to the next line
                    continue;
                }
            } else if let Some(parameter) = current_opt_arg.take() {
                parameters.push(parameter);
            }
        }

        // Handle description and epilog
        if read_desc {
            if desc.is_none() {
                desc = Some(line.to_string());
            } else {
                desc.as_mut().map(|desc| {
                    desc.push_str("\n");
                    desc.push_str(line);
                });
            }
        } else if read_epilog {
            if epilog.is_none() {
                epilog = Some(line.to_string());
            } else {
                epilog.as_mut().map(|epilog| {
                    epilog.push_str("\n");
                    epilog.push_str(line);
                });
            }
        }
    }

    // If any parameter was being read, consider it done and add it to the list
    if let Some(parameter) = current_opt_arg.take() {
        parameters.push(parameter);
    }

    let syntax = if parameters.is_empty() && usage.is_none() {
        None
    } else {
        Some(CommandSyntax { usage, parameters })
    };

    if syntax.is_none() && desc.is_none() && epilog.is_none() {
        return None;
    }

    // Trim the description and epilog if any
    desc = desc.map(|desc| desc.trim().to_string());
    epilog = epilog.map(|epilog| epilog.trim().to_string());

    Some(ParsedCommandHelp {
        desc,
        epilog,
        syntax,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_command_help_eq(left: Option<ParsedCommandHelp>, right: Option<ParsedCommandHelp>) {
        match (&left, &right) {
            (Some(left), Some(right)) => {
                assert_eq!(left.desc, right.desc, "'desc' is not matching");
                assert_eq!(left.epilog, right.epilog, "'epilog' is not matching");
                match (&left.syntax, &right.syntax) {
                    (Some(left), Some(right)) => {
                        assert_eq!(left.usage, right.usage, "'syntax.usage' is not matching");
                        assert_eq!(
                            left.parameters, right.parameters,
                            "'syntax.parameters' is not matching"
                        );
                    }
                    (None, None) => {}
                    _ => assert_eq!(left, right),
                }
            }
            (None, None) => {}
            _ => assert_eq!(left, right),
        }
    }

    #[test]
    fn test_argparse_help_parser_basic() {
        let help = include_str!("../../../tests/fixtures/argparse/basic.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: None,
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h]".to_string()),
                parameters: [SyntaxOptArg {
                    name: "-h".to_string(),
                    alt_names: vec!["--help".to_string()],
                    desc: Some("show this help message and exit".to_string()),
                    ..SyntaxOptArg::default()
                }]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_complex() {
        let help = include_str!("../../../tests/fixtures/argparse/complex.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: Some("argparse tester".to_string()),
            epilog: Some("test epilog".to_string()),
            syntax: Some(CommandSyntax {
                usage: Some("[-h] [--root] [--no-help] [--outdir out_dir] [--in-dir IN_DIR] [--foo | --bar] {first,f,second,third} ...".to_string()),
                parameters: [
                    SyntaxOptArg {
                        name: "-h".to_string(),
                        alt_names: vec!["--help".to_string()],
                        desc: Some("show this help message and exit".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--root".to_string(),
                        desc: Some("root flag".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--no-help".to_string(),
                        desc: None,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--outdir".to_string(),
                        alt_names: vec!["-o".to_string()],
                        placeholder: Some("out_dir".to_string()),
                        desc: Some("output directory".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--in-dir".to_string(),
                        alt_names: vec!["-i".to_string()],
                        placeholder: Some("IN_DIR".to_string()),
                        desc: Some("input directory".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--foo".to_string(),
                        desc: Some("foo".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--bar".to_string(),
                        desc: Some("bar".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_positional_argument() {
        let help = include_str!("../../../tests/fixtures/argparse/positional-argument.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: None,
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h] x".to_string()),
                parameters: [
                    SyntaxOptArg {
                        name: "x".to_string(),
                        desc: Some("arg".to_string()),
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "-h".to_string(),
                        alt_names: vec!["--help".to_string()],
                        desc: Some("show this help message and exit".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_description_oneline() {
        let help = include_str!("../../../tests/fixtures/argparse/description-oneline.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: Some("desc".to_string()),
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h]".to_string()),
                parameters: [SyntaxOptArg {
                    name: "-h".to_string(),
                    alt_names: vec!["--help".to_string()],
                    desc: Some("show this help message and exit".to_string()),
                    ..SyntaxOptArg::default()
                }]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_description_multiline() {
        let help = include_str!("../../../tests/fixtures/argparse/description-multiline.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: Some("This description\nspans multiple lines.\n\n  this line is indented.\n    and also this.\n\nNow this should be a separate paragraph.".to_string()),
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h] [--dummy DUMMY]".to_string()),
                parameters: [
                    SyntaxOptArg {
                        name: "-h".to_string(),
                        alt_names: vec!["--help".to_string()],
                        desc: Some("show this help message and exit".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--dummy".to_string(),
                        placeholder: Some("DUMMY".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_epilog_oneline() {
        let help = include_str!("../../../tests/fixtures/argparse/epilog-oneline.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: None,
            epilog: Some("epi".to_string()),
            syntax: Some(CommandSyntax {
                usage: Some("[-h]".to_string()),
                parameters: [SyntaxOptArg {
                    name: "-h".to_string(),
                    alt_names: vec!["--help".to_string()],
                    desc: Some("show this help message and exit".to_string()),
                    ..SyntaxOptArg::default()
                }]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_epilog_multiline() {
        let help = include_str!("../../../tests/fixtures/argparse/epilog-multiline.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: None,
            epilog: Some("This epilog\nspans multiple lines.\n\n  this line is indented.\n    and also this.\n\nNow this should be a separate paragraph.".to_string()),
            syntax: Some(CommandSyntax {
                usage: Some("[-h]".to_string()),
                parameters: [SyntaxOptArg {
                    name: "-h".to_string(),
                    alt_names: vec!["--help".to_string()],
                    desc: Some("show this help message and exit".to_string()),
                    ..SyntaxOptArg::default()
                }]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_positional_argument_with_option_name() {
        let help = include_str!(
            "../../../tests/fixtures/argparse/positional-argument-with-option-name.txt"
        );
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: Some("argparse tester".to_string()),
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h] [--meerkat] meerkat".to_string()),
                parameters: [
                    SyntaxOptArg {
                        name: "meerkat".to_string(),
                        desc: Some("meerkat argument".to_string()),
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "-h".to_string(),
                        alt_names: vec!["--help".to_string()],
                        desc: Some("show this help message and exit".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--meerkat".to_string(),
                        desc: Some("meerkat flag".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_exclusive_group() {
        let help = include_str!("../../../tests/fixtures/argparse/exclusive-group.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: Some("argparse tester".to_string()),
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h] [--root] [--foo | --bar] root {first,f,second} ...".to_string()),
                parameters: [
                    SyntaxOptArg {
                        name: "root".to_string(),
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "-h".to_string(),
                        alt_names: vec!["--help".to_string()],
                        desc: Some("show this help message and exit".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--root".to_string(),
                        desc: Some("root flag".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--foo".to_string(),
                        desc: Some("foo".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--bar".to_string(),
                        desc: Some("bar".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_lower_upper_refs() {
        let help = include_str!("../../../tests/fixtures/argparse/lower-upper-refs.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: None,
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h] [-d D] [-D D]".to_string()),
                parameters: [
                    SyntaxOptArg {
                        name: "-h".to_string(),
                        alt_names: vec!["--help".to_string()],
                        desc: Some("show this help message and exit".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "-d".to_string(),
                        placeholder: Some("D".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "-D".to_string(),
                        placeholder: Some("D".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }

    #[test]
    fn test_argparse_help_parser_many_alt_options() {
        let help = include_str!("../../../tests/fixtures/argparse/many-alt-options.txt");
        let actual = argparse_help_parser(help);

        let expected = Some(ParsedCommandHelp {
            desc: None,
            epilog: None,
            syntax: Some(CommandSyntax {
                usage: Some("[-h] [--foo FOO]".to_string()),
                parameters: [
                    SyntaxOptArg {
                        name: "-h".to_string(),
                        alt_names: vec!["--help".to_string()],
                        desc: Some("show this help message and exit".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        name: "--foo".to_string(),
                        alt_names: vec![
                            "--foo2", "--foo3", "--foo4", "--foo5", "--foo6", "--foo7", "--foo8",
                            "--foo9", "--foo10", "--foo11", "--foo12", "--foo13", "--foo14",
                            "--foo15", "--foo16", "--foo17", "--foo18", "--foo19", "--foo20",
                            "--foo21", "--foo22", "--foo23", "--foo24", "--foo25", "--foo26",
                            "--foo27", "--foo28", "--foo29", "--foo30", "--foo31", "--foo32",
                            "--foo33", "--foo34", "--foo35", "--foo36", "--foo37", "--foo38",
                            "--foo39", "--foo40", "--foo41", "--foo42", "--foo43", "--foo44",
                            "--foo45", "--foo46", "--foo47", "--foo48", "--foo49", "--foo50",
                            "--foo51", "--foo52", "--foo53", "--foo54", "--foo55", "--foo56",
                            "--foo57", "--foo58", "--foo59", "--foo60", "--foo61", "--foo62",
                            "--foo63", "--foo64", "--foo65", "--foo66", "--foo67", "--foo68",
                            "--foo69", "--foo70", "--foo71", "--foo72", "--foo73", "--foo74",
                            "--foo75", "--foo76", "--foo77", "--foo78", "--foo79", "--foo80",
                            "--foo81", "--foo82", "--foo83", "--foo84", "--foo85", "--foo86",
                            "--foo87", "--foo88", "--foo89", "--foo90", "--foo91", "--foo92",
                            "--foo93", "--foo94", "--foo95", "--foo96", "--foo97", "--foo98",
                            "--foo99", "--foo100",
                        ]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect(),
                        placeholder: Some("FOO".to_string()),
                        desc: Some("foo".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ]
                .to_vec(),
            }),
        });

        parsed_command_help_eq(actual, expected);
    }
}
