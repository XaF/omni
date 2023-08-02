use std::path::PathBuf;
use std::process::exit;

use uuid::Uuid;
use shell_escape::escape;

use crate::user_interface::StringColor;
use crate::omni_error;

pub fn uuid_hook() {
    let uuid = Uuid::new_v4();
    println!("{}", uuid.to_string());
}

pub fn init_hook(shell: &str) {
    let current_exe = std::env::current_exe();
    if current_exe.is_err() {
        omni_error!("failed to get current executable path", "hook init");
        exit(1);
    }
    let current_exe = current_exe.unwrap();
    match shell {
        "bash" => dump_integration(
            current_exe,
            include_bytes!("../../shell_integration/omni.bash.tmpl"),
        ),
        "zsh" => dump_integration(
            current_exe,
            include_bytes!("../../shell_integration/omni.zsh.tmpl"),
        ),
        "fish" => dump_integration(
            current_exe,
            include_bytes!("../../shell_integration/omni.fish.tmpl"),
        ),
        _ => {
            omni_error!(
                format!(
                    "invalid shell '{}', omni only supports bash, zsh and fish",
                    shell
                ),
                "hook init"
            );
            exit(1);
        }
    }
}

fn dump_integration(current_exe: PathBuf, integration: &[u8]) {
    let mut integration = String::from_utf8_lossy(integration).to_string();
    integration = integration.replace("{{OMNI_BIN}}", &escape(std::borrow::Cow::Borrowed(current_exe.to_str().unwrap())));
    println!("{}", integration);
}
