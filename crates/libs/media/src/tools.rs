use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use anyhow::{Result, bail};

struct ToolCache {
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
}

fn tool_cache() -> &'static Mutex<ToolCache> {
    static CACHE: OnceLock<Mutex<ToolCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ToolCache {
        ffmpeg: None,
        ffprobe: None,
    }))
}

fn tools_override() -> &'static Mutex<Option<PathBuf>> {
    static OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    OVERRIDE.get_or_init(|| Mutex::new(None))
}

/// Store a folder-only override for ffmpeg/ffprobe discovery and clear cached paths.
pub fn set_tools_dir(dir: Option<PathBuf>) {
    *tools_override().lock().expect("tools override lock") = dir;
    let mut cache = tool_cache().lock().expect("tool cache lock");
    cache.ffmpeg = None;
    cache.ffprobe = None;
}

/// Current tools-folder override, if any (primarily for tests).
pub fn tools_dir() -> Option<PathBuf> {
    tools_override().lock().expect("tools override lock").clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegToolsProbe {
    pub ok: bool,
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    pub error: Option<String>,
}

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

/// Resolve `ffmpeg` / `ffprobe`: tools-dir override, then sibling of current exe,
/// `lib/` next to the GUI, `../lib/` from `cli/`, legacy parent dir,
/// `MESSAGE_VAULT_IO_BIN`, then PATH.
fn resolve_tool(name: &str) -> Option<PathBuf> {
    let override_dir = tools_override().lock().expect("tools override lock").clone();
    let mut cache = tool_cache().lock().expect("tool cache lock");
    let slot = match name {
        "ffmpeg" => &mut cache.ffmpeg,
        "ffprobe" => &mut cache.ffprobe,
        _ => return find_tool_with_override(name, override_dir.as_deref()),
    };
    if slot.is_none() {
        *slot = find_tool_with_override(name, override_dir.as_deref());
    }
    slot.clone()
}

fn find_tool_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = dir.join(executable_name(name));
    if candidate.is_file() && command_ok(&candidate, &["-version"]) {
        Some(candidate)
    } else {
        None
    }
}

pub fn probe_ffmpeg_tools(dir: Option<&Path>) -> FfmpegToolsProbe {
    let (ffmpeg, ffprobe) = match dir {
        Some(d) => (find_tool_in_dir(d, "ffmpeg"), find_tool_in_dir(d, "ffprobe")),
        None => (resolve_tool("ffmpeg"), resolve_tool("ffprobe")),
    };
    match (ffmpeg, ffprobe) {
        (Some(f), Some(p)) => FfmpegToolsProbe {
            ok: true,
            ffmpeg_path: Some(f),
            ffprobe_path: Some(p),
            error: None,
        },
        (f, p) => {
            let mut parts = Vec::new();
            if f.is_none() {
                parts.push("ffmpeg not found or failed -version");
            }
            if p.is_none() {
                parts.push("ffprobe not found or failed -version");
            }
            FfmpegToolsProbe {
                ok: false,
                ffmpeg_path: f,
                ffprobe_path: p,
                error: Some(parts.join("; ")),
            }
        }
    }
}

fn find_tool(name: &str) -> Option<PathBuf> {
    let override_dir = tools_override().lock().expect("tools override lock").clone();
    find_tool_with_override(name, override_dir.as_deref())
}

fn find_tool_with_override(name: &str, override_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = override_dir {
        return find_tool_in_dir(dir, name);
    }

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
    use std::sync::Mutex;

    fn tools_state_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("tools test state lock")
    }

    struct RestoreToolsDir(Option<PathBuf>);

    impl RestoreToolsDir {
        fn capture() -> Self {
            Self(tools_dir())
        }
    }

    impl Drop for RestoreToolsDir {
        fn drop(&mut self) {
            set_tools_dir(self.0.clone());
        }
    }

    fn write_mock_tool(path: &Path) {
        fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

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
    fn probe_folder_requires_both_tools() {
        let _guard = tools_state_lock();
        let _restore = RestoreToolsDir::capture();
        let dir = tempfile::tempdir().unwrap();
        write_mock_tool(&dir.path().join("ffmpeg"));

        let probe = probe_ffmpeg_tools(Some(dir.path()));
        assert!(!probe.ok);
        assert!(probe.ffmpeg_path.is_some());
        assert!(probe.ffprobe_path.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn set_tools_dir_overrides_and_clears_cache() {
        let _guard = tools_state_lock();
        let _restore = RestoreToolsDir::capture();
        let dir = tempfile::tempdir().unwrap();
        for name in ["ffmpeg", "ffprobe"] {
            write_mock_tool(&dir.path().join(name));
        }
        set_tools_dir(Some(dir.path().to_path_buf()));
        assert_eq!(tools_dir(), Some(dir.path().to_path_buf()));
        assert!(ffmpeg_available());
        set_tools_dir(None);
        assert_eq!(tools_dir(), None);
    }

    #[cfg(unix)]
    #[test]
    fn probe_candidate_folder_does_not_change_override() {
        let _guard = tools_state_lock();
        let _restore = RestoreToolsDir::capture();
        let live = tempfile::tempdir().unwrap();
        for name in ["ffmpeg", "ffprobe"] {
            write_mock_tool(&live.path().join(name));
        }
        set_tools_dir(Some(live.path().to_path_buf()));

        let candidate = tempfile::tempdir().unwrap();
        write_mock_tool(&candidate.path().join("ffmpeg"));

        let _probe = probe_ffmpeg_tools(Some(candidate.path()));
        assert_eq!(tools_dir(), Some(live.path().to_path_buf()));
    }

    #[cfg(unix)]
    #[test]
    fn find_tool_prefers_message_vault_io_bin() {
        let _guard = tools_state_lock();
        let _restore = RestoreToolsDir::capture();
        set_tools_dir(None);
        let dir = tempfile::tempdir().unwrap();
        write_mock_tool(&dir.path().join("ffmpeg"));

        // SAFETY: test-only env mutation for discovery path coverage.
        unsafe {
            std::env::set_var("MESSAGE_VAULT_IO_BIN", dir.path());
        }
        let found = find_tool("ffmpeg").expect("ffmpeg from MESSAGE_VAULT_IO_BIN");
        assert_eq!(found, dir.path().join("ffmpeg"));
        unsafe {
            std::env::remove_var("MESSAGE_VAULT_IO_BIN");
        }
    }
}
