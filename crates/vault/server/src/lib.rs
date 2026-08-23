//! HTTP API and SQLite storage for browsing imported messages.
#![warn(missing_docs)]

pub mod api_tokens_api;
pub mod asset_uploads;
pub mod assets;
pub mod auth;
pub mod cli;
pub mod config;
pub mod contact_groups_api;
pub mod contacts_api;
pub mod conversations_api;
pub mod db;
pub mod dedupe;
pub mod export_api;
pub mod guest_clone;
pub mod guest_pool;
pub mod import;
pub mod import_cli;
pub mod import_media;
pub mod jsonl;
pub mod media_tools;
pub mod models;
pub mod openapi;
pub mod operation_lock;
pub(crate) mod page_limits;
pub mod process_assets;
pub mod profile;
pub mod reset_demo;
pub mod search_query;
pub mod server;
pub mod thread_tags_api;

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
