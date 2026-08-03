//! OpenExtract conversation CSV (+ VCF) → shared per-chat CSV.
//!
//! Library entrypoint: [`run`] for the full pipeline.
//! The `openextract-exporter` binary is a thin CLI over [`run`].

mod emit;
mod parse;
mod run;

pub use message_vault_io_core::{RunResult, parse_date_range};
pub use run::run;

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
