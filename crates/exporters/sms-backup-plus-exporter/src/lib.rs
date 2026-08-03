//! SMS Backup+ (jberkel) EML → per-conversation CSV exporter.
//!
//! Library entrypoint: [`run`] for the full pipeline.
//! The `sms-backup-plus-exporter` binary is a thin CLI over [`run`].

mod archive;
mod assets;
mod contacts;
mod emit;
mod flat_eml;
mod identity;
mod run;
mod types;

pub use message_vault_io_core::{RunResult, parse_date_range};
pub use run::run;

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
