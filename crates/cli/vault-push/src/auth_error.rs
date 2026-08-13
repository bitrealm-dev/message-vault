//! Typed login failures from `GET /v1/auth/check`.
//!
//! Each variant has a stable `kind()` string for tests and a short
//! `user_message()` for the desktop app banner.

use std::fmt;

/// Failure from `GET /v1/auth/check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    InvalidUrl {
        url: String,
        detail: String,
    },
    Client {
        detail: String,
    },
    Network {
        url: String,
        detail: String,
    },
    Timeout {
        url: String,
        detail: String,
    },
    ReadResponse {
        detail: String,
    },
    WrongHostHtml {
        url: String,
        status: u16,
    },
    /// Requested `http://…` but the vault redirected to `https://…` (auth header dropped).
    HttpsRequired {
        url: String,
    },
    InvalidKey,
    Forbidden {
        status: u16,
        body: String,
    },
    ApiNotFound {
        status: u16,
        body: String,
    },
    RateLimited {
        status: u16,
        body: String,
    },
    ServerError {
        status: u16,
        body: String,
    },
    HttpStatus {
        status: u16,
        body: String,
    },
    BadJson {
        url: String,
        status: u16,
        snippet: String,
    },
    Rejected {
        message: String,
    },
    MissingAccountId,
}

impl AuthError {
    /// Stable machine-readable kind for tests and mapping.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidUrl { .. } => "invalid_url",
            Self::Client { .. } => "client",
            Self::Network { .. } => "network",
            Self::Timeout { .. } => "timeout",
            Self::ReadResponse { .. } => "read_response",
            Self::WrongHostHtml { .. } => "wrong_host",
            Self::HttpsRequired { .. } => "https_required",
            Self::InvalidKey => "invalid_key",
            Self::Forbidden { .. } => "forbidden",
            Self::ApiNotFound { .. } => "api_not_found",
            Self::RateLimited { .. } => "rate_limited",
            Self::ServerError { .. } => "server_error",
            Self::HttpStatus { .. } => "http_status",
            Self::BadJson { .. } => "bad_json",
            Self::Rejected { .. } => "rejected",
            Self::MissingAccountId => "missing_account",
        }
    }

    /// Short message for the GUI error banner (no transport internals).
    pub fn user_message(&self) -> String {
        match self {
            Self::InvalidUrl { .. } => {
                "This Vault URL is not valid. Enter the full URL, including `https://`.".into()
            }
            Self::Timeout { .. } => {
                "The vault did not respond within 15 seconds. Check the URL and try again.".into()
            }
            Self::Network { .. } => {
                "Could not connect to the vault. Check the URL, your network connection, and whether the vault is running.".into()
            }
            Self::Client { .. } => {
                "Could not start a secure connection to the vault. Restart the app and try again."
                    .into()
            }
            Self::ReadResponse { .. } => {
                "Connected to the vault, but could not read its response. Try again.".into()
            }
            Self::WrongHostHtml { .. } => {
                "This URL points to the Message Vault website, not the vault API. Use the vault server URL (the TLS vault host or port 8080, not port 3000).".into()
            }
            Self::HttpsRequired { .. } => {
                "This Vault requires https:// but http:// was specified.".into()
            }
            Self::InvalidKey => {
                "This API key is not valid for the specified vault. Paste a valid key and try again."
                    .into()
            }
            Self::Forbidden { .. } => {
                "This API key does not have permission to access the specified vault.".into()
            }
            Self::ApiNotFound { .. } => {
                "The vault API was not found at this URL. Enter the vault’s base URL without `/v1/auth/check`.".into()
            }
            Self::RateLimited { .. } => {
                "Too many verification attempts. Wait a moment, then try again.".into()
            }
            Self::ServerError { status, .. } => {
                format!(
                    "The vault could not verify your credentials right now (HTTP {status}). Try again later."
                )
            }
            Self::HttpStatus { status, .. } => {
                format!(
                    "The vault rejected the verification request (HTTP {status}). Open the Log tab for details."
                )
            }
            Self::BadJson { .. } => {
                "Connected to the vault, but its response was not recognized. Confirm that the vault server is compatible with this app.".into()
            }
            Self::Rejected { .. } => {
                "The vault rejected these credentials. Check the Vault URL and API key.".into()
            }
            Self::MissingAccountId => {
                "The API key was accepted, but the vault did not return an account. Contact your vault administrator.".into()
            }
        }
    }

    /// Technical detail for the Log tab / CLI.
    pub fn detail(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl { url, detail } => {
                write!(f, "invalid vault URL {url}: {detail}")
            }
            Self::Client { detail } => write!(f, "build HTTP client: {detail}"),
            Self::Network { url, detail } | Self::Timeout { url, detail } => {
                write!(f, "GET {url}: {detail}")
            }
            Self::ReadResponse { detail } => write!(f, "read auth/check body: {detail}"),
            Self::WrongHostHtml { url, status } => write!(
                f,
                "auth/check returned HTML from {url} (HTTP {status}). \
                 Vault URL must point at the vault host (TLS site or port 8080), \
                 not the Next.js browse UI alone (port 3000)"
            ),
            Self::HttpsRequired { url } => write!(
                f,
                "vault URL {url} redirected from http to https; \
                 use https:// so the API key is sent (http redirects drop Authorization)"
            ),
            Self::InvalidKey => write!(f, "invalid vault key"),
            Self::Forbidden { status, body }
            | Self::ApiNotFound { status, body }
            | Self::RateLimited { status, body }
            | Self::ServerError { status, body }
            | Self::HttpStatus { status, body } => {
                write!(f, "auth/check failed (HTTP {status}): {body}")
            }
            Self::BadJson {
                url,
                status,
                snippet,
            } => write!(
                f,
                "parse auth/check JSON from {url} (HTTP {status}): {snippet}"
            ),
            Self::Rejected { message } => write!(f, "auth/check rejected: {message}"),
            Self::MissingAccountId => write!(f, "auth/check did not return account_id"),
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_user_messages_cover_all_variants() {
        let cases: Vec<(AuthError, &str)> = vec![
            (
                AuthError::InvalidUrl {
                    url: "notaurl".into(),
                    detail: "relative URL without a base".into(),
                },
                "invalid_url",
            ),
            (
                AuthError::Client {
                    detail: "tls".into(),
                },
                "client",
            ),
            (
                AuthError::Network {
                    url: "https://v/v1/auth/check".into(),
                    detail: "dns".into(),
                },
                "network",
            ),
            (
                AuthError::Timeout {
                    url: "https://v/v1/auth/check".into(),
                    detail: "timed out".into(),
                },
                "timeout",
            ),
            (
                AuthError::ReadResponse {
                    detail: "reset".into(),
                },
                "read_response",
            ),
            (
                AuthError::WrongHostHtml {
                    url: "https://app/v1/auth/check".into(),
                    status: 200,
                },
                "wrong_host",
            ),
            (
                AuthError::HttpsRequired {
                    url: "http://app.example".into(),
                },
                "https_required",
            ),
            (AuthError::InvalidKey, "invalid_key"),
            (
                AuthError::Forbidden {
                    status: 403,
                    body: "nope".into(),
                },
                "forbidden",
            ),
            (
                AuthError::ApiNotFound {
                    status: 404,
                    body: "missing".into(),
                },
                "api_not_found",
            ),
            (
                AuthError::RateLimited {
                    status: 429,
                    body: "slow down".into(),
                },
                "rate_limited",
            ),
            (
                AuthError::ServerError {
                    status: 503,
                    body: "busy".into(),
                },
                "server_error",
            ),
            (
                AuthError::HttpStatus {
                    status: 418,
                    body: "teapot".into(),
                },
                "http_status",
            ),
            (
                AuthError::BadJson {
                    url: "https://v/v1/auth/check".into(),
                    status: 200,
                    snippet: "{".into(),
                },
                "bad_json",
            ),
            (
                AuthError::Rejected {
                    message: "bad token".into(),
                },
                "rejected",
            ),
            (AuthError::MissingAccountId, "missing_account"),
        ];

        for (error, kind) in cases {
            assert_eq!(error.kind(), kind);
            let user = error.user_message();
            assert!(!user.is_empty(), "{kind} user message empty");
            // Banner copy must stay free of transport / body dumps.
            assert!(
                !user.contains("dns")
                    && !user.contains("teapot")
                    && !user.contains("busy")
                    && !user.contains("nope")
                    && !user.contains("missing")
                    && !user.contains("slow down")
                    && !user.contains("bad token")
                    && !user.contains("relative URL")
                    && !user.contains("GET https"),
                "{kind} user message leaked detail: {user}"
            );
            let detail = error.detail();
            assert!(!detail.is_empty(), "{kind} detail empty");
            match &error {
                AuthError::InvalidKey | AuthError::MissingAccountId => {}
                AuthError::HttpsRequired { url } => {
                    assert!(detail.contains(url));
                    assert!(detail.contains("https"));
                    assert!(user.contains("https://"));
                }
                AuthError::WrongHostHtml { .. } => {
                    assert!(detail.contains("HTML") || detail.contains("html"));
                }
                AuthError::ServerError { status, body, .. }
                | AuthError::HttpStatus { status, body, .. }
                | AuthError::Forbidden { status, body, .. }
                | AuthError::ApiNotFound { status, body, .. }
                | AuthError::RateLimited { status, body, .. } => {
                    assert!(detail.contains(&status.to_string()));
                    assert!(detail.contains(body));
                }
                AuthError::BadJson { snippet, .. } => assert!(detail.contains(snippet)),
                AuthError::Rejected { message } => assert!(detail.contains(message)),
                AuthError::Network { detail: d, .. }
                | AuthError::Timeout { detail: d, .. }
                | AuthError::Client { detail: d }
                | AuthError::ReadResponse { detail: d }
                | AuthError::InvalidUrl { detail: d, .. } => {
                    assert!(detail.contains(d));
                }
            }
        }
    }

    #[test]
    fn status_messages_include_http_code() {
        assert!(
            AuthError::ServerError {
                status: 502,
                body: "x".into()
            }
            .user_message()
            .contains("HTTP 502")
        );
        assert!(
            AuthError::HttpStatus {
                status: 418,
                body: "x".into()
            }
            .user_message()
            .contains("HTTP 418")
        );
    }
}
