//! Blocking HTTP helpers for vault auth, asset upload, and JSONL import.

use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::AuthError;

#[derive(Debug, Clone)]
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
    #[serde(default)]
    account_ok: Option<bool>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssetPutResponse {
    pub ok: bool,
    #[serde(default)]
    pub already_present: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
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
pub struct HttpSession {
    client: Client,
}

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
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .pool_max_idle_per_host(16)
            .build()
            .context("build HTTP client")?;
        Ok(Self { client })
    }
}

fn looks_like_html(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with("<!DOCTYPE") || t.starts_with("<html") || t.starts_with("<HTML")
}

fn looks_like_payload_too_large(status: reqwest::StatusCode, body: &str) -> bool {
    status.as_u16() == 413
        || body.contains("413 Payload Too Large")
        || body.contains("413 Request Entity Too Large")
        || (looks_like_html(body) && body.to_ascii_lowercase().contains("payload too large"))
}

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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

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
    pub fn auth_check(
        &self,
        base_url: &str,
        key: &str,
        username: &str,
    ) -> std::result::Result<AuthInfo, AuthError> {
        // Validate the token first (no account=). A wrong User ID used to return
        // HTTP 403 "username does not match vault key", which looked like a bad token.
        let base = base_url.trim().trim_end_matches('/');
        if let Err(error) = reqwest::Url::parse(base) {
            return Err(AuthError::InvalidUrl {
                url: base.to_string(),
                detail: error.to_string(),
            });
        }
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
            return Err(AuthError::InvalidKey);
        }
        if !status.is_success() {
            return Err(classify_auth_http_status(status_code, text));
        }
        let parsed: AuthCheckResponse = serde_json::from_str(&text).map_err(|_| {
            AuthError::BadJson {
                url: url.clone(),
                status: status_code,
                snippet: truncate(&text, 200),
            }
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

    /// Probe whether the vault already has this SHA. Returns `Some` when present,
    /// `None` when missing (HTTP 404). Does not transfer the file body.
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
        if status.as_u16() == 404 {
            return Ok(None);
        }
        if status.as_u16() == 401 {
            bail!("invalid vault key");
        }
        if status.as_u16() == 403 {
            bail!("username does not match vault key");
        }
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            bail!("asset HEAD failed (HTTP {status}): {text}");
        }
        // Prefer JSON body when present; some servers omit HEAD bodies.
        let text = response.text().unwrap_or_default();
        if text.trim().is_empty() {
            return Ok(Some(AssetPutResponse {
                ok: true,
                already_present: true,
                error: None,
            }));
        }
        let parsed: AssetPutResponse = serde_json::from_str(&text).unwrap_or(AssetPutResponse {
            ok: true,
            already_present: true,
            error: None,
        });
        Ok(Some(parsed))
    }

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
        let mut req = self
            .client
            .put(&url)
            .timeout(Duration::from_secs(600))
            .header("Authorization", format!("Bearer {}", request.key.trim()))
            .body(bytes);
        if let Some(mime) = request.mime.filter(|m| !m.is_empty()) {
            req = req.header("Content-Type", mime);
        } else {
            req = req.header("Content-Type", "application/octet-stream");
        }
        let response = req.send().with_context(|| format!("PUT {url}"))?;
        let status = response.status();
        let text = response.text().context("read asset response")?;
        if looks_like_payload_too_large(status, &text) {
            bail!("{}", payload_too_large_message("asset upload", Some(file_len as usize)));
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

    pub fn post_import(
        &self,
        base_url: &str,
        key: &str,
        username: &str,
        source: &str,
        mode: &str,
        ndjson: Vec<u8>,
    ) -> Result<ImportResponse> {
        let body_len = ndjson.len();
        if body_len > crate::run::MAX_PROXY_BODY_BYTES {
            bail!(
                "{}",
                payload_too_large_message("import", Some(body_len))
            );
        }
        let base = base_url.trim_end_matches('/');
        let url = format!(
            "{base}/v1/import?source={}&account={}&mode={}",
            encode(source),
            encode(username),
            encode(mode)
        );
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
}

fn classify_auth_transport_error(url: &str, error: reqwest::Error) -> AuthError {
    let detail = error.to_string();
    if error.is_timeout() {
        AuthError::Timeout {
            url: url.to_string(),
            detail,
        }
    } else if error.is_builder() {
        AuthError::InvalidUrl {
            url: url.to_string(),
            detail,
        }
    } else if error.is_connect() || error.is_request() {
        AuthError::Network {
            url: url.to_string(),
            detail,
        }
    } else {
        AuthError::Network {
            url: url.to_string(),
            detail,
        }
    }
}

fn classify_auth_http_status(status: u16, body: String) -> AuthError {
    match status {
        403 => AuthError::Forbidden { status, body },
        404 => AuthError::ApiNotFound { status, body },
        429 => AuthError::RateLimited { status, body },
        500..=599 => AuthError::ServerError { status, body },
        _ => AuthError::HttpStatus { status, body },
    }
}

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
                if attempt > max_retries {
                    return Err(e);
                }
                thread::sleep(Duration::from_secs(u64::from(attempt)));
            }
        }
    }
}
