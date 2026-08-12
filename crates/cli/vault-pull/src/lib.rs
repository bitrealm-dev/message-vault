//! Pull messages from Message Vault into a message-ir folder.
//!
//! Query prefers `GET /v1/export/messages/count`, then falls back to paging
//! `GET /v1/export/messages`. Used by the `vault-pull` CLI and the Message Vault
//! GUI Vault Export screen.

mod http;
pub mod journal;
mod project;
mod run;

pub use http::ExportMessage;
pub use journal::{PULL_JOURNAL_NAME, PullJournalEvent, PullJournalState, journal_path};
pub use run::{
    DEFAULT_ASSET_DOWNLOAD_WORKERS, DEFAULT_PAGE_LIMIT, ProgressEvent, ProgressFn, PullReport,
    QueryStats, VaultPullConfig, compose_query, query_stats, run,
};
pub use vault_push::{AuthError, AuthInfo, authenticate};
