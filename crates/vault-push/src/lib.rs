//! Push message-ir JSONL export folders into a Message Vault import API.
//!
//! Used by the `vault-push` CLI and the Message Vault GUI Vault tab.

mod auth_error;
mod http;
mod journal;
mod project;
mod run;

pub use auth_error::AuthError;
pub use http::AuthInfo;
pub use journal::{JOURNAL_NAME, LOG_NAME, REPORT_NAME};
pub use run::{
    DEFAULT_ASSET_MAX_BYTES, DEFAULT_ASSET_UPLOAD_WORKERS, DEFAULT_BATCH_SIZE, FileResult,
    MAX_IMPORT_BODY_BYTES, MAX_PROXY_BODY_BYTES, ProgressEvent, ProgressFn, PushReport,
    UploadProfile, VaultPushConfig, authenticate, detect_source, format_duration_ms, run,
};
