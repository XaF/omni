use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvVar;
use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::FifoHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::StringColor;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigCustom {
    pub meet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub met: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unmeet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

impl UpConfigCustom {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut meet = None;
        let mut met = None;
        let mut unmeet = None;
        let mut name = None;
        let mut dir = None;

        if let Some(config_value) = config_value {
            if let Some(value) = config_value.get_as_str_forced("meet") {
                meet = Some(value.to_string());
            }
            if let Some(value) = config_value.get_as_str_forced("met?") {
                met = Some(value.to_string());
            }
            if let Some(value) = config_value.get_as_str_forced("unmeet") {
                unmeet = Some(value.to_string());
            }
            if let Some(value) = config_value.get_as_str_forced("name") {
                name = Some(value.to_string());
            }
            if let Some(value) = config_value.get_as_str_forced("dir") {
                dir = Some(value.to_string());
            }
        }

        if meet.is_none() {
            meet = Some("".to_string());
        }

        UpConfigCustom {
            meet: meet.unwrap(),
            met,
            unmeet,
            name,
            dir,
        }
    }

    pub fn dir(&self) -> Option<String> {
        self.dir.as_ref().map(|dir| dir.to_string())
    }

    pub fn up(
        &self,
        _options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        let name = if let Some(name) = &self.name {
            name.to_string()
        } else {
            self.meet
                .split_whitespace()
                .next()
                .unwrap_or("custom")
                .to_string()
        };

        progress_handler.init(format!("{}:", name).light_blue());

        if self.met().unwrap_or(false) {
            progress_handler.success_with_message("skipping (already met)".light_black());
            return Ok(());
        }

        if let Err(err) = self.meet(environment, progress_handler) {
            progress_handler.error_with_message(format!("{}", err).light_red());
            return Err(UpError::StepFailed(name, progress_handler.step()));
        }

        progress_handler.success();

        Ok(())
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        let name = if let Some(name) = &self.name {
            name.to_string()
        } else {
            self.unmeet
                .clone()
                .unwrap_or("custom".to_string())
                .split_whitespace()
                .next()
                .unwrap_or("custom")
                .to_string()
        };

        progress_handler.init(format!("{}:", name).light_blue());

        if let Some(_unmeet) = &self.unmeet {
            if !self.met().unwrap_or(true) {
                progress_handler.success_with_message("skipping (not met)".light_black());
                return Ok(());
            }

            progress_handler.progress("reverting".light_black());

            if let Err(err) = self.unmeet(progress_handler) {
                progress_handler.error_with_message(format!("{}", err).light_red());
                return Err(err);
            }
        }

        progress_handler.success();

        Ok(())
    }

    fn met(&self) -> Option<bool> {
        if let Some(met) = &self.met {
            let mut command = std::process::Command::new("bash");
            command.arg("-c");
            command.arg(met);
            command.stdout(std::process::Stdio::null());
            command.stderr(std::process::Stdio::null());

            let output = command.output().unwrap();
            Some(output.status.success())
        } else {
            None
        }
    }

    fn meet(
        &self,
        environment: &mut UpEnvironment,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<(), UpError> {
        if !self.meet.is_empty() {
            progress_handler.progress("running (meet) command".to_string());

            let mut fifo_handler =
                FifoHandler::new().map_err(|err| UpError::Exec(format!("{}", err)))?;

            let mut command = TokioCommand::new("bash");
            command.arg("-c");
            command.arg(&self.meet);
            command.env("OMNI_ENV", fifo_handler.path());
            command.stdout(std::process::Stdio::piped());
            command.stderr(std::process::Stdio::piped());

            run_progress(
                &mut command,
                Some(progress_handler),
                RunConfig::default().with_askpass(),
            )?;

            // Close the fifo handler
            fifo_handler.close();

            // Parse the contents of the environment file
            let env_vars = parse_env_file_lines(fifo_handler.lines().into_iter())?;

            // Add the environment operations to the environment
            environment.add_raw_env_vars(env_vars);
        }

        Ok(())
    }

    fn unmeet(&self, progress_handler: &dyn ProgressHandler) -> Result<(), UpError> {
        if let Some(unmeet) = &self.unmeet {
            progress_handler.progress("running (unmeet) command".to_string());

            let mut command = TokioCommand::new("bash");
            command.arg("-c");
            command.arg(unmeet);
            command.stdout(std::process::Stdio::piped());
            command.stderr(std::process::Stdio::piped());

            run_progress(&mut command, Some(progress_handler), RunConfig::default())?;
        }

        Ok(())
    }
}

/// Parse an environment file lines in the format supported by omni, and return a list of
/// environment operations to be performed, which will be added to the dynamic
/// environment of the work directory.
///
/// The file format supports the following:
/// - `unset VAR1 VAR2...`: Unset the specified environment variables
/// - `VAR1=VALUE1`: Set the specified environment variable to the specified value
/// - `VAR1>=VALUE1`: Append the specified value to the specified environment variable
/// - `VAR1<=VALUE1`: Prepend the specified value to the specified environment variable
/// - `VAR1>>=VALUE1`: Append the specified value to the specified environment variable,
///                    working on the assumption that the variable is a path-type variable
///                    (e.g. PATH, LD_LIBRARY_PATH, etc.) separated by colons (':')
/// - `VAR1<<=VALUE1`: Append the specified value to the specified environment variable,
///                    working on the assumption that the variable is a path-type variable
///                    (e.g. PATH, LD_LIBRARY_PATH, etc.) separated by colons (':')
/// - `VAR1-=VALUE1`: Remove the specified value from the specified environment variable,
///                   working on the assumption that the variable is a path-type variable
///                   (e.g. PATH, LD_LIBRARY_PATH, etc.) separated by colons (':')
/// - `VAR1<<EOF`: Set a multi-line value for the specified environment variable, with the
///                value being read from the following lines until an `EOF` is encountered
///                (can be any delimiter value instead of `EOF`)
///
/// Any `export` prefix is simply removed.
/// Any line starting with a `#` is considered a comment and ignored.
/// Any unexpected line format will result in an error.
fn parse_env_file_lines<I, T>(mut lines: I) -> Result<Vec<UpEnvVar>, UpError>
where
    I: Iterator<Item = T>,
    T: AsRef<str>,
{
    // Prepare the output vector
    let mut env_operations = Vec::new();

    'outer: while let Some(line) = lines.next() {
        let line = line.as_ref().trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle unsetting variables through 'unset VAR1 VAR2...'
        if let Some(vars) = line.strip_prefix("unset ") {
            let vars = vars.split_whitespace();

            // Validate that all the values are valid environment variables
            for var in vars {
                if !is_valid_env_name(var) {
                    return Err(UpError::Exec(format!(
                        "invalid environment variable name: '{}', in 'unset' operation",
                        var
                    )));
                }

                env_operations.push(UpEnvVar {
                    name: var.to_string(),
                    operation: EnvOperationEnum::Set,
                    value: None,
                });
            }

            continue;
        }

        // Remove the `export ` prefix if any
        let line = match line.strip_prefix("export ") {
            Some(line) => line,
            None => line,
        };

        // Handle the different operations
        for (operation_str, operation) in &[
            (">>=", EnvOperationEnum::Append),
            ("<<=", EnvOperationEnum::Prepend),
            ("-=", EnvOperationEnum::Remove),
            (">=", EnvOperationEnum::Prefix),
            ("<=", EnvOperationEnum::Suffix),
            ("=", EnvOperationEnum::Set),
        ] {
            if let Some((var, value)) = line.split_once(operation_str) {
                if !is_valid_env_name(var) {
                    return Err(UpError::Exec(format!(
                        "invalid environment variable name: '{}', in '{}' operation",
                        var, operation_str
                    )));
                }

                env_operations.push(UpEnvVar {
                    name: var.to_string(),
                    operation: operation.clone(),
                    value: Some(value.to_string()),
                });

                continue 'outer;
            }
        }

        // Handle the multi-line 'heredoc' operation
        if let Some((var, delimiter)) = line.split_once("<<") {
            if !is_valid_env_name(var) {
                return Err(UpError::Exec(format!(
                    "invalid environment variable name: '{}', in '<<' operation",
                    var
                )));
            }

            // Check if the heredoc is an indented heredoc
            let delimiter = delimiter.trim();
            let (delimiter, remove_all_indent, remove_min_indent) =
                if let Some(delimiter) = delimiter.strip_prefix('-') {
                    (delimiter.trim(), true, false)
                } else if let Some(delimiter) = delimiter.strip_prefix('~') {
                    (delimiter.trim(), false, true)
                } else {
                    (delimiter, false, false)
                };

            // Remove quotes around the delimiter if any, but just one set of quotes
            let delimiter = if delimiter.starts_with('\'') && delimiter.ends_with('\'') {
                &delimiter[1..delimiter.len() - 1]
            } else if delimiter.starts_with('"') && delimiter.ends_with('"') {
                &delimiter[1..delimiter.len() - 1]
            } else {
                delimiter
            };

            // Validate the delimiter
            if !is_valid_delimiter(delimiter) {
                return Err(UpError::Exec(format!(
                    "invalid delimiter: '{}', in '<<' operation",
                    delimiter
                )));
            }

            // Now read the value until the delimiter is encountered
            let mut value = String::new();
            let mut ended = false;
            while let Some(line) = lines.next() {
                let line = line.as_ref();
                let line = if remove_all_indent {
                    line.trim_start_matches(|c| c == ' ' || c == '\t')
                } else {
                    line
                };

                if line == delimiter {
                    ended = true;
                    break;
                }

                value.push_str(&line);
                value.push('\n');
            }

            if !ended {
                return Err(UpError::Exec(format!(
                    "expected delimiter '{}' to end '<<' operation",
                    delimiter
                )));
            }

            // Remove the minimum indentation if requested
            if remove_min_indent {
                // Find the minimum indentation; if that minimum in N, it requires
                // N characters at the beginning of each line to be either all spaces
                // or all tabs for the whole value
                let mut min_indent = 0;
                let mut indent_type = None;
                for line in value.lines() {
                    // Skip empty lines
                    if line.is_empty() {
                        continue;
                    }

                    // Check the indentation type
                    let current_indent_type = line.chars().next().unwrap();

                    if current_indent_type != ' ' && current_indent_type != '\t' {
                        min_indent = 0;
                        break;
                    }

                    match indent_type {
                        Some(indent_type) => {
                            if current_indent_type != indent_type {
                                min_indent = 0;
                                break;
                            }
                        }
                        None => {
                            indent_type = Some(current_indent_type);
                        }
                    }

                    // Now count the indentation
                    let indent = line
                        .chars()
                        .take_while(|c| *c == current_indent_type)
                        .count();

                    if indent < min_indent || min_indent == 0 {
                        min_indent = indent;
                    }
                }

                // Now modify all the lines to remove the minimum indentation
                let mut new_value = String::new();
                for line in value.lines() {
                    if line.len() >= min_indent {
                        new_value.push_str(&line[min_indent..]);
                    }
                    new_value.push('\n');
                }
                value = new_value;
            }

            // Remove the last newline character
            if value.ends_with('\n') {
                value.pop();
            }

            env_operations.push(UpEnvVar {
                name: var.to_string(),
                operation: EnvOperationEnum::Set,
                value: Some(value),
            });

            continue;
        }

        // If no operation was found, return an error
        return Err(UpError::Exec(format!(
            "invalid environment operation: '{}'",
            line
        )));
    }

    Ok(env_operations)
}

fn is_valid_env_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return false;
    }

    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_valid_delimiter(delimiter: &str) -> bool {
    !delimiter.is_empty()
        && delimiter
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}
