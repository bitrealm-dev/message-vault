//! Pull messages from Message Vault `GET /v1/export/messages` into a message-ir folder.
//!
//! Used by the `vault-pull` CLI and the Message Vault GUI Vault Export screen.

mod http;
mod project;
mod run;

pub use http::ExportMessage;
pub use run::{
    DEFAULT_PAGE_LIMIT, ProgressEvent, ProgressFn, PullReport, QueryStats, VaultPullConfig,
    compose_query, query_stats, run,
};
pub use vault_push::{AuthError, AuthInfo, authenticate};
