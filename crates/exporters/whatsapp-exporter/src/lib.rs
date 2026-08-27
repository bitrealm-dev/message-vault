//! Convert WhatsApp chats (via KnugiHK wtsexporter JSON) into the shared
//! conversation structure ([`message_ir::ConversationDocument`]) every exporter writes.
//!
//! Library entry: [`run`] for the full pipeline.
//! The `whatsapp-exporter` binary is a thin CLI over [`run`].

mod emit;
mod jid;
mod parse;
mod run;
mod wtsexporter;

pub use message_vault_io_core::{RunResult, parse_date_range};
pub use run::run;

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub use cli::clap_command;

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
