//! Pull messages from Message Vault into a message-ir folder.
//!
//! Query prefers `GET /v1/export/messages/count`, then falls back to paging
//! `GET /v1/export/messages`. Used by the `vault-pull` CLI and the Message Vault
//! GUI Vault Export screen.

mod http;
mod project;
mod run;

pub use http::ExportMessage;
pub use run::{
    DEFAULT_PAGE_LIMIT, ProgressEvent, ProgressFn, PullReport, QueryStats, VaultPullConfig,
    compose_query, query_stats, run,
};
pub use vault_push::{AuthError, AuthInfo, authenticate};
