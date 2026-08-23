//! Convert SMS Backup & Restore XML into the shared conversation structure
//! ([`message_ir::ConversationDocument`]) every exporter writes.
//!
//! Library entry: [`run`] for the full pipeline. The
//! `sms-backup-restore-exporter` binary is a thin CLI over it.

#[cfg(feature = "cli")]
pub mod cli;
mod emit;
mod run;

pub use message_vault_io_core::{RunResult, parse_date_range};
pub use run::run;

#[cfg(feature = "cli")]
pub use cli::clap_command;

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
