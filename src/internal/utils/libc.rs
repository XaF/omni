use std::path::Path;
use std::process::Command as StdCommand;

/// Detects the C library used by the current system.
/// Returns true for glibc and false for musl.
#[cfg(target_os = "linux")]
#[inline]
pub fn detect_libc() -> bool {
    // First try filesystem check as it's faster
    if Path::new("/lib64/ld-linux-x86-64.so.2").exists()
        || Path::new("/lib32/ld-linux.so.2").exists()
    {
        return true;
    }

    if Path::new("/lib/ld-musl-x86_64.so.1").exists() {
        return false;
    }

    // Fallback to ldd check
    if let Ok(output) = StdCommand::new("ldd")
        .arg("--version")
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
    {
        let output_str = String::from_utf8_lossy(&output.stdout).to_lowercase();
        if output_str.contains("gnu") || output_str.contains("glibc") {
            return true;
        } else if output_str.contains("musl") {
            return false;
        }

        let error_str = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if error_str.contains("musl") {
            return false;
        }
    }

    // Default to glibc if we can't determine
    true
}
