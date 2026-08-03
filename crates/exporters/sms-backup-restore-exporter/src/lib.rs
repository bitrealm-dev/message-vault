//! SMS Backup & Restore → per-conversation CSV or EML archive exporter.
//!
//! Library entrypoint: [`run`] for the full pipeline. The
//! `sms-backup-restore-exporter` binary is a thin CLI over it.

mod emit;
mod run;

pub use message_vault_io_core::{RunResult, parse_date_range};
pub use run::run;

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
