//! GO SMS Pro → per-conversation CSV exporter.
//!
//! Library entrypoint: [`run`] for the full pipeline.
//! The `go-sms-pro-exporter` binary is a thin CLI over [`run`].

mod emit;
mod phone;
mod run;
mod xml;

pub use message_vault_io_core::{RunResult, parse_date_range};
pub use run::run;

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
