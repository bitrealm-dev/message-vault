//! Download messages from the Message Vault HTTP server into a local folder.
//!
//! A count request (`GET /v1/export/messages/count`) is tried first. Older
//! servers that lack that route are queried by paging `GET /v1/export/messages`.
//! The `vault-pull` command and the desktop app Vault Export screen both call
//! this crate.

mod http;
pub mod journal;
mod project;
mod run;

pub use http::ExportMessage;
pub use journal::{PULL_JOURNAL_NAME, PullJournalEvent, PullJournalState, journal_path};
pub use run::{
    DEFAULT_ASSET_DOWNLOAD_WORKERS, DEFAULT_PAGE_LIMIT, ProgressEvent, ProgressFn, PullReport,
    VaultPullConfig, compose_query, run,
};
pub use vault_http::{AuthError, AuthInfo, auth_check as authenticate};
