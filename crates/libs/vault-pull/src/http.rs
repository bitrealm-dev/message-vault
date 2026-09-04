//! HTTP helpers for paging exported messages and downloading attachments.
//!
//! Calls are blocking so they can run on worker threads without an async
//! runtime. The session type is [`vault_http::HttpSession`].

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::Method;
use serde::Deserialize;
use vault_http::{VaultHttpError, trim_base_url, truncate};

pub use vault_http::HttpSession;

/// One page from `GET /v1/export/messages`: `{items, total, limit, offset}`.
#[derive(Debug, Deserialize)]
pub struct ExportMessagesPage {
    #[serde(default)]
    pub items: Vec<Message>,
    #[serde(default)]
    pub total: u64,
}

/// The vault's failure body: `{error}`.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
}

/// The sentence to show for a failed response: the body's `error` when it has
/// one, otherwise the body itself, clipped.
fn error_sentence(body: &str, _status: u16) -> String {
    match serde_json::from_str::<ErrorBody>(body) {
        Ok(ErrorBody {
            error: Some(message),
        }) if !message.trim().is_empty() => message,
        _ => truncate(body, 300),
    }
}

#[derive(Debug, Clone, Deserialize)]
/// One message row from the vault export API.
pub struct Message {
    pub id: i64,
    pub source: String,
    /// Platform service, e.g. `imessage`. The vault sends it on the message,
    /// not on the conversation.
    #[serde(default)]
    pub service: Option<String>,
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
    pub conversation: MessageConversation,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub tapbacks: Vec<Tapback>,
}

#[derive(Debug, Clone, Deserialize)]
/// Chat metadata attached to each export message.
pub struct MessageConversation {
    pub id: i64,
    pub chat_identifier: String,
    pub conversation_type: String,
    #[serde(default)]
    pub group_title: Option<String>,
    #[serde(default)]
    pub participants: Vec<Participant>,
}

#[derive(Debug, Clone, Deserialize)]
/// One person in a conversation, mirroring the vault's `Participant` schema.
///
/// `name` is never empty — the vault falls back to the handle when nothing
/// else names the person — so it is not `#[serde(default)]`: a response
/// missing it should fail loudly rather than silently becoming `None`.
///
/// `handle` is optional because the vault sends `"handle": null` for a
/// participant a backup named without recording any address for them. It was
/// once `String`, which made every page carrying such a person fail to
/// deserialize and aborted the whole pull.
pub struct Participant {
    #[serde(default)]
    pub handle: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
/// One attachment on an export message (path, fingerprint, transcription).
pub struct Attachment {
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
    #[serde(default)]
    pub missing_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
/// One reaction (tapback) on an export message.
pub struct Tapback {
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

struct ExportUrl<'a> {
    base_url: &'a str,
    /// A query in the vault's search language. Sent even when empty.
    q: &'a str,
    /// Page size.
    limit: usize,
    /// Row offset.
    offset: usize,
    /// Vault account name. Left out when blank.
    account: &'a str,
}

/// Build the request URL for `GET /v1/export/messages`, leaving out `account`
/// when it is blank.
fn export_url(request: ExportUrl<'_>) -> Result<reqwest::Url> {
    let base = trim_base_url(request.base_url);
    let mut url = reqwest::Url::parse(&format!("{base}/v1/export/messages"))
        .with_context(|| format!("invalid vault URL {base}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", request.q);
        pairs.append_pair("limit", &request.limit.to_string());
        pairs.append_pair("offset", &request.offset.to_string());
        let account = request.account.trim();
        if !account.is_empty() {
            pairs.append_pair("account", account);
        }
    }
    Ok(url)
}

/// Arguments for [`export_messages`].
pub(crate) struct ExportMessagesArgs<'a> {
    pub base_url: &'a str,
    pub key: &'a str,
    pub q: &'a str,
    pub limit: usize,
    pub offset: usize,
    pub account: &'a str,
}

/// Fetch one page of messages from `GET /v1/export/messages`.
///
/// # Errors
///
/// Returns an error when the request fails or the body is not valid JSON.
pub fn export_messages(
    http: &HttpSession,
    args: ExportMessagesArgs<'_>,
) -> Result<ExportMessagesPage> {
    let ExportMessagesArgs {
        base_url,
        key,
        q,
        limit,
        offset,
        account,
    } = args;
    let url = export_url(ExportUrl {
        base_url,
        q,
        limit,
        offset,
        account,
    })?;

    let response = http
        .request_url(Method::GET, url, key)
        .timeout(Duration::from_secs(120))
        .send()
        .context("GET /v1/export/messages")?;

    let status = response.status();
    let body = response.text().unwrap_or_default();
    if !status.is_success() {
        let detail = error_sentence(&body, status.as_u16());
        return Err(VaultHttpError::new(
            status.as_u16(),
            format!("export messages failed ({status}): {detail}"),
        )
        .into());
    }
    let parsed: ExportMessagesPage =
        serde_json::from_str(&body).context("parse export messages response")?;
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
    http: &HttpSession,
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
    let base = trim_base_url(base_url);
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

    let mut response = http
        .request_url(Method::GET, url, key)
        .timeout(Duration::from_secs(300))
        .send()
        .context("GET /v1/assets")?;

    let status = response.status();
    if status.as_u16() == 404 {
        return Err(VaultHttpError::new(
            404,
            format!("asset not found: {sha256} (source={source})"),
        )
        .into());
    }
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(VaultHttpError::new(
            status.as_u16(),
            format!("asset download failed ({status}): {}", truncate(&body, 300)),
        )
        .into());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    // Write to a temp file then rename, so a partial download (crash, cancel,
    // network drop) never leaves a truncated file at the destination path.
    let tmp = dest.with_extension("part");
    let mut file = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    std::io::copy(&mut response, &mut file).with_context(|| format!("write {}", tmp.display()))?;
    file.flush()?;
    std::fs::rename(&tmp, dest)
        .with_context(|| format!("rename {} -> {}", tmp.display(), dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_url_carries_q_limit_offset_and_account_only() {
        let url = export_url(ExportUrl {
            base_url: "http://127.0.0.1:8080/",
            q: "from:me",
            limit: 500,
            offset: 1000,
            account: " alice ",
        })
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:8080/v1/export/messages?q=from%3Ame&limit=500&offset=1000&account=alice"
        );
    }

    #[test]
    fn a_page_parses_without_an_ok_flag_and_a_failure_body_yields_its_sentence() {
        let page: ExportMessagesPage =
            serde_json::from_str(r#"{"items":[],"total":7,"limit":500,"offset":0}"#).unwrap();
        assert_eq!((page.items.len(), page.total), (0, 7));
        assert_eq!(
            error_sentence(r#"{"error":"limit exceeds maximum of 500"}"#, 400),
            "limit exceeds maximum of 500"
        );
        assert_eq!(error_sentence("<html>", 502), "<html>");
    }
}
