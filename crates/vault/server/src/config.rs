//! Config file model ([`Config`]) plus path/source validation and the
//! environment-driven settings ([`AuthMode`], [`GuestDemoSettings`]).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use message_ir_format::UNSAFE_ATTACHMENT_PATH_PREFIX;
use serde::{Deserialize, Serialize};

/// Complete server configuration, loaded from a TOML file.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Filesystem locations (database, per-account data).
    pub paths: PathsConfig,
    /// HTTP ingest server (`message-vault-server serve`). Required for `serve`.
    #[serde(default)]
    pub server: Option<ServerConfig>,
    /// Database engine and connection URL. When `url` is set (a
    /// `postgres://…` or `sqlite://…` URL), `serve` connects through it
    /// instead of `paths.db`. Required for Postgres.
    #[serde(default)]
    pub database: DatabaseConfig,
}

/// `[database]` section: optional connection URL selecting the engine.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DatabaseConfig {
    /// Connection URL (`postgres://…` or `sqlite://…`). Unset = SQLite at
    /// `paths.db`.
    #[serde(default)]
    pub url: Option<String>,
}

/// `[server]` section: HTTP bind address, CORS, and asset upload limits.
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
    /// Serve Swagger UI at `/docs` and the spec at `/openapi.json`. Default false.
    #[serde(default = "default_openapi_ui")]
    pub openapi_ui: bool,
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

fn default_openapi_ui() -> bool {
    false
}

/// `[paths]` section: database file and per-account data directories.
#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    /// SQLite database file path.
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
                bail!("{UNSAFE_ATTACHMENT_PATH_PREFIX}: {name}");
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
    /// Read and parse a TOML config file. Relative `paths.db` and
    /// `paths.data_dir` values resolve against the directory above the config
    /// file's folder (the repo root for `config/config.toml`).
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
/// Sign-in mechanism: local account passwords or Hanko passkeys.
pub enum AuthMode {
    /// Hanko passkey sign-in via `POST /v1/auth/hanko/session`.
    Hanko,
    /// Local vault account login (username and password).
    Local,
}

impl AuthMode {
    /// Auth mode from the `VAULT_AUTH` environment variable: `hanko` when set,
    /// otherwise `local`.
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

/// Hosted Try it demo settings, read from `GUEST_DEMO_POOL`,
/// `GUEST_POOL_MIN`, `GUEST_POOL_MAX`, and `GUEST_SESSION_SECS`.
#[derive(Debug, Clone, Copy)]
pub struct GuestDemoSettings {
    /// Whether the hosted Try it demo is on.
    pub enabled: bool,
    /// Minimum unused ready guest accounts kept in the pool.
    pub pool_min: u32,
    /// Maximum unused ready guest accounts.
    pub pool_max: u32,
    /// Lifetime of one demo session, in seconds.
    pub session_secs: u64,
}

fn env_truthy(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes"
    )
}

impl GuestDemoSettings {
    /// Read demo settings from the environment; unset or malformed values
    /// fall back to the defaults.
    pub fn from_env() -> Self {
        Self::parse(
            &std::env::var("GUEST_DEMO_POOL").unwrap_or_default(),
            &std::env::var("GUEST_POOL_MIN").unwrap_or_default(),
            &std::env::var("GUEST_POOL_MAX").unwrap_or_default(),
            &std::env::var("GUEST_SESSION_SECS").unwrap_or_default(),
        )
    }

    /// Demo settings with the hosted Try it demo off (tests only).
    #[cfg(test)]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            pool_min: 2,
            pool_max: 20,
            session_secs: 86_400,
        }
    }

    pub(crate) fn parse(pool: &str, min: &str, max: &str, secs: &str) -> Self {
        let enabled = env_truthy(pool);
        let pool_min = min.parse::<u32>().unwrap_or(2).max(1);
        let mut pool_max = max.parse::<u32>().unwrap_or(20).max(1);
        if pool_max < pool_min {
            pool_max = pool_min;
        }
        let session_secs = secs.parse::<u64>().unwrap_or(86_400).max(60);
        Self {
            enabled,
            pool_min,
            pool_max,
            session_secs,
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

    #[test]
    fn guest_demo_settings_default_disabled() {
        let s = GuestDemoSettings::parse("", "", "", "");
        assert!(!s.enabled);
        assert_eq!(s.pool_min, 2);
        assert_eq!(s.pool_max, 20);
        assert_eq!(s.session_secs, 86_400);
    }

    #[test]
    fn guest_demo_settings_truthy_and_clamps() {
        let s = GuestDemoSettings::parse("true", "0", "100", "60");
        assert!(s.enabled);
        assert_eq!(s.pool_min, 1);
        assert_eq!(s.pool_max, 100);
        assert_eq!(s.session_secs, 60);
        let s = GuestDemoSettings::parse("yes", "5", "3", "not-a-number");
        assert!(s.enabled);
        assert_eq!(s.pool_max, 5);
        assert_eq!(s.session_secs, 86_400);
    }

    #[test]
    fn openapi_ui_defaults_false() {
        let raw = r#"
bind = "127.0.0.1:8080"
"#;
        let cfg: ServerConfig = toml::from_str(raw).unwrap();
        assert!(!cfg.openapi_ui);
    }

    #[test]
    fn openapi_ui_can_enable() {
        let raw = r#"
bind = "127.0.0.1:8080"
openapi_ui = true
"#;
        let cfg: ServerConfig = toml::from_str(raw).unwrap();
        assert!(cfg.openapi_ui);
    }

    const PACKAGED_ORIGINS: &[&str] = &[
        "https://tauri.localhost",
        "http://tauri.localhost",
        "tauri://localhost",
    ];

    /// `scripts/run-vault-dev.sh` only uncomments the `# cors_origins =` line.
    /// That line must be a complete array or first-run / `--reset-demo` configs
    /// are invalid TOML.
    #[test]
    fn example_cors_origins_uncomments_to_a_complete_array() {
        let example = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../config/config.toml.example"
        ));
        let cors_lines: Vec<&str> = example
            .lines()
            .filter(|line| {
                line.starts_with("# cors_origins =") || line.starts_with("cors_origins =")
            })
            .collect();
        assert_eq!(
            cors_lines.len(),
            1,
            "run-vault-dev.sh uncomments one cors_origins line"
        );
        assert!(
            cors_lines[0].contains('[') && cors_lines[0].contains(']'),
            "cors_origins must stay on one line so sed yields a closed array, got {}",
            cors_lines[0]
        );

        let uncommented: String = example
            .lines()
            .map(|line| {
                line.strip_prefix("# cors_origins =")
                    .map(|rest| format!("cors_origins ={rest}"))
                    .unwrap_or_else(|| line.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n");
        let cfg: Config =
            toml::from_str(&uncommented).expect("example after run-vault-dev.sh sed must parse");
        let origins = &cfg
            .server
            .as_ref()
            .expect("[server] in example")
            .cors_origins;
        for origin in [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            PACKAGED_ORIGINS[0],
            PACKAGED_ORIGINS[1],
            PACKAGED_ORIGINS[2],
        ] {
            assert!(
                origins.iter().any(|item| item == origin),
                "missing {origin} in {origins:?}"
            );
        }
    }

    #[test]
    fn docker_config_includes_packaged_desktop_origins() {
        let docker = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../config/config.docker.toml"
        ));
        let cfg: Config = toml::from_str(docker).expect("config.docker.toml must parse");
        let origins = &cfg
            .server
            .as_ref()
            .expect("[server] in docker config")
            .cors_origins;
        for origin in PACKAGED_ORIGINS {
            assert!(
                origins.iter().any(|item| item == origin),
                "missing {origin} in {origins:?}"
            );
        }
    }
}
