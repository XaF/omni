use std::process::Command;

use time::format_description;
use time::OffsetDateTime;

fn main() {
    let version = if let Ok(version) = std::env::var("OMNI_RELEASE_VERSION") {
        version
    } else if let Some(version) = get_git_version() {
        version
    } else {
        let now = OffsetDateTime::now_utc();
        let format = format_description::parse("[year][month][day][hour][minute][second]").unwrap();
        format!("0.0.0-nogit-{}", now.format(&format).unwrap())
    };

    println!("cargo:rustc-env=CARGO_PKG_VERSION={}", version);
    println!("cargo:rerun-if-env-changed=OMNI_RELEASE_VERSION");
    println!("cargo:rerun-if-changed=.git/HEAD");
}

fn get_git_version() -> Option<String> {
    let mut command = Command::new("git");
    command.args(&[
        "describe",
        "--tags",
        "--broken",
        "--dirty",
        "--match", "v*",
    ])]);

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
        "describe",
        "--tags",
        "--always",
        "--broken",
        "--dirty",
        "--match", "v*",
    ])]);

    if let Ok(output) = command.output() {
        if output.status.success() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                return Some(format!("0.0.0-{}", version));
            }
        }
    }

    return None;
}
