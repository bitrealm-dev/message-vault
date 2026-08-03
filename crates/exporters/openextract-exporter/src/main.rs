use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use message_vault_io_core::{
    ContactsConfig, ContactsKind, ExporterConfig, MediaConfig, ObfuscateConfig, OpenExtractConfig,
    OutputFormat, SourceConfig,
};
use media::MediaMode;
use openextract_exporter::{parse_date_range, run};

#[derive(Parser, Debug)]
#[command(name = "openextract-exporter")]
#[command(
    about = "Convert OpenExtract conversation CSV (+ VCF) via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
struct Cli {
    /// OpenExtract CSV file or directory of conversation_*.csv / all_conversations.csv
    #[arg(long)]
    input: PathBuf,

    /// Output directory for packaging + attachments/
    #[arg(long)]
    output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    format: String,

    /// Contacts VCF from the OpenExtract export (phone ↔ name)
    #[arg(long)]
    vcf: Option<PathBuf>,

    /// Contacts file instead of --vcf (VCF or vCard CSV; same as contacts-validate)
    #[arg(long)]
    contacts: Option<PathBuf>,

    /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
    #[arg(long)]
    obfuscate: bool,

    /// Optional 8-hex seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    obfuscate_seed: Option<String>,

    /// Only messages on or after this date (YYYY-MM-DD, local midnight, inclusive)
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    start_date: Option<String>,

    /// Only messages before this date (YYYY-MM-DD, local midnight, exclusive)
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    end_date: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let date_range = parse_date_range(cli.start_date.as_deref(), cli.end_date.as_deref())
        .map_err(anyhow::Error::msg)?;
    let output_format = OutputFormat::parse(&cli.format).map_err(anyhow::Error::msg)?;
    let contacts = match (cli.contacts, cli.vcf) {
        (Some(path), _) => Some(ContactsConfig {
            path,
            kind: ContactsKind::Csv,
        }),
        (None, Some(path)) => Some(ContactsConfig {
            path,
            kind: ContactsKind::Vcf,
        }),
        (None, None) => None,
    };
    let result = run(&ExporterConfig {
        inputs: vec![cli.input],
        output: cli.output,
        date_range,
        contacts,
        obfuscate: ObfuscateConfig {
            enabled: cli.obfuscate,
            seed: cli.obfuscate_seed,
        },
        media: MediaConfig {
            mode: MediaMode::Disabled,
            compress: Default::default(),
        },
        cancel: None,
        log: None,
        output_format,
        source: SourceConfig::OpenExtract(OpenExtractConfig {}),
    })?;

    for line in &result.messages {
        if line.starts_with("Obfuscated ") {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}
