//! Pulls messages out of a running vault, a page at a time, through
//! `GET /v1/export/messages?offset=&limit=`, and writes them as chat files.
//!
//! The `vault-pull` command and the desktop app Vault Export screen both call
//! this crate.

mod http;
pub mod journal;
mod project;
mod run;

pub use http::Message;
pub use journal::{PULL_JOURNAL_NAME, PullJournalEvent, PullJournalState, journal_path};
pub use run::{
    DEFAULT_ASSET_DOWNLOAD_WORKERS, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT, ProgressEvent, ProgressFn,
    PullReport, VaultPullConfig, run,
};
pub use vault_http::{AuthError, AuthInfo, auth_check as authenticate};
