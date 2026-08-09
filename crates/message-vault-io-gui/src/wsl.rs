//! Windows interoperability used when the Linux GUI runs under WSL.

use std::process::Command;

pub fn is_wsl() -> bool {
    std::env::var_os("WSL_INTEROP").is_some() || std::env::var_os("WSL_DISTRO_NAME").is_some()
}

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
