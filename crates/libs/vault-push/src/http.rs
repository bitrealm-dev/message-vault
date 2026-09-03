//! HTTP helpers for attachment upload and JSON Lines message import.
//!
//! JSON Lines means one JSON object per line. Calls are blocking so they can
//! run on worker threads without an async runtime. Login lives in
//! [`vault_http::auth_check`]; the session type here is
//! [`vault_http::HttpSession`].

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use vault_http::{VaultHttpError, looks_like_html, trim_base_url};

pub use vault_http::HttpSession;

#[derive(Debug, Deserialize)]
/// Vault reply after HEAD or PUT of one attachment.
pub struct AssetPutResponse {
    #[serde(default)]
    pub already_present: bool,
}

#[derive(Debug, Deserialize)]
/// Vault reply after posting one JSON Lines import batch.
pub struct ImportResponse {
    #[serde(default)]
    pub messages: u64,
    #[serde(default)]
    pub messages_appended: u64,
    #[serde(default)]
    pub messages_deduped: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub conversations: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub attachments: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub assets_copied: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub assets_missing: u64,
}

/// Arguments for uploading one attachment file.
pub struct AssetPutRequest<'a> {
    pub base_url: &'a str,
    pub key: &'a str,
    pub username: &'a str,
    pub source: &'a str,
    pub sha256: &'a str,
    pub file: &'a Path,
    pub mime: Option<&'a str>,
    /// Files larger than this use multipart upload (typically [`crate::run::MAX_PROXY_BODY_BYTES`]).
    pub multipart_threshold: usize,
}

/// Arguments for [`post_import`].
pub(crate) struct PostImportArgs<'a> {
    pub base_url: &'a str,
    pub key: &'a str,
    pub username: &'a str,
    pub source: &'a str,
    pub mode: &'a str,
    pub import_id: Option<i64>,
    pub contact_name_mode: &'a str,
    pub ndjson: Vec<u8>,
}

/// Arguments for [`complete_import`].
pub(crate) struct CompleteImportArgs<'a> {
    pub base_url: &'a str,
    pub key: &'a str,
    pub import_id: i64,
    pub ok: bool,
    /// Session outcome: `completed`, `completed_with_issues`, or `failed`.
    pub status: &'a str,
    pub message_count: u64,
    pub attachment_count: u64,
    pub bytes_uploaded: u64,
}

#[derive(Debug, Deserialize)]
struct UploadStartResponse {
    #[serde(default)]
    upload_id: Option<String>,
    #[serde(default)]
    part_size: Option<usize>,
    #[serde(default)]
    already_present: bool,
}

/// The vault's failure body: `{error}`.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
}

/// Parse a vault JSON response body, or bail with the server's error text.
///
/// A 2xx status is a success and the body is `T`. Anything else is a
/// [`VaultHttpError`] carrying the body's `error` sentence when it has one,
/// else `HTTP {status}: {body}`.
fn ok_json<T: DeserializeOwned>(status: reqwest::StatusCode, text: &str) -> Result<T> {
    if status.is_success() {
        return serde_json::from_str::<T>(text).map_err(|e| {
            VaultHttpError::new(
                status.as_u16(),
                format!("could not read the vault's answer ({e}): {text}"),
            )
            .into()
        });
    }
    let message = match serde_json::from_str::<ErrorBody>(text) {
        Ok(ErrorBody { error: Some(m) }) if !m.trim().is_empty() => m,
        _ => format!("HTTP {status}: {text}"),
    };
    Err(VaultHttpError::new(status.as_u16(), message).into())
}

/// True for HTTP 413 or a proxy HTML page that says the body was too large.
fn looks_like_payload_too_large(status: reqwest::StatusCode, body: &str) -> bool {
    status.as_u16() == 413
        || body.contains("413 Payload Too Large")
        || body.contains("413 Request Entity Too Large")
        || (looks_like_html(body) && body.to_ascii_lowercase().contains("payload too large"))
}

/// Human-readable 413 error that names the request kind and optional byte size.
fn payload_too_large_message(kind: &str, bytes: Option<usize>) -> String {
    let size = bytes
        .map(|n| format!(" (request was {n} bytes)"))
        .unwrap_or_default();
    format!(
        "{kind} rejected: HTTP 413 Payload Too Large{size}. \
         Cloudflare Free/Pro caps proxied uploads at ~100 MB. \
         vault-push chunks message imports under 64 MiB and large assets via multipart; \
         if this still fails, raise nginx client_max_body_size for /v1 (need ≥100m for 64 MiB parts) \
         or tunnel to vault :8080."
    )
}

/// Build `{base}/v1/assets/...` with extra path segments (percent-encoded) and
/// the `source=` / `account=` query pair every asset route takes.
fn asset_url(
    base_url: &str,
    segments: &[&str],
    source: &str,
    account: &str,
) -> Result<reqwest::Url> {
    let base = trim_base_url(base_url);
    let mut url = reqwest::Url::parse(base).with_context(|| format!("invalid vault URL {base}"))?;
    url.path_segments_mut()
        .map_err(|()| anyhow!("invalid vault URL {base}"))?
        .pop_if_empty()
        .extend(["v1", "assets"].into_iter().chain(segments.iter().copied()));
    url.query_pairs_mut()
        .append_pair("source", source)
        .append_pair("account", account);
    Ok(url)
}

/// Probe whether the vault already has this SHA-256 fingerprint.
///
/// Returns `Some` when present, `None` when missing (HTTP 404). Does not
/// transfer the file body. SHA-256 here is a short hex fingerprint of the
/// file bytes.
///
/// # Errors
///
/// Returns an error on auth failure or any non-404 HTTP error.
pub fn head_asset(
    http: &HttpSession,
    base_url: &str,
    key: &str,
    username: &str,
    source: &str,
    sha256: &str,
) -> Result<Option<AssetPutResponse>> {
    let url = asset_url(base_url, &[sha256], source, username)?;
    let response = http
        .request_url(Method::HEAD, url.clone(), key)
        .timeout(Duration::from_secs(15))
        .send()
        .with_context(|| format!("HEAD {url}"))?;
    let status = response.status();
    match status.as_u16() {
        404 => return Ok(None),
        401 => return Err(VaultHttpError::new(401, "invalid vault key").into()),
        403 => {
            return Err(VaultHttpError::new(403, "username does not match vault key").into());
        }
        _ => {}
    }
    if !status.is_success() {
        let text = response.text().unwrap_or_default();
        return Err(VaultHttpError::new(
            status.as_u16(),
            format!("asset HEAD failed (HTTP {status}): {text}"),
        )
        .into());
    }
    // A 2xx without a usable JSON body means the asset is there: plain HEAD
    // responders and proxies often send no body at all.
    let assumed_present = AssetPutResponse {
        already_present: true,
    };
    let text = response.text().unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Some(assumed_present));
    }
    let Ok(parsed) = serde_json::from_str::<AssetPutResponse>(&text) else {
        return Ok(Some(assumed_present));
    };
    // With a JSON body, only treat the asset as present when the server says so.
    if !parsed.already_present {
        return Ok(None);
    }
    Ok(Some(parsed))
}

/// Upload one attachment with PUT, or multipart when the file is large.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the vault rejects the
/// upload, or the response cannot be parsed.
pub fn put_asset(http: &HttpSession, request: AssetPutRequest<'_>) -> Result<AssetPutResponse> {
    let file_len = std::fs::metadata(request.file)
        .with_context(|| format!("stat {}", request.file.display()))?
        .len();
    if file_len > request.multipart_threshold as u64 {
        return put_asset_multipart(http, request, file_len);
    }

    let url = asset_url(
        request.base_url,
        &[request.sha256],
        request.source,
        request.username,
    )?;
    let bytes =
        std::fs::read(request.file).with_context(|| format!("read {}", request.file.display()))?;
    let content_type = request
        .mime
        .filter(|mime| !mime.is_empty())
        .unwrap_or("application/octet-stream");
    let response = http
        .request_url(Method::PUT, url.clone(), request.key)
        .timeout(Duration::from_secs(600))
        .header("Content-Type", content_type)
        .body(bytes)
        .send()
        .with_context(|| format!("PUT {url}"))?;
    let status = response.status();
    let text = response.text().context("read asset response")?;
    if looks_like_payload_too_large(status, &text) {
        return Err(VaultHttpError::new(
            413,
            payload_too_large_message("asset upload", Some(file_len as usize)),
        )
        .into());
    }
    ok_json::<AssetPutResponse>(status, &text)
}

/// Upload a large file as several parts, then complete the upload.
///
/// # Errors
///
/// Returns an error when start, part PUT, or complete fails, or the file
/// cannot be read.
fn put_asset_multipart(
    http: &HttpSession,
    request: AssetPutRequest<'_>,
    file_len: u64,
) -> Result<AssetPutResponse> {
    let start_url = asset_url(
        request.base_url,
        &[request.sha256, "uploads"],
        request.source,
        request.username,
    )?;
    let mut start_body = serde_json::json!({ "bytes": file_len });
    if let Some(mime) = request.mime.filter(|m| !m.is_empty()) {
        start_body["mime"] = serde_json::Value::String(mime.to_string());
    }
    let start_resp = http
        .request_url(Method::POST, start_url.clone(), request.key)
        .timeout(Duration::from_secs(30))
        .header("Content-Type", "application/json")
        .json(&start_body)
        .send()
        .with_context(|| format!("POST {start_url}"))?;
    let start_status = start_resp.status();
    let start_text = start_resp.text().context("read upload start response")?;
    if looks_like_payload_too_large(start_status, &start_text) {
        return Err(VaultHttpError::new(
            413,
            payload_too_large_message("asset upload start", None),
        )
        .into());
    }
    let started: UploadStartResponse = ok_json(start_status, &start_text)?;
    if started.already_present {
        return Ok(AssetPutResponse {
            already_present: true,
        });
    }
    let upload_id = started
        .upload_id
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("upload start missing upload_id"))?;
    let part_size = started
        .part_size
        .filter(|&n| n > 0)
        .ok_or_else(|| anyhow!("upload start missing part_size"))?;

    let abort = |upload_id: &str| {
        let Ok(abort_url) = asset_url(
            request.base_url,
            &[request.sha256, "uploads", upload_id],
            request.source,
            request.username,
        ) else {
            return;
        };
        let _ = http
            .request_url(Method::DELETE, abort_url, request.key)
            .timeout(Duration::from_secs(30))
            .send();
    };

    let mut file = std::fs::File::open(request.file)
        .with_context(|| format!("open {}", request.file.display()))?;
    let mut part: u32 = 1;
    let mut remaining = file_len;
    while remaining > 0 {
        let this_len = remaining.min(part_size as u64) as usize;
        let mut buf = vec![0u8; this_len];
        use std::io::Read;
        file.read_exact(&mut buf)
            .with_context(|| format!("read part {part} from {}", request.file.display()))?;
        let part_url = asset_url(
            request.base_url,
            &[
                request.sha256,
                "uploads",
                &upload_id,
                "parts",
                &part.to_string(),
            ],
            request.source,
            request.username,
        )?;
        let response = http
            .request_url(Method::PUT, part_url.clone(), request.key)
            .timeout(Duration::from_secs(600))
            .header("Content-Type", "application/octet-stream")
            .body(buf)
            .send()
            .with_context(|| format!("PUT {part_url}"))?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if looks_like_payload_too_large(status, &text) {
            abort(&upload_id);
            return Err(VaultHttpError::new(
                413,
                payload_too_large_message("asset upload part", Some(this_len)),
            )
            .into());
        }
        if !status.is_success() {
            abort(&upload_id);
            return Err(VaultHttpError::new(
                status.as_u16(),
                format!("asset part {part} failed (HTTP {status}): {text}"),
            )
            .into());
        }
        remaining -= this_len as u64;
        part += 1;
    }

    let complete_url = asset_url(
        request.base_url,
        &[request.sha256, "uploads", &upload_id, "complete"],
        request.source,
        request.username,
    )?;
    let response = http
        .request_url(Method::POST, complete_url.clone(), request.key)
        .timeout(Duration::from_secs(600))
        .send()
        .with_context(|| format!("POST {complete_url}"))?;
    let status = response.status();
    let text = response.text().context("read upload complete response")?;
    let completed = ok_json::<AssetPutResponse>(status, &text);
    if completed.is_err() {
        abort(&upload_id);
    }
    completed
}

/// POST one JSON Lines batch (`ndjson`) to `/v1/import`.
///
/// JSON Lines means one JSON object per line.
///
/// # Errors
///
/// Returns an error when the body is too large, the vault rejects the batch,
/// or the response cannot be parsed.
pub fn post_import(http: &HttpSession, args: PostImportArgs<'_>) -> Result<ImportResponse> {
    let PostImportArgs {
        base_url,
        key,
        username,
        source,
        mode,
        import_id,
        contact_name_mode,
        ndjson,
    } = args;
    let body_len = ndjson.len();
    if body_len > crate::run::MAX_PROXY_BODY_BYTES {
        return Err(
            VaultHttpError::new(413, payload_too_large_message("import", Some(body_len))).into(),
        );
    }
    let mut query: Vec<(&str, String)> = vec![
        ("source", source.to_string()),
        ("account", username.to_string()),
        ("mode", mode.to_string()),
        ("contact_name_mode", contact_name_mode.to_string()),
    ];
    if let Some(id) = import_id {
        query.push(("import_id", id.to_string()));
    }
    let response = http
        .vault_request(Method::POST, base_url, "/v1/import", key)
        .query(&query)
        .timeout(Duration::from_secs(600))
        .header("Content-Type", "application/jsonl")
        .body(ndjson)
        .send()
        .context("POST /v1/import")?;
    let status = response.status();
    let text = response.text().context("read import response")?;
    if looks_like_payload_too_large(status, &text) {
        return Err(
            VaultHttpError::new(413, payload_too_large_message("import", Some(body_len))).into(),
        );
    }
    ok_json::<ImportResponse>(status, &text)
}

#[derive(Debug, Deserialize)]
/// Body of the `/v1/imports` and `/v1/imports/{id}/complete` replies.
struct ImportSessionResponse {
    #[serde(default)]
    id: Option<i64>,
}

/// Start a vault import session. Returns `None` when the vault is older and
/// does not expose `/v1/imports` (push continues without message linking).
///
/// # Errors
///
/// Returns an error when the vault rejects the request (other than 404).
pub fn start_import(
    http: &HttpSession,
    base_url: &str,
    key: &str,
    username: &str,
    source: &str,
    mode: &str,
    tool: Option<&str>,
) -> Result<Option<i64>> {
    let mut body = serde_json::json!({
        "source": source,
        "mode": mode,
        "account": username,
    });
    if let Some(tool) = tool {
        body["tool"] = serde_json::Value::String(tool.to_string());
    }
    let response = http
        .vault_request(Method::POST, base_url, "/v1/imports", key)
        .timeout(Duration::from_secs(60))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .context("POST /v1/imports")?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    let text = response.text().context("read start-import response")?;
    let parsed: ImportSessionResponse = ok_json(status, &text)?;
    Ok(parsed.id)
}

/// Complete a vault import session. Soft-fails with `Ok(())` on 404.
///
/// # Errors
///
/// Returns an error when the vault rejects the request (other than 404).
pub fn complete_import(http: &HttpSession, args: CompleteImportArgs<'_>) -> Result<()> {
    let CompleteImportArgs {
        base_url,
        key,
        import_id,
        ok,
        status,
        message_count,
        attachment_count,
        bytes_uploaded,
    } = args;
    let body = serde_json::json!({
        "ok": ok,
        "status": status,
        "message_count": message_count,
        "attachment_count": attachment_count,
        "bytes_uploaded": bytes_uploaded,
    });
    let response = http
        .vault_request(
            Method::POST,
            base_url,
            &format!("/v1/imports/{import_id}/complete"),
            key,
        )
        .timeout(Duration::from_secs(60))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .with_context(|| format!("POST /v1/imports/{import_id}/complete"))?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Ok(());
    }
    let text = response.text().context("read complete-import response")?;
    let _: ImportSessionResponse = ok_json(status, &text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_too_large_mentions_64_mib_import_chunks() {
        let msg = payload_too_large_message("import", Some(10));
        assert!(
            msg.contains("imports under 64 MiB"),
            "413 help must name the import chunk size, got {msg}"
        );
    }

    #[test]
    fn asset_url_encodes_segments_and_query() {
        let url = asset_url(
            "http://127.0.0.1:8080/",
            &["abc123", "uploads", "up 1"],
            "sms backup",
            "alice",
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:8080/v1/assets/abc123/uploads/up%201?source=sms+backup&account=alice"
        );
    }

    #[test]
    fn ok_json_prefers_the_body_error_sentence() {
        let err = ok_json::<AssetPutResponse>(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"sha mismatch"}"#,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "sha mismatch");
    }

    #[test]
    fn ok_json_falls_back_to_status_and_body() {
        let err = ok_json::<AssetPutResponse>(reqwest::StatusCode::BAD_GATEWAY, "{}").unwrap_err();
        assert!(err.to_string().starts_with("HTTP 502"));
        let err = ok_json::<AssetPutResponse>(reqwest::StatusCode::BAD_GATEWAY, "gateway text")
            .unwrap_err();
        assert!(err.to_string().contains("gateway text"));
    }

    #[test]
    fn ok_json_trusts_the_status_not_a_flag() {
        let parsed = ok_json::<AssetPutResponse>(
            reqwest::StatusCode::OK,
            r#"{"sha256":"abc","assets_path":"a/b","already_present":true}"#,
        )
        .unwrap();
        assert!(parsed.already_present);
        assert!(
            ok_json::<AssetPutResponse>(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "{}").is_err()
        );
        // A success whose body cannot be read is a failure that names the problem.
        let err = ok_json::<AssetPutResponse>(reqwest::StatusCode::OK, "not json").unwrap_err();
        assert!(err.to_string().contains("not json"));
    }
}
