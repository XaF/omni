use std::path::Path;
use std::process::Command as StdCommand;

/// Detects the C library used by the current system.
/// Returns true for glibc and false for musl.
#[cfg(target_os = "linux")]
#[inline]
pub fn detect_libc() -> bool {
    // First try filesystem check as it's faster
    if Path::new("/lib/ld-musl-x86_64.so.1").exists() {
        return false;
    }

    if Path::new("/lib64/ld-linux-x86-64.so.2").exists() {
        return true;
    }

    // Fallback to ldd check
    match StdCommand::new("ldd")
        .arg("--version")
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .output()
    {
        Ok(output) => {
            let error_str = String::from_utf8_lossy(&output.stderr);
            error_str.contains("GNU") || error_str.contains("GLIBC")
        }
        Err(_) => true, // Default to glibc if we can't determine
    }
}
