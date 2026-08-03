//! iMazing Messages / WhatsApp CSV (+ vCard CSV / VCF contacts) → shared per-chat CSV.
//!
//! Library entrypoint: [`run`] for the full pipeline.
//! The `imazing-exporter` binary is a thin CLI over [`run`].

mod attachments;
mod emit;
mod parse;
mod run;

pub use message_vault_io_core::{RunResult, parse_date_range_tz as parse_date_range};
pub use run::run;

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
