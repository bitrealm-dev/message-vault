//! CLI for regenerating the committed demo bundle.

use std::path::Path;

use anyhow::Result;
use clap::Parser;
use demo_seed::SeedConfig;

#[derive(Parser)]
#[command(name = "demo-seed")]
#[command(about = "Generate committed iMessage demo data for Message Vault")]
struct Cli {
    /// Path to demo_seed.toml
    #[arg(long, default_value_t = SeedConfig::default_path().display().to_string())]
    config: String,

    /// Output directory (demo bundle root); overrides config
    #[arg(long)]
    out: Option<String>,

    /// PRNG seed; overrides config
    #[arg(long)]
    seed: Option<u64>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut cfg = SeedConfig::load(Path::new(&cli.config))?;
    if let Some(out) = cli.out {
        cfg.out = out;
    }
    if let Some(seed) = cli.seed {
        cfg.seed = seed;
    }
    demo_seed::generate(&cfg)?;
    Ok(())
}
