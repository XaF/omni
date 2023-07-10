use std::process::Command;

use time::format_description;
use time::OffsetDateTime;

fn main() {
    let pkg_version = env!("CARGO_PKG_VERSION");
    let version = if pkg_version != "0.0.0-git" {
        pkg_version.to_string()
    } else if let Ok(version) = std::env::var("OMNI_RELEASE_VERSION") {
        version
    } else if let Some(version) = get_git_version() {
        version
    } else {
        let now = OffsetDateTime::now_utc();
        let format = format_description::parse("[year][month][day][hour][minute][second]").unwrap();
        format!("0.0.0-nogit-{}", now.format(&format).unwrap())
    };

    println!("cargo:rustc-env=CARGO_PKG_VERSION={}", version);
}

fn get_git_version() -> Option<String> {
    let mut command = Command::new("git");
    command.args(&["describe", "--tags", "--broken", "--dirty", "--match", "v*"]);

    if let Ok(output) = command.output() {
        if output.status.success() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                let version = version.trim_start_matches('v').to_string();
                return Some(version);
            }
        }
    }

    let mut command = Command::new("git");
    command.args(&[
        "describe", "--tags", "--always", "--broken", "--dirty", "--match", "v*",
    ]);

    if let Ok(output) = command.output() {
        if output.status.success() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                return Some(format!("0.0.0-{}", version));
            }
        }
    }

    return None;
}
