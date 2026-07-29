use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    /// HTTP ingest server (`message-vault-rs serve`). Required for `serve`.
    #[serde(default)]
    pub server: Option<ServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Bind address (default `127.0.0.1:8080`).
    #[serde(default = "default_server_bind")]
    pub bind: String,
}

fn default_server_bind() -> String {
    "127.0.0.1:8080".to_string()
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
    /// Used by web `process-assets` and path helpers (not the CLI binary itself).
    #[serde(default = "default_assets_converted_dir_name")]
    #[allow(dead_code)]
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

const DEFAULT_CONTACTS_CSV_HEADER: &str =
    "phones,first_name,last_name,exclude,label_1,label_2,label_3,label_4,label_5\n";
const DEFAULT_EXCLUDE_CSV_HEADER: &str = "phones,label\n";

/// Safe source slug for path segments and `messages.source` values.
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

impl PathsConfig {
    /// Per-account contacts CSV: `data_dir/<account_id>/contacts.csv`.
    pub fn contacts_csv_for_account(&self, account_id: &str) -> PathBuf {
        self.data_dir.join(account_id).join("contacts.csv")
    }

    /// Per-account exclude CSV: `data_dir/<account_id>/exclude.csv`.
    pub fn exclude_csv_for_account(&self, account_id: &str) -> PathBuf {
        self.data_dir.join(account_id).join("exclude.csv")
    }

    /// Originals: `data_dir/<account_id>/<source_id>/<assets_dir>`.
    pub fn assets_dir_for_account(&self, account_id: &str, source_id: &str) -> PathBuf {
        self.data_dir
            .join(account_id)
            .join(source_id)
            .join(&self.assets_dir)
    }

    /// Converted media: `data_dir/<account_id>/<source_id>/<assets_converted_dir>`.
    #[allow(dead_code)]
    pub fn assets_converted_dir_for_account(&self, account_id: &str, source_id: &str) -> PathBuf {
        self.data_dir
            .join(account_id)
            .join(source_id)
            .join(&self.assets_converted_dir)
    }

    /// Ensure per-account CSVs exist (empty headers when missing).
    pub fn ensure_account_csvs(&self, account_id: &str) -> Result<(PathBuf, PathBuf)> {
        let contacts = self.contacts_csv_for_account(account_id);
        let exclude = self.exclude_csv_for_account(account_id);
        if let Some(parent) = contacts.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create account data dir {}", parent.display()))?;
        }
        write_csv_if_missing(&contacts, DEFAULT_CONTACTS_CSV_HEADER)?;
        write_csv_if_missing(&exclude, DEFAULT_EXCLUDE_CSV_HEADER)?;
        Ok((contacts, exclude))
    }
}

fn write_csv_if_missing(dest: &Path, empty_header: &str) -> Result<()> {
    if dest.is_file() {
        return Ok(());
    }
    fs::write(dest, empty_header)
        .with_context(|| format!("write empty CSV header {}", dest.display()))?;
    Ok(())
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
        self.server
            .as_ref()
            .context("config missing [server] section (needed for serve)")
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
}
