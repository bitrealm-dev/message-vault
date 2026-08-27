//! Write and read [`message_ir::ConversationDocument`] in each output format.
//!
//! A `ConversationDocument` is the shared conversation structure every
//! exporter writes. This crate packages those documents as JSON, JSON Lines
//! (one JSON object per line), CSV, EML, MBOX, or a single SyncTech
//! `smses.xml`. It also applies media transforms and obfuscation through
//! [`FormatSink`].
//!
//! Directory convert lives in `message-reexport`. Schema types live in
//! `message-ir`.

#![warn(missing_docs)]

mod clean;
mod export_transforms;
mod format_sink;
mod normalize;
mod pipeline;
mod read_csv;
mod read_json;
mod read_mail;
mod read_sbr;
mod util;
mod write;
mod write_sbr;

pub use clean::clean_previous_ir_output;
pub use export_transforms::ExportTransforms;
pub use format_sink::{FormatSink, FormatSinkResult};
pub use pipeline::{finish_run, run_pipeline};
pub use read_csv::read_conversation_csv;
pub use read_json::{read_conversation_json, read_conversation_jsonl};
pub use read_mail::{read_conversation_eml_dir, read_conversation_mbox};
pub use read_sbr::{SbrReadOptions, SbrReadReport, read_sbr_documents};
pub use util::UNSAFE_ATTACHMENT_PATH_PREFIX;
pub use write::{CSV_HEADERS, document_to_mail_messages};

#[cfg(test)]
use normalize::normalize_document_for_compare;
#[cfg(test)]
use write::{write_conversation_csv, write_format};
#[cfg(test)]
use write_sbr::SbrBackupSession;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
