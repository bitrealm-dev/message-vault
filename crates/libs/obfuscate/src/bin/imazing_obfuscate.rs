use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use obfuscate::{obfuscate_imazing, resolve_obfuscator};

#[derive(Parser, Debug)]
#[command(name = "imazing-obfuscate")]
#[command(
    about = "Rewrite iMazing Messages CSV with obfuscated names, numbers, text, and attachments"
)]
struct Cli {
    /// iMazing CSV file or directory of CSVs
    #[arg(long)]
    input: PathBuf,

    /// Output directory for obfuscated CSV + placeholder attachments/
    #[arg(long)]
    output: PathBuf,

    /// Optional 8-hex seed for reproducible remaps
    #[arg(long = "obfuscate-seed")]
    obfuscate_seed: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut anon = resolve_obfuscator(cli.obfuscate_seed.as_deref())?;
    let n = obfuscate_imazing(&cli.input, &cli.output, &mut anon)?;
    println!(
        "Wrote {} obfuscated CSV file(s) to {}",
        n,
        cli.output.display()
    );
    Ok(())
}
