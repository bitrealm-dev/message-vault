//! Shared CLI argument groups for exporter binaries.
//!
//! Every exporter CLI flattens [`CommonCli`] plus its own source-specific args.

use std::path::PathBuf;

use media::{MaxResolution, MediaMode};

use crate::{ContactsConfig, ContactsKind, ObfuscateConfig, contacts_kind_from_path};

/// CLI arguments common to (nearly) every exporter.
///
/// Use with `#[command(flatten)]` in the exporter's `Cli` struct.
#[derive(Debug, Clone, clap::Args)]
pub struct CommonCli {
    /// Output directory for packaging + attachments/
    #[arg(long)]
    pub output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    pub format: String,

    /// vCard CSV or VCF contacts file for phone→name resolution.
    /// Optional; without it phone numbers are not resolved to names.
    #[arg(long, conflicts_with = "vcf")]
    pub contacts: Option<PathBuf>,

    /// Contacts VCF (alternate to `--contacts`).
    #[arg(long)]
    pub vcf: Option<PathBuf>,

    /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
    #[arg(long)]
    pub obfuscate: bool,

    /// Optional seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    pub obfuscate_seed: Option<String>,

    /// Only messages on or after this date (YYYY-MM-DD, local midnight, inclusive)
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    pub start_date: Option<String>,

    /// Only messages before this date (YYYY-MM-DD, local midnight, exclusive)
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    pub end_date: Option<String>,

    /// Attachment media: disabled (no files), clone (default), convert, or compress
    #[arg(long = "media-mode", default_value = "clone", value_name = "MODE")]
    pub media_mode: MediaMode,

    /// Compress only: max long edge (720p, 1080p, 4k)
    #[arg(
        long = "media-max-resolution",
        default_value = "1080p",
        value_name = "RES"
    )]
    pub media_max_resolution: MaxResolution,

    /// Compress only: max frame rate
    #[arg(long = "media-max-fps", default_value_t = 30.0)]
    pub media_max_fps: f32,

    /// Compress only: only re-encode videos at/above this size (e.g. 20M)
    #[arg(long = "media-min-size", default_value = "20M")]
    pub media_min_size: String,

    /// Compress only: skip already-efficient HEVC under max resolution (default on)
    #[arg(long = "media-skip-efficient", default_value_t = true, action = clap::ArgAction::Set)]
    pub media_skip_efficient: bool,
}

impl CommonCli {
    /// Build the `ContactsConfig` from `--contacts` / `--vcf`.
    ///
    /// When `--contacts` has a known extension (`.vcf`), the kind is inferred;
    /// otherwise it is treated as CSV. This matches iMazing's historical
    /// behavior and works for all exporters.
    pub fn contacts_config(&self) -> Option<ContactsConfig> {
        match (&self.contacts, &self.vcf) {
            (Some(path), _) => {
                let kind = contacts_kind_from_path(&path.to_string_lossy());
                Some(ContactsConfig {
                    path: path.clone(),
                    kind,
                })
            }
            (None, Some(path)) => Some(ContactsConfig {
                path: path.clone(),
                kind: ContactsKind::Vcf,
            }),
            (None, None) => None,
        }
    }

    /// Build the `ObfuscateConfig` from `--obfuscate` / `--obfuscate-seed`.
    pub fn obfuscate_config(&self) -> ObfuscateConfig {
        ObfuscateConfig {
            enabled: self.obfuscate,
            seed: self.obfuscate_seed.clone(),
        }
    }
}
