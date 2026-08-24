//! HTTP API and SQLite storage for browsing imported messages.
#![warn(missing_docs)]

pub mod cli;
pub mod config;

pub(crate) mod api_tokens_api;
pub(crate) mod asset_uploads;
pub(crate) mod assets;
pub(crate) mod auth;
pub(crate) mod contact_groups_api;
pub(crate) mod contacts_api;
pub(crate) mod conversations_api;
pub(crate) mod db;
pub(crate) mod dedupe;
pub(crate) mod export_api;
pub(crate) mod guest_clone;
pub(crate) mod guest_pool;
pub(crate) mod import;
pub(crate) mod import_cli;
pub(crate) mod import_media;
pub(crate) mod jsonl;
pub(crate) mod media_tools;
pub(crate) mod models;
pub(crate) mod named_membership;
pub(crate) mod openapi;
pub(crate) mod operation_lock;
pub(crate) mod page_limits;
pub(crate) mod process_assets;
pub(crate) mod profile;
pub(crate) mod reset_demo;
pub(crate) mod search_query;
pub(crate) mod server;
pub(crate) mod thread_tags_api;

pub use server::{ApiError, AppState, AuthCapability, AuthIdentity, ErrorBody, resolve_auth, run};

// Integration tests (crates/vault/server/tests) cannot see `pub(crate)`
// modules, so the search-parity suite reaches the schema, export, and query
// parser entry points through these re-exports.
pub use db::schema::ensure_vault_schema;
pub use export_api::{ExportPageOpts, export_messages};
pub use search_query::parse_search_query;

use clap::Command;

/// Postgres test URL when the gated suite should run (CI sets this).
pub fn pg_test_url() -> Option<String> {
    std::env::var("MV_TEST_POSTGRES_URL")
        .ok()
        .filter(|u| !u.is_empty())
}

/// Serializes the Postgres-gated tests: they share one database, and two of
/// them (`messages_fts_stays_in_sync_pg` and `promote_fts_cycle_pg`) run in
/// the same test binary, where concurrent `ensure_vault_schema` calls race on
/// Postgres's composite-type creation (`CREATE TABLE IF NOT EXISTS` is not
/// race-safe there). The integration-test binary runs after the lib binary,
/// so it needs no lock.
#[cfg(test)]
pub(crate) static PG_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Clap command definition for the `message-vault-server` CLI; delegates to
/// [`cli::clap_command`].
pub fn clap_command() -> Command {
    cli::clap_command()
}

#[cfg(test)]
mod clap_command_tests {
    #[test]
    fn clap_command_is_message_vault_server() {
        let cmd = crate::clap_command();
        assert_eq!(cmd.get_name(), "message-vault-server");
        let subs: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();
        assert!(subs.contains(&"serve"));
        assert!(subs.contains(&"dump-openapi"));
        assert!(subs.contains(&"import"));
    }
}
