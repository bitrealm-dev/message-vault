//! Blocking HTTP helpers for vault export + asset download.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ExportMessagesResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub messages: Vec<ExportMessage>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExportCountResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub messages: u64,
    #[serde(default)]
    pub attachments: u64,
    #[serde(default)]
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportMessage {
    pub id: i64,
    pub source: String,
    #[serde(default)]
    pub guid: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub timestamp_utc: Option<String>,
    #[serde(default)]
    pub is_from_me: bool,
    #[serde(default)]
    pub sender: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub is_announcement: bool,
    #[serde(default)]
    pub is_reply: bool,
    #[serde(default)]
    pub thread_originator_guid: Option<String>,
    #[serde(default)]
    pub thread_originator_part: Option<i64>,
    #[serde(default)]
    pub num_replies: i64,
    pub conversation: ExportConversation,
    #[serde(default)]
    pub attachments: Vec<ExportAttachment>,
    #[serde(default)]
    pub tapbacks: Vec<ExportTapback>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportConversation {
    pub id: i64,
    pub chat_identifier: String,
    #[serde(default)]
    pub service: Option<String>,
    pub conversation_type: String,
    #[serde(default)]
    pub group_title: Option<String>,
    #[serde(default)]
    pub participants: Vec<ExportParticipant>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportParticipant {
    pub handle: String,
    #[serde(default)]
    pub name_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportAttachment {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub original_name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub is_sticker: bool,
    #[serde(default)]
    pub transcription: Option<String>,
    /// Asset length in bytes when the vault echoes import metadata.
    #[serde(default, alias = "size", alias = "bytes")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportTapback {
    #[serde(default)]
    pub part_index: i64,
    pub kind: String,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub is_from_me: bool,
    #[serde(default)]
    pub sender: Option<String>,
}

#[derive(Clone)]
pub struct HttpSession {
    client: Client,
}

impl HttpSession {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .pool_max_idle_per_host(8)
            .build()
            .context("build HTTP client")?;
        Ok(Self { client })
    }

    pub fn export_messages(
        &self,
        base_url: &str,
        key: &str,
        q: &str,
        limit: usize,
        cursor: Option<&str>,
        account: &str,
        source: Option<&str>,
    ) -> Result<ExportMessagesResponse> {
        let base = base_url.trim().trim_end_matches('/');
        let mut url = reqwest::Url::parse(&format!("{base}/v1/export/messages"))
            .with_context(|| format!("invalid vault URL {base}"))?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("q", q);
            qp.append_pair("limit", &limit.to_string());
            if let Some(c) = cursor.filter(|s| !s.is_empty()) {
                qp.append_pair("cursor", c);
            }
            let account = account.trim();
            if !account.is_empty() {
                qp.append_pair("account", account);
            }
            if let Some(s) = source.map(str::trim).filter(|s| !s.is_empty()) {
                qp.append_pair("source", s);
            }
        }

        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(120))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .send()
            .context("GET /v1/export/messages")?;

        let status = response.status();
        let body = response.text().unwrap_or_default();
        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<ExportMessagesResponse>(&body) {
                if let Some(msg) = err.error {
                    bail!("export messages failed ({status}): {msg}");
                }
            }
            bail!(
                "export messages failed ({status}): {}",
                truncate(&body, 300)
            );
        }
        let parsed: ExportMessagesResponse =
            serde_json::from_str(&body).context("parse export messages response")?;
        if !parsed.ok {
            bail!(
                "export messages rejected: {}",
                parsed.error.unwrap_or_else(|| "unknown error".into())
            );
        }
        Ok(parsed)
    }

    /// `GET /v1/export/messages/count`. Returns `Ok(None)` when the vault does not
    /// support the route (HTTP 404), so callers can fall back to paging.
    pub fn export_message_count(
        &self,
        base_url: &str,
        key: &str,
        q: &str,
        account: &str,
        source: Option<&str>,
    ) -> Result<Option<ExportCountResponse>> {
        let base = base_url.trim().trim_end_matches('/');
        let mut url = reqwest::Url::parse(&format!("{base}/v1/export/messages/count"))
            .with_context(|| format!("invalid vault URL {base}"))?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("q", q);
            let account = account.trim();
            if !account.is_empty() {
                qp.append_pair("account", account);
            }
            if let Some(s) = source.map(str::trim).filter(|s| !s.is_empty()) {
                qp.append_pair("source", s);
            }
        }

        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(120))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .send()
            .context("GET /v1/export/messages/count")?;

        let status = response.status();
        let body = response.text().unwrap_or_default();
        if status.as_u16() == 404 {
            return Ok(None);
        }
        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<ExportCountResponse>(&body) {
                if let Some(msg) = err.error {
                    bail!("export message count failed ({status}): {msg}");
                }
            }
            bail!(
                "export message count failed ({status}): {}",
                truncate(&body, 300)
            );
        }
        let parsed: ExportCountResponse =
            serde_json::from_str(&body).context("parse export message count response")?;
        if !parsed.ok {
            bail!(
                "export message count rejected: {}",
                parsed.error.unwrap_or_else(|| "unknown error".into())
            );
        }
        Ok(Some(parsed))
    }

    pub fn download_asset(
        &self,
        base_url: &str,
        key: &str,
        account: &str,
        source: &str,
        sha256: &str,
        dest: &Path,
    ) -> Result<()> {
        // Validate sha256 is a 64-char hex string before putting it in the URL.
        let sha_clean = sha256.trim();
        if sha_clean.len() != 64 || !sha_clean.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("invalid SHA-256 digest for asset download: {sha256}");
        }
        let base = base_url.trim().trim_end_matches('/');
        let mut url = reqwest::Url::parse(&format!("{base}/v1/assets/{sha_clean}"))
            .with_context(|| format!("invalid vault URL {base}"))?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("source", source);
            let account = account.trim();
            if !account.is_empty() {
                qp.append_pair("account", account);
            }
        }

        let mut response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(300))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .send()
            .context("GET /v1/assets")?;

        let status = response.status();
        if status.as_u16() == 404 {
            bail!("asset not found: {sha256} (source={source})");
        }
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            bail!("asset download failed ({status}): {}", truncate(&body, 300));
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("mkdir {}", parent.display()))?;
        }
        let mut file =
            File::create(dest).with_context(|| format!("create {}", dest.display()))?;
        std::io::copy(&mut response, &mut file)
            .with_context(|| format!("write {}", dest.display()))?;
        file.flush()?;
        Ok(())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

