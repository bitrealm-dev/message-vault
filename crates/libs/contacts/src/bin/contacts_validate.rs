//! Copy a contacts VCF or CSV and rewrite only unambiguous phone numbers.

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;
use contacts::{ValidateMode, validate_contacts_file};
use phone::PhoneRegion;

#[derive(Parser, Debug)]
#[command(name = "contacts-validate")]
#[command(about = "Check or update contacts phones; write corrected copy on update")]
struct Cli {
    /// Contacts file (.vcf or vCard CSV)
    #[arg(long)]
    input: PathBuf,

    /// usa | international
    #[arg(long, default_value = "usa")]
    region: String,

    /// Analyze only; print the validate log to stdout and write nothing
    #[arg(long)]
    check: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let Some(region) = PhoneRegion::parse_cli(&cli.region) else {
        bail!(
            "unknown --region {:?} (use usa or international)",
            cli.region
        );
    };

    let mode = if cli.check {
        ValidateMode::Check
    } else {
        ValidateMode::Update
    };
    let report = validate_contacts_file(&cli.input, region, mode)?;

    for line in &report.log_lines {
        println!("{line}");
    }
    Ok(())
}
