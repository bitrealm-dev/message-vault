//! HTTP helpers for login, attachment upload, and JSON Lines message import.
//!
//! JSON Lines means one JSON object per line. Calls are blocking so they can
//! run on worker threads without an async runtime.

use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::AuthError;

#[derive(Debug, Clone)]
/// Account id and username returned by a successful `GET /v1/auth/check`.
pub struct AuthInfo {
    pub account_id: String,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthCheckResponse {
    ok: bool,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    username: Option<String>,
    /// Present on some vault versions; accepted for wire compat, unused here.
    #[serde(default)]
    #[allow(dead_code)]
    account_ok: Option<bool>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
/// Vault reply after HEAD or PUT of one attachment.
pub struct AssetPutResponse {
    pub ok: bool,
    #[serde(default)]
    pub already_present: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
/// Vault reply after posting one JSON Lines import batch.
pub struct ImportResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub messages: u64,
    #[serde(default)]
    pub messages_appended: u64,
    #[serde(default)]
    #[allow(dead_code)]
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

#[derive(Clone)]
/// Blocking HTTP client used for login, attachment upload, and import.
pub struct HttpSession {
    client: Client,
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

#[derive(Debug, Deserialize)]
struct UploadStartResponse {
    ok: bool,
    #[serde(default)]
    upload_id: Option<String>,
    #[serde(default)]
    part_size: Option<usize>,
    #[serde(default)]
    already_present: bool,
    #[serde(default)]
    error: Option<String>,
}

impl HttpSession {
    /// Blocking HTTP client with a connection pool for worker threads.
    ///
    /// # Errors
    ///
    /// Returns an error when the reqwest client cannot be built.
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .pool_max_idle_per_host(16)
            .build()
            .context("build HTTP client")?;
        Ok(Self { client })
    }
}

/// True when the body looks like an HTML error page instead of JSON.
fn looks_like_html(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with("<!DOCTYPE") || t.starts_with("<html") || t.starts_with("<HTML")
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
         Cloudflare Free/Pro caps proxied uploads at ~100 MB. \
         vault-push chunks message imports under 8 MiB and large assets via multipart; \
         if this still fails, raise nginx client_max_body_size for /v1 (need ≥100m for 64 MiB parts) \
         or tunnel to vault :8080."
    )
}

/// Copy `s`, cutting it to `max` bytes and adding an ellipsis when longer.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

/// Percent-encode a query value so it is safe inside a vault URL.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl HttpSession {
    /// Call `GET /v1/auth/check` and return the account id on success.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the URL is invalid, the host is unreachable,
    /// or the key is rejected.
    pub fn auth_check(
        &self,
        base_url: &str,
        key: &str,
        username: &str,
    ) -> std::result::Result<AuthInfo, AuthError> {
        // Validate the token first (no account=). A wrong User ID used to return
        // HTTP 403 "username does not match vault key", which looked like a bad token.
        let base = base_url.trim().trim_end_matches('/');
        let parsed_base = match reqwest::Url::parse(base) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Err(AuthError::InvalidUrl {
                    url: base.to_string(),
                    detail: error.to_string(),
                });
            }
        };
        let url = format!("{base}/v1/auth/check");
        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(15))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .send()
            .map_err(|error| classify_auth_transport_error(&url, error))?;
        let status = response.status();
        let status_code = status.as_u16();
        // After redirects, reqwest reports the final URL. http→https drops Authorization.
        let final_url = response.url().clone();
        let text = response.text().map_err(|error| AuthError::ReadResponse {
            detail: error.to_string(),
        })?;
        if looks_like_html(&text) {
            return Err(AuthError::WrongHostHtml {
                url,
                status: status_code,
            });
        }
        if status_code == 401 {
            return Err(classify_unauthorized(base, &parsed_base, &final_url));
        }
        if !status.is_success() {
            return Err(classify_auth_http_status(status_code, text));
        }
        let parsed: AuthCheckResponse =
            serde_json::from_str(&text).map_err(|_| AuthError::BadJson {
                url: url.clone(),
                status: status_code,
                snippet: truncate(&text, 200),
            })?;
        if !parsed.ok {
            return Err(AuthError::Rejected {
                message: parsed.error.unwrap_or(text),
            });
        }
        let account_id = parsed
            .account_id
            .filter(|s| !s.is_empty())
            .ok_or(AuthError::MissingAccountId)?;
        // Token is authoritative. Ignore a wrong Username field — callers should
        // prefer `AuthInfo.username` for later account= query params.
        let _ = username;
        Ok(AuthInfo {
            account_id,
            username: parsed.username,
        })
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
        &self,
        base_url: &str,
        key: &str,
        username: &str,
        source: &str,
        sha256: &str,
    ) -> Result<Option<AssetPutResponse>> {
        let base = base_url.trim_end_matches('/');
        let url = format!(
            "{base}/v1/assets/{}?source={}&account={}",
            encode(sha256),
            encode(source),
            encode(username)
        );
        let response = self
            .client
            .head(&url)
            .timeout(Duration::from_secs(15))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .send()
            .with_context(|| format!("HEAD {url}"))?;
        let status = response.status();
        match status.as_u16() {
            404 => return Ok(None),
            401 => bail!("invalid vault key"),
            403 => bail!("username does not match vault key"),
            _ => {}
        }
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            bail!("asset HEAD failed (HTTP {status}): {text}");
        }
        // A 2xx without a usable JSON body means the asset is there: plain HEAD
        // responders and proxies often send no body at all.
        let assumed_present = AssetPutResponse {
            ok: true,
            already_present: true,
            error: None,
        };
        let text = response.text().unwrap_or_default();
        if text.trim().is_empty() {
            return Ok(Some(assumed_present));
        }
        let Ok(parsed) = serde_json::from_str::<AssetPutResponse>(&text) else {
            return Ok(Some(assumed_present));
        };
        // With a JSON body, only treat the asset as present when the server says so.
        if !parsed.ok || !parsed.already_present {
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
    pub fn put_asset(&self, request: AssetPutRequest<'_>) -> Result<AssetPutResponse> {
        let file_len = std::fs::metadata(request.file)
            .with_context(|| format!("stat {}", request.file.display()))?
            .len();
        if file_len > request.multipart_threshold as u64 {
            return self.put_asset_multipart(request, file_len);
        }

        let base = request.base_url.trim_end_matches('/');
        let url = format!(
            "{base}/v1/assets/{}?source={}&account={}",
            encode(request.sha256),
            encode(request.source),
            encode(request.username)
        );
        let bytes = std::fs::read(request.file)
            .with_context(|| format!("read {}", request.file.display()))?;
        let content_type = request
            .mime
            .filter(|mime| !mime.is_empty())
            .unwrap_or("application/octet-stream");
        let response = self
            .client
            .put(&url)
            .timeout(Duration::from_secs(600))
            .header("Authorization", format!("Bearer {}", request.key.trim()))
            .header("Content-Type", content_type)
            .body(bytes)
            .send()
            .with_context(|| format!("PUT {url}"))?;
        let status = response.status();
        let text = response.text().context("read asset response")?;
        if looks_like_payload_too_large(status, &text) {
            bail!(
                "{}",
                payload_too_large_message("asset upload", Some(file_len as usize))
            );
        }
        let parsed: AssetPutResponse = serde_json::from_str(&text).unwrap_or(AssetPutResponse {
            ok: false,
            already_present: false,
            error: Some(text.clone()),
        });
        if !status.is_success() || !parsed.ok {
            bail!(
                "{}",
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}"))
            );
        }
        Ok(parsed)
    }

    /// Upload a large file as several parts, then complete the upload.
    ///
    /// # Errors
    ///
    /// Returns an error when start, part PUT, or complete fails, or the file
    /// cannot be read.
    fn put_asset_multipart(
        &self,
        request: AssetPutRequest<'_>,
        file_len: u64,
    ) -> Result<AssetPutResponse> {
        let base = request.base_url.trim_end_matches('/');
        let qs = format!(
            "source={}&account={}",
            encode(request.source),
            encode(request.username)
        );
        let start_url = format!("{base}/v1/assets/{}/uploads?{qs}", encode(request.sha256));
        let mut start_body = serde_json::json!({ "bytes": file_len });
        if let Some(mime) = request.mime.filter(|m| !m.is_empty()) {
            start_body["mime"] = serde_json::Value::String(mime.to_string());
        }
        let start_resp = self
            .client
            .post(&start_url)
            .timeout(Duration::from_secs(30))
            .header("Authorization", format!("Bearer {}", request.key.trim()))
            .header("Content-Type", "application/json")
            .json(&start_body)
            .send()
            .with_context(|| format!("POST {start_url}"))?;
        let start_status = start_resp.status();
        let start_text = start_resp.text().context("read upload start response")?;
        if looks_like_payload_too_large(start_status, &start_text) {
            bail!("{}", payload_too_large_message("asset upload start", None));
        }
        let started: UploadStartResponse =
            serde_json::from_str(&start_text).with_context(|| {
                format!(
                    "parse upload start JSON (HTTP {start_status}): {}",
                    truncate(&start_text, 200)
                )
            })?;
        if !start_status.is_success() || !started.ok {
            bail!(
                "{}",
                started
                    .error
                    .unwrap_or_else(|| format!("HTTP {start_status}: {start_text}"))
            );
        }
        if started.already_present {
            return Ok(AssetPutResponse {
                ok: true,
                already_present: true,
                error: None,
            });
        }
        let upload_id = started
            .upload_id
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("upload start missing upload_id"))?;
        let part_size = started
            .part_size
            .filter(|&n| n > 0)
            .ok_or_else(|| anyhow::anyhow!("upload start missing part_size"))?;

        let abort = |session: &HttpSession, upload_id: &str| {
            let abort_url = format!(
                "{base}/v1/assets/{}/uploads/{}?{qs}",
                encode(request.sha256),
                encode(upload_id)
            );
            let _ = session
                .client
                .delete(&abort_url)
                .timeout(Duration::from_secs(30))
                .header("Authorization", format!("Bearer {}", request.key.trim()))
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
            let part_url = format!(
                "{base}/v1/assets/{}/uploads/{}/parts/{part}?{qs}",
                encode(request.sha256),
                encode(&upload_id)
            );
            let response = self
                .client
                .put(&part_url)
                .timeout(Duration::from_secs(600))
                .header("Authorization", format!("Bearer {}", request.key.trim()))
                .header("Content-Type", "application/octet-stream")
                .body(buf)
                .send()
                .with_context(|| format!("PUT {part_url}"))?;
            let status = response.status();
            let text = response.text().unwrap_or_default();
            if looks_like_payload_too_large(status, &text) {
                abort(self, &upload_id);
                bail!(
                    "{}",
                    payload_too_large_message("asset upload part", Some(this_len))
                );
            }
            if !status.is_success() {
                abort(self, &upload_id);
                bail!("asset part {part} failed (HTTP {status}): {text}");
            }
            remaining -= this_len as u64;
            part += 1;
        }

        let complete_url = format!(
            "{base}/v1/assets/{}/uploads/{}/complete?{qs}",
            encode(request.sha256),
            encode(&upload_id)
        );
        let response = self
            .client
            .post(&complete_url)
            .timeout(Duration::from_secs(600))
            .header("Authorization", format!("Bearer {}", request.key.trim()))
            .send()
            .with_context(|| format!("POST {complete_url}"))?;
        let status = response.status();
        let text = response.text().context("read upload complete response")?;
        let parsed: AssetPutResponse = serde_json::from_str(&text).unwrap_or(AssetPutResponse {
            ok: false,
            already_present: false,
            error: Some(text.clone()),
        });
        if !status.is_success() || !parsed.ok {
            abort(self, &upload_id);
            bail!(
                "{}",
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}"))
            );
        }
        Ok(parsed)
    }

    /// POST one JSON Lines batch (`ndjson`) to `/v1/import`.
    ///
    /// JSON Lines means one JSON object per line.
    ///
    /// # Errors
    ///
    /// Returns an error when the body is too large, the vault rejects the batch,
    /// or the response cannot be parsed.
    #[allow(clippy::too_many_arguments)]
    pub fn post_import(
        &self,
        base_url: &str,
        key: &str,
        username: &str,
        source: &str,
        mode: &str,
        import_id: Option<i64>,
        contact_name_mode: &str,
        ndjson: Vec<u8>,
    ) -> Result<ImportResponse> {
        let body_len = ndjson.len();
        if body_len > crate::run::MAX_PROXY_BODY_BYTES {
            bail!("{}", payload_too_large_message("import", Some(body_len)));
        }
        let base = base_url.trim_end_matches('/');
        let mut url = format!(
            "{base}/v1/import?source={}&account={}&mode={}&contact_name_mode={}",
            encode(source),
            encode(username),
            encode(mode),
            encode(contact_name_mode)
        );
        if let Some(id) = import_id {
            url.push_str(&format!("&import_id={id}"));
        }
        let response = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(600))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .header("Content-Type", "application/jsonl")
            .body(ndjson)
            .send()
            .with_context(|| format!("POST {url}"))?;
        let status = response.status();
        let text = response.text().context("read import response")?;
        if looks_like_payload_too_large(status, &text) {
            bail!("{}", payload_too_large_message("import", Some(body_len)));
        }
        let parsed: ImportResponse = serde_json::from_str(&text).unwrap_or(ImportResponse {
            ok: false,
            error: Some(text.clone()),
            messages: 0,
            messages_appended: 0,
            messages_deduped: 0,
            conversations: 0,
            attachments: 0,
            assets_copied: 0,
            assets_missing: 0,
        });
        if !status.is_success() || !parsed.ok {
            bail!(
                "{}",
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}"))
            );
        }
        Ok(parsed)
    }

    /// Start a vault import session. Returns `None` when the vault is older and
    /// does not expose `/v1/imports` (push continues without message linking).
    ///
    /// # Errors
    ///
    /// Returns an error when the vault rejects the request (other than 404).
    pub fn start_import(
        &self,
        base_url: &str,
        key: &str,
        username: &str,
        source: &str,
        mode: &str,
        tool: Option<&str>,
    ) -> Result<Option<i64>> {
        #[derive(Deserialize)]
        struct Resp {
            ok: bool,
            #[serde(default)]
            id: Option<i64>,
            #[serde(default)]
            error: Option<String>,
        }
        let base = base_url.trim_end_matches('/');
        let url = format!("{base}/v1/imports");
        let mut body = serde_json::json!({
            "source": source,
            "mode": mode,
            "account": username,
        });
        if let Some(tool) = tool {
            body["tool"] = serde_json::Value::String(tool.to_string());
        }
        let response = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(60))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .with_context(|| format!("POST {url}"))?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }
        let text = response.text().context("read start-import response")?;
        let parsed: Resp = serde_json::from_str(&text).unwrap_or(Resp {
            ok: false,
            id: None,
            error: Some(text.clone()),
        });
        if !status.is_success() || !parsed.ok {
            bail!(
                "{}",
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}"))
            );
        }
        Ok(parsed.id)
    }

    /// Complete a vault import session. Soft-fails with `Ok(())` on 404.
    ///
    /// # Errors
    ///
    /// Returns an error when the vault rejects the request (other than 404).
    #[allow(clippy::too_many_arguments)]
    pub fn complete_import(
        &self,
        base_url: &str,
        key: &str,
        import_id: i64,
        ok: bool,
        message_count: u64,
        attachment_count: u64,
        bytes_uploaded: u64,
    ) -> Result<()> {
        #[derive(Deserialize)]
        struct Resp {
            ok: bool,
            #[serde(default)]
            error: Option<String>,
        }
        let base = base_url.trim_end_matches('/');
        let url = format!("{base}/v1/imports/{import_id}/complete");
        let body = serde_json::json!({
            "ok": ok,
            "message_count": message_count,
            "attachment_count": attachment_count,
            "bytes_uploaded": bytes_uploaded,
        });
        let response = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(60))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .with_context(|| format!("POST {url}"))?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Ok(());
        }
        let text = response.text().context("read complete-import response")?;
        let parsed: Resp = serde_json::from_str(&text).unwrap_or(Resp {
            ok: false,
            error: Some(text.clone()),
        });
        if !status.is_success() || !parsed.ok {
            bail!(
                "{}",
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}"))
            );
        }
        Ok(())
    }
}

/// Map HTTP 401. When `http://` was redirected to `https://`, the API key was
/// dropped with the Authorization header — tell the user to use https.
fn classify_unauthorized(
    requested_base: &str,
    requested_url: &reqwest::Url,
    final_url: &reqwest::Url,
) -> AuthError {
    if requested_url.scheme() == "http" && final_url.scheme() == "https" {
        AuthError::HttpsRequired {
            url: requested_base.to_string(),
        }
    } else {
        AuthError::InvalidKey
    }
}

/// Map a reqwest transport failure (timeout, connect, TLS) onto [`AuthError`].
fn classify_auth_transport_error(url: &str, error: reqwest::Error) -> AuthError {
    let url = url.to_string();
    let detail = error.to_string();
    if error.is_timeout() {
        AuthError::Timeout { url, detail }
    } else if error.is_builder() {
        AuthError::InvalidUrl { url, detail }
    } else {
        // Connection refused, DNS failure, and anything else unrecognized all
        // mean "could not reach the vault".
        AuthError::Network { url, detail }
    }
}

/// Map a non-success HTTP status from `/v1/auth/check` onto [`AuthError`].
fn classify_auth_http_status(status: u16, body: String) -> AuthError {
    match status {
        403 => AuthError::Forbidden { status, body },
        404 => AuthError::ApiNotFound { status, body },
        429 => AuthError::RateLimited { status, body },
        500..=599 => AuthError::ServerError { status, body },
        _ => AuthError::HttpStatus { status, body },
    }
}

/// Build a session and call [`HttpSession::auth_check`].
///
/// # Errors
///
/// Returns [`AuthError`] when the client cannot be built or login fails.
pub fn auth_check(
    base_url: &str,
    key: &str,
    username: &str,
) -> std::result::Result<AuthInfo, AuthError> {
    let session = HttpSession::new().map_err(|error| AuthError::Client {
        detail: format!("{error:#}"),
    })?;
    session.auth_check(base_url, key, username)
}

/// Returns true when an error is likely to succeed on retry (network, timeout, 5xx).
/// Permanent errors (4xx auth, 413, malformed input) should not be retried.
fn is_transient_error(error: &anyhow::Error) -> bool {
    let msg = error.to_string().to_ascii_lowercase();
    // Never retry auth failures.
    if msg.contains("invalid vault key")
        || msg.contains("username does not match")
        || msg.contains("401")
        || msg.contains("403")
    {
        return false;
    }
    // Never retry payload-too-large (413) — it will never succeed.
    if msg.contains("413") || msg.contains("payload too large") {
        return false;
    }
    // Never retry path-not-found or missing-file errors.
    if msg.contains("no such file") || (msg.contains("not found") && msg.contains("404")) {
        return false;
    }
    // Everything else is worth retrying: connection resets, timeouts, server
    // errors (5xx), and failures this code does not recognize.
    true
}

/// Run `op` again on transient failures, with backoff, up to `max_retries` extra tries.
///
/// # Errors
///
/// Returns the last error from `op` when retries are exhausted or the error is
/// permanent (auth, 413, missing file).
pub fn with_retries<T, F>(max_retries: u32, mut op: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if attempt > max_retries || !is_transient_error(&e) {
                    return Err(e);
                }
                // Exponential backoff with jitter.
                let base_ms = 500u64 * 2u64.saturating_pow(attempt.saturating_sub(1));
                let jitter_ms = (base_ms / 4).min(5000);
                let wait_ms = base_ms + (jitter_ms / 2) + (jitter_ms as f64 * rand_factor()) as u64;
                thread::sleep(Duration::from_millis(wait_ms.min(30_000)));
            }
        }
    }
}

/// Deterministic pseudo-random factor in [0.0, 1.0) for retry jitter.
fn rand_factor() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1000) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_http_to_https_redirect_asks_for_https() {
        let requested = reqwest::Url::parse("http://app.bitrealm.io").unwrap();
        let final_url = reqwest::Url::parse("https://app.bitrealm.io/v1/auth/check").unwrap();
        let err = classify_unauthorized("http://app.bitrealm.io", &requested, &final_url);
        assert_eq!(err.kind(), "https_required");
        assert!(err.user_message().contains("https://"));
        assert!(err.detail().contains("Authorization"));
    }

    #[test]
    fn unauthorized_same_scheme_is_invalid_key() {
        let requested = reqwest::Url::parse("https://app.bitrealm.io").unwrap();
        let final_url = reqwest::Url::parse("https://app.bitrealm.io/v1/auth/check").unwrap();
        let err = classify_unauthorized("https://app.bitrealm.io", &requested, &final_url);
        assert_eq!(err.kind(), "invalid_key");
    }

    #[test]
    fn unauthorized_local_http_is_invalid_key() {
        let requested = reqwest::Url::parse("http://127.0.0.1:8080").unwrap();
        let final_url = reqwest::Url::parse("http://127.0.0.1:8080/v1/auth/check").unwrap();
        let err = classify_unauthorized("http://127.0.0.1:8080", &requested, &final_url);
        assert_eq!(err.kind(), "invalid_key");
    }
}
