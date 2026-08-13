use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    /// HTTP ingest server (`message-vault-server serve`). Required for `serve`.
    #[serde(default)]
    pub server: Option<ServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Bind address (default `127.0.0.1:8080`).
    #[serde(default = "default_server_bind")]
    pub bind: String,
    /// Max size of one asset (single PUT or multipart complete), in bytes.
    /// Default 512 MiB.
    #[serde(default = "default_asset_max_bytes")]
    pub asset_max_bytes: u64,
    /// Multipart part size advertised to clients, in bytes. Default 64 MiB
    /// (under Cloudflare Free/Pro ~100 MB). Must be ≤ `asset_max_bytes`.
    #[serde(default = "default_asset_part_size")]
    pub asset_part_size: usize,
    /// Attachments at or above this size historically skipped SHA-256 at upload
    /// completion. Multipart completion always checks fingerprints now; this field
    /// remains for config compatibility.
    #[serde(default = "default_asset_hash_threshold_bytes")]
    pub asset_hash_threshold_bytes: u64,
    /// Allowed Cross-Origin Resource Sharing (CORS) origins. Empty = same-origin
    /// only (no `Access-Control-Allow-Origin`). CORS is the browser rule that
    /// decides which other websites may call this API.
    /// Use `["*"]` only for local debugging. Example: `["https://app.example.com"]`.
    #[serde(default)]
    pub cors_origins: Vec<String>,
}

fn default_server_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_asset_max_bytes() -> u64 {
    512 * 1024 * 1024
}

fn default_asset_part_size() -> usize {
    64 * 1024 * 1024
}

fn default_asset_hash_threshold_bytes() -> u64 {
    20 * 1024 * 1024
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    pub db: PathBuf,
    /// Root for per-account data (`data/<account_id>/…`).
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Directory name for originals under each account source (default `assets`).
    #[serde(default = "default_assets_dir_name")]
    pub assets_dir: String,
    /// Directory name for converted media under each account source.
    #[serde(default = "default_assets_converted_dir_name")]
    pub assets_converted_dir: String,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("data")
}

fn default_assets_dir_name() -> String {
    "assets".to_string()
}

fn default_assets_converted_dir_name() -> String {
    "assets_converted".to_string()
}

/// Safe source slug for path segments and `messages.source` values.
///
/// # Errors
///
/// Returns an error when the id is empty, too long, or uses disallowed characters.
pub fn validate_source_id(source: &str) -> Result<()> {
    let s = source.trim();
    if s.is_empty() {
        bail!("source id must not be empty");
    }
    if s.len() > 64 {
        bail!("source id must be at most 64 characters");
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        bail!("source id '{s}' must use only lowercase letters, digits, hyphens, and underscores");
    }
    if s.starts_with('-') || s.starts_with('_') {
        bail!("source id must not start with '-' or '_'");
    }
    Ok(())
}

/// Reject absolute paths and `..` so joins stay under an approved root.
///
/// # Errors
///
/// Returns an error when `name` is empty, absolute, or contains `..`.
pub fn safe_rel_path(name: &str) -> Result<PathBuf> {
    use std::path::{Component, Path};

    let name = name.trim();
    if name.is_empty() {
        bail!("empty attachment path");
    }
    let path = Path::new(name);
    if path.is_absolute() {
        bail!("attachment path must be relative: {name}");
    }
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("unsafe attachment path: {name}");
            }
        }
    }
    if out.as_os_str().is_empty() {
        bail!("empty attachment path after normalize: {name}");
    }
    Ok(out)
}

/// Join `rel` under `root` after rejecting traversal. Does not follow the final path.
///
/// # Errors
///
/// Returns an error when `rel` is not a safe relative path.
pub fn resolve_under_root(root: &Path, rel: &str) -> Result<PathBuf> {
    Ok(root.join(safe_rel_path(rel)?))
}

impl PathsConfig {
    /// Originals: `data_dir/<account_id>/<source_id>/<assets_dir>`.
    pub fn assets_dir_for_account(&self, account_id: &str, source_id: &str) -> PathBuf {
        self.data_dir
            .join(account_id)
            .join(source_id)
            .join(&self.assets_dir)
    }

    /// Converted media: `data_dir/<account_id>/<source_id>/<assets_converted_dir>`.
    pub fn assets_converted_dir_for_account(&self, account_id: &str, source_id: &str) -> PathBuf {
        self.data_dir
            .join(account_id)
            .join(source_id)
            .join(&self.assets_converted_dir)
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut config: Config = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;

        let abs_config = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .context("failed to get current directory")?
                .join(path)
        };
        let config_dir = abs_config
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let repo = config_dir
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(config_dir);

        config.paths.db = resolve_path(repo, &config.paths.db);
        config.paths.data_dir = resolve_path(repo, &config.paths.data_dir);

        Ok(config)
    }

    /// Server settings for `serve`. Fails if `[server]` is missing.
    pub fn require_server(&self) -> Result<&ServerConfig> {
        let server = self
            .server
            .as_ref()
            .context("config missing [server] section (needed for serve)")?;
        if server.asset_part_size == 0 {
            bail!("server.asset_part_size must be > 0");
        }
        if server.asset_max_bytes == 0 {
            bail!("server.asset_max_bytes must be > 0");
        }
        if server.asset_part_size as u64 > server.asset_max_bytes {
            bail!(
                "server.asset_part_size ({}) must be ≤ server.asset_max_bytes ({})",
                server.asset_part_size,
                server.asset_max_bytes
            );
        }
        Ok(server)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Hanko,
    Local,
}

impl AuthMode {
    pub fn from_env() -> Self {
        Self::parse(&std::env::var("VAULT_AUTH").unwrap_or_default())
    }

    fn parse(raw: &str) -> Self {
        match raw.to_lowercase().as_str() {
            "hanko" => AuthMode::Hanko,
            _ => AuthMode::Local,
        }
    }
}

fn resolve_path(base: &Path, configured: &Path) -> PathBuf {
    if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        base.join(configured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_source_id_accepts_slugs() {
        assert!(validate_source_id("imessage").is_ok());
        assert!(validate_source_id("go-sms-pro").is_ok());
        assert!(validate_source_id("sms_backup_plus").is_ok());
        assert!(validate_source_id("a1").is_ok());
    }

    #[test]
    fn validate_source_id_rejects_bad() {
        assert!(validate_source_id("").is_err());
        assert!(validate_source_id("iMessage").is_err());
        assert!(validate_source_id("../x").is_err());
        assert!(validate_source_id("-bad").is_err());
        assert!(validate_source_id("has space").is_err());
    }

    #[test]
    fn safe_rel_path_rejects_traversal() {
        assert!(safe_rel_path("attachments/a.jpg").is_ok());
        assert!(safe_rel_path("../etc/passwd").is_err());
        assert!(safe_rel_path("/etc/passwd").is_err());
        assert!(safe_rel_path("").is_err());
        assert!(safe_rel_path("a/../../b").is_err());
    }

    #[test]
    fn resolve_under_root_keeps_paths_inside() {
        let root = PathBuf::from("/tmp/export");
        let joined = resolve_under_root(&root, "attachments/a.jpg").unwrap();
        assert_eq!(joined, root.join("attachments/a.jpg"));
        assert!(resolve_under_root(&root, "../outside").is_err());
    }

    #[test]
    fn auth_mode_parse_hanko_case_insensitive() {
        assert_eq!(AuthMode::parse("hanko"), AuthMode::Hanko);
        assert_eq!(AuthMode::parse("Hanko"), AuthMode::Hanko);
        assert_eq!(AuthMode::parse("HANKO"), AuthMode::Hanko);
    }

    #[test]
    fn auth_mode_parse_defaults_to_local() {
        assert_eq!(AuthMode::parse(""), AuthMode::Local);
        assert_eq!(AuthMode::parse("local"), AuthMode::Local);
        assert_eq!(AuthMode::parse("anything-else"), AuthMode::Local);
    }
}
