use std::path::PathBuf;

use clap::Parser;
use dump_cli_docs::write_pages;

#[derive(Parser)]
#[command(name = "dump-cli-docs")]
struct Args {
    #[arg(long)]
    output_dir: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    write_pages(&args.output_dir)?;
    Ok(())
}
