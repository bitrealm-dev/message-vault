use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = message_vault_server::cli::Cli::parse();
    message_vault_server::cli::run(cli)
}
