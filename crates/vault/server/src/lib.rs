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

use clap::Command;

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
