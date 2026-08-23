//! Convert Apple Messages (`chat.db` or an iOS backup) into the shared
//! conversation structure ([`message_ir::ConversationDocument`]) every exporter writes.
//!
//! Each row becomes a [`mail::MailMessage`], then a [`message_ir::IrMessage`].
//! [`message_ir_format::FormatSink`] writes the chosen output format (JSON Lines,
//! JSON, CSV, EML, MBOX, or XML).

mod attachments;
mod backup;
mod body;
mod contacts;
mod data_source;
mod emit;
mod error;
mod fields;
mod options;
mod run;
mod session;

pub use message_vault_io_core::RunResult;
pub use run::run;

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub use cli::clap_command;
