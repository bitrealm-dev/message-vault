//! Synthetic iMessage JSONL demo dataset for Message Vault.

mod assets;
mod contacts;
mod conversations;
mod personas;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Parser)]
#[command(name = "demo-seed")]
#[command(about = "Generate committed iMessage demo data for Message Vault")]
struct Cli {
    /// Output directory (demo bundle root)
    #[arg(long, default_value = "demo")]
    out: String,

    /// PRNG seed for reproducible output
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let out = Path::new(&cli.out);
    let mut rng = ChaCha8Rng::seed_from_u64(cli.seed);

    let staging = out.join("staging/imessage");
    let attachments = staging.join("attachments");
    let config_dir = out.join("config");

    fs::create_dir_all(&staging)?;
    fs::create_dir_all(&attachments)?;
    fs::create_dir_all(&config_dir)?;

    assets::write_attachment_blobs(&attachments)?;
    let roster = personas::build_roster();
    contacts::write_csvs(&config_dir, &roster)?;
    contacts::write_config_toml(&config_dir)?;
    contacts::write_seed_toml(&config_dir)?;

    let stats = conversations::write_all(&staging, &attachments, &roster, &mut rng)?;

    write_readme(out, &stats)?;

    println!("demo-seed: wrote {}", out.display());
    println!("  contacts:      {}", stats.contacts);
    println!("  conversations: {}", stats.conversation_files);
    println!("  messages:      {}", stats.messages);
    println!("  attachments:   {}", stats.attachment_refs);
    Ok(())
}

fn write_readme(out: &Path, stats: &conversations::GenStats) -> Result<()> {
    let path = out.join("README.md");
    let body = format!(
        r#"# Message Vault demo dataset

Committed iMessage JSONL bundle for local browsing without a real iPhone backup.

Regenerate with:

```bash
cargo run -p demo-seed -- --out demo --seed 42
```

Then import:

```bash
cargo run --release -- reset-demo
cd web && npm run process-assets
```

## Contents

| Item | Count |
|------|-------|
| Contacts (CSV) | {contact_count} |
| Conversation files | {conversation_count} |
| Messages | {message_count} |
| Attachment references | {attachment_count} |

## Exercises

- **Contacts / All / Excluded / No Messages** — CSV `exclude` and zero-message rows
- **Unassigned** — handles with messages but no CSV row (phone + email-only)
- **Frequent / lapsed** — ~15 contacts busy in the past 3 years; ~10 mostly older history
- **High volume** — a couple 1:1 threads with 1000+ messages
- **Group Chats** — ~200 threads, many untitled, some phone-number-only participants, sizes up to 20
- **Year threads** — message history from 2016 through present (10 years)
- **Replies, tapbacks, attachments** — including one intentionally missing file
- **orphaned.jsonl** — messages without a conversation header
- **exclude.csv** — short-code spam absent after import
"#,
        contact_count = stats.contacts,
        conversation_count = stats.conversation_files,
        message_count = stats.messages,
        attachment_count = stats.attachment_refs,
    );
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
