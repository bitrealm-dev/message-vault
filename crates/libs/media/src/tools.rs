use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use anyhow::{Result, bail};

pub fn ffmpeg_available() -> bool {
    resolve_tool("ffmpeg").is_some() && resolve_tool("ffprobe").is_some()
}

pub(crate) fn require_ffmpeg() -> Result<()> {
    if ffmpeg_available() {
        Ok(())
    } else {
        bail!(
            "ffmpeg and ffprobe are required for --media-mode convert/compress. \
             Keep the release-bundled tools in lib/ next to this program (or ../lib/ from cli/), \
             install ffmpeg on PATH, or set MESSAGE_VAULT_IO_BIN to a directory that contains both."
        )
    }
}

fn command_ok(bin: &Path, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Resolve `ffmpeg` / `ffprobe`: sibling of current exe, `lib/` next to the GUI,
/// `../lib/` from `cli/`, legacy parent dir, then `MESSAGE_VAULT_IO_BIN`, then PATH.
fn resolve_tool(name: &str) -> Option<PathBuf> {
    static FFMPEG: OnceLock<Option<PathBuf>> = OnceLock::new();
    static FFPROBE: OnceLock<Option<PathBuf>> = OnceLock::new();
    let cache = match name {
        "ffmpeg" => &FFMPEG,
        "ffprobe" => &FFPROBE,
        _ => return find_tool(name),
    };
    cache.get_or_init(|| find_tool(name)).clone()
}

fn find_tool(name: &str) -> Option<PathBuf> {
    let executable = executable_name(name);

    if let Ok(current) = std::env::current_exe()
        && let Some(dir) = current.parent()
    {
        let candidates = [
            dir.join(&executable),
            dir.join("lib").join(&executable),
            dir.parent()
                .map(|p| p.join("lib").join(&executable))
                .unwrap_or_default(),
            // Legacy flat-root archives.
            dir.parent()
                .map(|p| p.join(&executable))
                .unwrap_or_default(),
        ];
        for candidate in candidates {
            if candidate.as_os_str().is_empty() {
                continue;
            }
            if candidate.is_file() && command_ok(&candidate, &["-version"]) {
                return Some(candidate);
            }
        }
    }

    if let Some(extra) = std::env::var_os("MESSAGE_VAULT_IO_BIN") {
        let candidate = PathBuf::from(extra).join(&executable);
        if candidate.is_file() && command_ok(&candidate, &["-version"]) {
            return Some(candidate);
        }
    }

    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join(&executable);
            if candidate.is_file() && command_ok(&candidate, &["-version"]) {
                return Some(candidate);
            }
        }
    }

    // Last resort: bare name (PATH lookup by the OS / shell semantics).
    let bare = PathBuf::from(&executable);
    if command_ok(&bare, &["-version"]) {
        return Some(bare);
    }

    None
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub(crate) fn run_ffmpeg(args: &[String]) -> Result<()> {
    let ffmpeg = resolve_tool("ffmpeg").ok_or_else(|| {
        anyhow::anyhow!(
            "ffmpeg not found in lib/ (or beside this program), in MESSAGE_VAULT_IO_BIN, or on PATH"
        )
    })?;
    let status = Command::new(ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("ffmpeg failed ({status})")
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Probe {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub bitrate: u64,
}

pub(crate) fn probe_video(path: &std::path::Path) -> Result<Probe> {
    let ffprobe = resolve_tool("ffprobe").ok_or_else(|| {
        anyhow::anyhow!(
            "ffprobe not found in lib/ (or beside this program), in MESSAGE_VAULT_IO_BIN, or on PATH"
        )
    })?;
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height,bit_rate",
            "-of",
            "csv=p=0",
            path.to_str().unwrap_or(""),
        ])
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        bail!("ffprobe failed for {}", path.display());
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.trim().split(',').collect();
    let codec = parts.first().copied().unwrap_or("").to_ascii_lowercase();
    let width = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let height = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let bitrate = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok(Probe {
        codec,
        width,
        height,
        bitrate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn executable_name_matches_platform() {
        let name = executable_name("ffmpeg");
        if cfg!(windows) {
            assert_eq!(name, "ffmpeg.exe");
        } else {
            assert_eq!(name, "ffmpeg");
        }
    }

    #[cfg(unix)]
    #[test]
    fn find_tool_prefers_message_vault_io_bin() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("ffmpeg");
        fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        // SAFETY: test-only env mutation for discovery path coverage.
        unsafe {
            std::env::set_var("MESSAGE_VAULT_IO_BIN", dir.path());
        }
        let found = find_tool("ffmpeg").expect("ffmpeg from MESSAGE_VAULT_IO_BIN");
        assert_eq!(found, script);
        unsafe {
            std::env::remove_var("MESSAGE_VAULT_IO_BIN");
        }
    }
}
