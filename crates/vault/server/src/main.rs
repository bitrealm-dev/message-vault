use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = message_vault_server::cli::Cli::parse();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(message_vault_server::cli::run(cli))
}
