//! Upload a folder of conversation files into the Message Vault HTTP server.
//!
//! Each conversation is a JSON Lines file (one JSON object per line). The
//! `vault-push` command and the desktop app Vault tab both call this crate.

#[cfg(feature = "cli")]
pub mod cli;
mod http;
mod journal;
mod project;
mod run;

pub use journal::{JOURNAL_NAME, LOG_NAME, REPORT_NAME};
pub use run::{
    DEFAULT_ASSET_MAX_BYTES, DEFAULT_ASSET_UPLOAD_WORKERS, DEFAULT_BATCH_SIZE,
    DEFAULT_PREPARE_AHEAD, DEFAULT_PREPARE_WORKERS, FileResult, MAX_IMPORT_BODY_BYTES,
    MAX_PROXY_BODY_BYTES, ProgressEvent, ProgressFn, PushReport, UploadProfile, VaultPushConfig,
    authenticate, detect_source, format_duration_ms, format_push_summary, run,
};
pub use vault_http::AuthError;
pub use vault_http::AuthInfo;

#[cfg(feature = "cli")]
pub use cli::clap_command;
