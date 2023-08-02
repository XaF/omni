use regex::Regex;
use term_size;

#[macro_export]
macro_rules! omni_header {
    () => {
        format!(
            "{} - omnipotent tool {}",
            "omni".to_string().bold(),
            format!("(v{})", env!("CARGO_PKG_VERSION"))
                .to_string()
                .italic()
                .light_black(),
        )
    };
}

#[macro_export]
macro_rules! omni_print {
    ($message:expr) => {
        eprintln!("{} {}", "omni:".to_string().light_cyan(), $message,)
    };
}

#[macro_export]
macro_rules! omni_info {
    ($message:expr) => {
        let cmd = std::env::var("OMNI_SUBCOMMAND").unwrap_or("".to_string());
        let cmd = if cmd != "" {
            format!(" {}:", cmd).light_yellow()
        } else {
            "".to_string()
        };
        eprintln!(
            "{}",
            format!("{}{} {}", "omni:".to_string().light_cyan(), cmd, $message)
        );
    };
    ($message:expr, $cmd:expr) => {
        let cmd = if $cmd != "" {
            format!(" {}:", $cmd).light_yellow()
        } else {
            "".to_string()
        };
        eprintln!(
            "{}",
            format!("{}{} {}", "omni:".to_string().light_cyan(), cmd, $message)
        );
    };
}

#[macro_export]
macro_rules! omni_warning {
    ($message:expr) => {
        let cmd = std::env::var("OMNI_SUBCOMMAND").unwrap_or("".to_string());
        let cmd = if cmd != "" {
            format!(" {}", cmd)
        } else {
            "".to_string()
        };
        eprintln!(
            "{}",
            format!(
                "{}{} {}",
                "omni:".to_string().light_cyan(),
                format!("{} warning:", cmd).to_string().yellow(),
                $message
            )
        );
    };
    ($message:expr, $cmd:expr) => {
        let cmd = if $cmd != "" {
            format!(" {}", $cmd)
        } else {
            "".to_string()
        };
        eprintln!(
            "{}",
            format!(
                "{}{} {}",
                "omni:".to_string().light_cyan(),
                format!("{} warning:", cmd).to_string().yellow(),
                $message
            )
        );
    };
}

#[macro_export]
macro_rules! omni_error {
    ($message:expr) => {
        let cmd = std::env::var("OMNI_SUBCOMMAND").unwrap_or("".to_string());
        let cmd = if cmd != "" {
            format!(" {}", cmd)
        } else {
            "".to_string()
        };
        eprintln!(
            "{}",
            format!(
                "{}{} {}",
                "omni:".to_string().light_cyan(),
                format!("{} command failed:", cmd).to_string().red(),
                $message
            )
        );
    };
    ($message:expr, $cmd:expr) => {
        let cmd = if $cmd != "" {
            format!(" {}", $cmd)
        } else {
            "".to_string()
        };
        eprintln!(
            "{}",
            format!(
                "{}{} {}",
                "omni:".to_string().light_cyan(),
                format!("{} command failed:", cmd).to_string().red(),
                $message
            )
        );
    };
}

pub fn term_width() -> usize {
    let width = if let Some((width, _)) = term_size::dimensions() {
        width
    } else {
        80
    };

    let max = 120;
    if width < max + 4 {
        width - 4
    } else {
        max
    }
}

pub fn wrap_blocks(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![];
    let paragraphs = text.split("\n\n");

    for (index, paragraph) in paragraphs.enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.extend(
            wrap_text(paragraph, width)
                .iter()
                .map(|line| line.trim().to_string()),
        );
    }

    lines
}

use lazy_static::lazy_static;

lazy_static! {
    static ref SPLIT_PATTERN: Regex = Regex::new(r"\s").unwrap();
    static ref COLOR_PATTERN: Regex = Regex::new(r"\x1B(?:\[(?:\d+)(?:;\d+)*m)").unwrap();
}

pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    let mut line_width = 0;
    for word in SPLIT_PATTERN.split(text) {
        let word_width = COLOR_PATTERN.replace_all(word, "").len();
        if line_width + word_width > width {
            lines.push(line);
            line = String::new();
            line_width = 0;
        }
        line.push_str(word);
        line_width += word_width;
        line.push(' ');
        line_width += 1;
    }
    lines.push(line);

    lines
}
