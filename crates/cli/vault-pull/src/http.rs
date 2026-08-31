//! HTTP helpers for paging exported messages and downloading attachments.
//!
//! Calls are blocking so they can run on worker threads without an async runtime.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;
use vault_http::truncate;

#[derive(Debug, Deserialize)]
/// One page from `GET /v1/export/messages`.
pub struct ExportMessagesResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub messages: Vec<ExportMessage>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
/// One message row from the vault export API.
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
/// Chat metadata attached to each export message.
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
/// One person in a conversation (handle plus optional display name).
pub struct ExportParticipant {
    pub handle: String,
    #[serde(default)]
    pub name_alias: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
/// One attachment on an export message (path, fingerprint, size).
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
    #[serde(default)]
    pub missing_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
/// One reaction (tapback) on an export message.
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
/// Blocking HTTP client used for paging export messages and downloading files.
pub struct HttpSession {
    client: Client,
}

struct ExportUrl<'a> {
    base_url: &'a str,
    /// Route path starting with a slash, for example `/v1/export/messages`.
    path: &'a str,
    /// Fastmail-style search query. Sent even when empty.
    q: &'a str,
    /// Page size. Only the paging route accepts it.
    limit: Option<usize>,
    /// Continuation token from a previous page.
    cursor: Option<&'a str>,
    /// Vault account name. Left out when blank.
    account: &'a str,
    /// Restrict the results to one vault source id.
    source: Option<&'a str>,
}

/// Build the request URL for an export route, leaving out parameters that are
/// absent or blank so the vault sees the same query string it did before.
fn export_url(request: ExportUrl<'_>) -> Result<reqwest::Url> {
    let base = request.base_url.trim().trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}{}", request.path))
        .with_context(|| format!("invalid vault URL {base}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", request.q);
        if let Some(limit) = request.limit {
            pairs.append_pair("limit", &limit.to_string());
        }
        if let Some(cursor) = request.cursor.filter(|s| !s.is_empty()) {
            pairs.append_pair("cursor", cursor);
        }
        let account = request.account.trim();
        if !account.is_empty() {
            pairs.append_pair("account", account);
        }
        if let Some(source) = request.source.map(str::trim).filter(|s| !s.is_empty()) {
            pairs.append_pair("source", source);
        }
    }
    Ok(url)
}

/// Arguments for [`HttpSession::export_messages`].
pub(crate) struct ExportMessagesArgs<'a> {
    pub base_url: &'a str,
    pub key: &'a str,
    pub q: &'a str,
    pub limit: usize,
    pub cursor: Option<&'a str>,
    pub account: &'a str,
    pub source: Option<&'a str>,
}

impl HttpSession {
    /// Blocking HTTP client with a connection pool for worker threads.
    ///
    /// # Errors
    ///
    /// Returns an error when the reqwest client cannot be built.
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: vault_http::build_client()?,
        })
    }

    /// Fetch one page of messages from `GET /v1/export/messages`.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or the body is not valid JSON.
    pub fn export_messages(&self, args: ExportMessagesArgs<'_>) -> Result<ExportMessagesResponse> {
        let ExportMessagesArgs {
            base_url,
            key,
            q,
            limit,
            cursor,
            account,
            source,
        } = args;
        let url = export_url(ExportUrl {
            base_url,
            path: "/v1/export/messages",
            q,
            limit: Some(limit),
            cursor,
            account,
            source,
        })?;

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
            // Prefer the vault's own error text; fall back to the raw body.
            if let Ok(parsed) = serde_json::from_str::<ExportMessagesResponse>(&body)
                && let Some(message) = parsed.error
            {
                bail!("export messages failed ({status}): {message}");
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

    /// Download one attachment by SHA-256 fingerprint to `dest`.
    ///
    /// Bytes are written to a `.part` file first, then renamed, so a crash does
    /// not leave a truncated file at the destination.
    ///
    /// # Errors
    ///
    /// Returns an error when the fingerprint is not 64 hex characters, the vault
    /// returns 404 or another failure, or the file cannot be written.
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
        // Write to a temp file then rename, so a partial download (crash, cancel,
        // network drop) never leaves a truncated file at the destination path.
        let tmp = dest.with_extension("part");
        let mut file = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        std::io::copy(&mut response, &mut file)
            .with_context(|| format!("write {}", tmp.display()))?;
        file.flush()?;
        std::fs::rename(&tmp, dest)
            .with_context(|| format!("rename {} -> {}", tmp.display(), dest.display()))?;
        Ok(())
    }
}
