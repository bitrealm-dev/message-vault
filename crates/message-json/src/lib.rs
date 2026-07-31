//! Shared JSONL interchange schemas for message archives.
//!
//! - [`vault`] — standard format for all sources (CSV ingest + future API)
//! - [`imessage`] — legacy iOS exporter wire shape (`schema: "imessage"`)
//! - [`sms`] — legacy SMS Backup+ JSONL (`schema: "sms"`)

pub mod imessage;
pub mod sms;
pub mod vault;
