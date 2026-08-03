//! Locate and run the external `wtsexporter` CLI.

use anyhow::{Context, Result, bail};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PINNED_HINT: &str = "whatsapp-chat-exporter>=0.13";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Android,
    Ios,
}

impl Platform {
    pub fn as_flag(self) -> &'static str {
        match self {
            Self::Android => "-a",
            Self::Ios => "-i",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WtsexporterArgs {
    pub platform: Platform,
    /// Search root for relative defaults (`msgstore.db`, `wa.db`, …). Not the process cwd.
    pub input: PathBuf,
    /// Scratch directory for wtsexporter (media extract + JSON). Must outlive convert.
    pub work_dir: PathBuf,
    /// Key file path or crypt15 hex string (`-k`).
    pub key: Option<String>,
    pub backup: Option<PathBuf>,
    pub wa: Option<PathBuf>,
    pub media: Option<PathBuf>,
    pub db: Option<PathBuf>,
    pub business: bool,
    pub move_media: bool,
}

/// Resolve `wtsexporter`: `WTSEXPORTER` → sibling of this exe → `cli/` next to the GUI →
/// legacy parent dir → `MESSAGE_VAULT_IO_BIN` → `PATH`.
pub(crate) fn resolve_wtsexporter() -> Result<PathBuf> {
    if let Some(explicit) = env::var_os("WTSEXPORTER") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "WTSEXPORTER is set but not a file: {}. Install with \
             pip install '{PINNED_HINT}' or place the release binary in cli/ next to this tool.",
            path.display()
        );
    }

    let executable = if cfg!(windows) {
        "wtsexporter.exe"
    } else {
        "wtsexporter"
    };
    let mut tried = Vec::new();

    if let Ok(current) = env::current_exe()
        && let Some(dir) = current.parent()
    {
        let candidates = [
            dir.join(executable),
            dir.join("cli").join(executable),
            // Legacy flat-root archives.
            dir.parent()
                .map(|p| p.join(executable))
                .unwrap_or_default(),
        ];
        for candidate in candidates {
            if candidate.as_os_str().is_empty() {
                continue;
            }
            tried.push(candidate.clone());
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Some(extra) = env::var_os("MESSAGE_VAULT_IO_BIN") {
        let candidate = PathBuf::from(extra).join(executable);
        tried.push(candidate.clone());
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join(executable);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    bail!(
        "Could not find {executable}. Install with: pip install '{PINNED_HINT}' \
         (or pip install 'whatsapp-chat-exporter[android_backup,crypt15]'), \
         put the KnugiHK release binary in cli/ next to this tool / in MESSAGE_VAULT_IO_BIN, \
         or set WTSEXPORTER. Tried: {}",
        tried
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// Run wtsexporter in `args.work_dir`; write JSON to `json_out`.
/// Returns stderr+stdout for logging.
pub(crate) fn run_wtsexporter(
    bin: &Path,
    args: &WtsexporterArgs,
    json_out: &Path,
) -> Result<String> {
    if !args.work_dir.is_dir() {
        bail!("work dir does not exist: {}", args.work_dir.display());
    }
    let paths = resolve_forwarded_paths(args)?;
    let out_dir = json_out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;

    let mut cmd = Command::new(bin);
    // Scratch cwd so iOS/Android extract does not pollute the GUI launch directory.
    cmd.current_dir(&args.work_dir)
        .arg(args.platform.as_flag())
        .arg("--no-html")
        .arg("--no-banner")
        .arg("-o")
        .arg(out_dir)
        .arg("-j")
        .arg(json_out)
        // wtsexporter uses tqdm; without this, progress bars spam piped capture
        // (GUI only shows the dump after the process exits).
        .env("TQDM_DISABLE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(db) = &paths.db {
        cmd.arg("-d").arg(db);
    }
    if let Some(key) = paths.key.as_deref() {
        cmd.arg("-k").arg(key);
    }
    push_opt(&mut cmd, "-b", paths.backup.as_deref());
    push_opt(&mut cmd, "-w", paths.wa.as_deref());
    push_opt(&mut cmd, "-m", paths.media.as_deref());
    if args.business {
        cmd.arg("--business");
    }
    if args.move_media {
        cmd.arg("-c");
    }

    let output = cmd.output().map_err(|err| {
        let hint = if err.kind() == std::io::ErrorKind::NotFound {
            " (often a broken pipx/venv shim: the script exists but its Python interpreter does not — try `pipx reinstall whatsapp-chat-exporter` or set WTSEXPORTER to a working binary)"
        } else {
            ""
        };
        anyhow::anyhow!("spawn {}: {err}{hint}", bin.display())
    })?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!(
            "wtsexporter failed ({}){}\n{}",
            output.status,
            if combined.trim().is_empty() { "" } else { ":" },
            combined.trim()
        );
    }
    if !json_out.is_file() {
        bail!(
            "wtsexporter finished but JSON missing at {}. Output:\n{}",
            json_out.display(),
            combined.trim()
        );
    }
    Ok(combined)
}

struct ForwardedPaths {
    key: Option<String>,
    backup: Option<PathBuf>,
    wa: Option<PathBuf>,
    media: Option<PathBuf>,
    db: Option<PathBuf>,
}

/// Absolutize user paths and fill Android/iOS defaults from `input` when missing.
fn resolve_forwarded_paths(args: &WtsexporterArgs) -> Result<ForwardedPaths> {
    let search = input_search_root(&args.input)?;

    let db = match &args.db {
        Some(p) => Some(absolutize(p)?),
        None if args.input.is_file() => Some(absolutize(&args.input)?),
        None => first_existing(&[
            search.join("msgstore.db"),
            search.join("ChatStorage.sqlite"),
        ]),
    };

    let wa = match &args.wa {
        Some(p) => Some(absolutize(p)?),
        None => first_existing(&[
            search.join("wa.db"),
            search.join("ContactsV2.sqlite"),
            search.join("AppDomainGroup-group.net.whatsapp.WhatsApp.shared/ContactsV2.sqlite"),
            search.join("AppDomainGroup-group.net.whatsapp.WhatsAppSMB.shared/ContactsV2.sqlite"),
        ]),
    };

    let media = match &args.media {
        Some(p) => Some(absolutize(p)?),
        None => first_existing(&[
            search.join("WhatsApp"),
            search.join("AppDomainGroup-group.net.whatsapp.WhatsApp.shared"),
            search.join("AppDomainGroup-group.net.whatsapp.WhatsAppSMB.shared"),
        ]),
    };

    let backup = match &args.backup {
        Some(p) => Some(absolutize(p)?),
        None => None,
    };

    let key = match args.key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) if looks_like_path(k) => {
            let path = absolutize(Path::new(k))?;
            Some(path.to_string_lossy().into_owned())
        }
        Some(k) => Some(k.to_string()),
        None => None,
    };

    Ok(ForwardedPaths {
        key,
        backup,
        wa,
        media,
        db,
    })
}

fn input_search_root(input: &Path) -> Result<PathBuf> {
    if input.is_dir() {
        return Ok(absolutize(input)?);
    }
    if input.is_file() {
        let parent = input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        return Ok(absolutize(parent)?);
    }
    bail!("input path does not exist: {}", input.display());
}

fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.exists()).cloned()
}

fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.contains('\\') || s.ends_with(".key") || Path::new(s).exists()
}

fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = env::current_dir().context("resolve current working directory")?;
    Ok(cwd.join(path))
}

fn push_opt(cmd: &mut Command, flag: &str, path: Option<&Path>) {
    if let Some(p) = path {
        cmd.arg(flag).arg(p);
    }
}
