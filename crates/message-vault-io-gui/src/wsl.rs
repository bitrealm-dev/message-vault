//! Windows helpers used when the Linux GUI runs under WSL.
//!
//! WSL (Windows Subsystem for Linux) is a Linux environment on Windows.
//! Opening URLs and native file dialogs must go through Windows so they
//! appear on the host desktop instead of inside the Linux session.

use std::process::Command;

/// True when the process is running inside WSL.
pub fn is_wsl() -> bool {
    std::env::var_os("WSL_INTEROP").is_some() || std::env::var_os("WSL_DISTRO_NAME").is_some()
}

/// Open `url` in the default browser.
///
/// On WSL this launches the Windows browser via `cmd.exe`.
/// Elsewhere it uses the `open` crate.
///
/// # Errors
///
/// Returns a message if the browser cannot be started, or if the Windows
/// launcher exits with a non-zero status.
pub fn open_url(url: &str) -> Result<(), String> {
    if !is_wsl() {
        return open::that(url).map_err(|error| error.to_string());
    }

    let status = Command::new("cmd.exe")
        .args(["/C", "start", "", url])
        .status()
        .map_err(|error| format!("could not start the Windows browser: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Windows browser launcher exited with {status}"))
    }
}
