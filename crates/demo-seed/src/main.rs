//! Synthetic iMessage JSONL demo dataset for Message Vault.

mod assets;
mod config;
mod contacts;
mod conversations;
mod corpus;
mod names;
mod personas;
mod phones;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::config::SeedConfig;

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

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut cfg = SeedConfig::load(Path::new(&cli.config))?;
    if let Some(out) = cli.out {
        cfg.out = out;
    }
    if let Some(seed) = cli.seed {
        cfg.seed = seed;
    }

    let out = Path::new(&cfg.out);
    let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);

    let staging = out.join("staging/imessage");
    let attachments = staging.join("attachments");
    let config_dir = out.join("config");

    fs::create_dir_all(&staging)?;
    fs::create_dir_all(&attachments)?;
    fs::create_dir_all(&config_dir)?;

    let corpus = corpus::Corpus::load_pride_and_prejudice()
        .context("load public-domain message corpus")?;
    let names = names::NameBank::load_default().context("load name lists")?;

    assets::write_attachment_blobs(&attachments)?;
    let roster = personas::build_roster(&cfg, &names, &mut rng);
    contacts::write_vcf(&config_dir, &roster)?;
    contacts::write_config_toml(&config_dir)?;
    contacts::write_seed_toml(&config_dir)?;

    let stats =
        conversations::write_all(&staging, &attachments, &roster, &cfg, &corpus, &mut rng)?;

    write_readme(out, &stats, &cfg, corpus.len())?;

    println!("demo-seed: wrote {}", out.display());
    println!("  seed:          {}", cfg.seed);
    println!("  contacts:      {}", stats.contacts);
    println!("  groups:        {}", stats.groups);
    println!("  conversations: {}", stats.conversation_files);
    println!("  messages:      {}", stats.messages);
    println!("  attachments:   {}", stats.attachment_refs);
    println!("  corpus lines:  {}", corpus.len());
    Ok(())
}

fn write_readme(
    out: &Path,
    stats: &conversations::GenStats,
    cfg: &SeedConfig,
    corpus_sentences: usize,
) -> Result<()> {
    let path = out.join("README.md");
    let body = format!(
        r#"# Message Vault demo dataset

Committed message-ir JSONL bundle for local browsing without a real phone backup.

Regenerate with:

```bash
cargo run -p demo-seed -- --config crates/demo-seed/demo_seed.toml --out demo
```

Config knobs live in `crates/demo-seed/demo_seed.toml` (seed, contact count, rate/span
distributions, group membership). Message bodies are sampled from Pride and Prejudice
({corpus_sentences} sentences) under `crates/demo-seed/data/corpus/`. Names come from
`crates/demo-seed/data/names/`.

Then import:

```bash
cargo run --release -- reset-demo
cargo run --release -- process-assets
```

## Contents (seed {seed})

| Item | Count |
|------|------:|
| Contacts (VCF) | {contact_count} |
| Groups | {group_count} |
| Conversation files | {conversation_count} |
| Messages | {message_count} |
| Attachment references | {attachment_count} |

## Exercises

- **Contacts / labels / No Messages** — label memberships and zero-message rows
- **Unassigned** — handles with messages but no VCF row (phone + email)
- **Rate skew** — most 1:1 threads ~200–300 msgs/year; rare whales up to ~12k/year
- **History** — typical first contact ~3–5 years ago; longest ~14 years; newest ~1 week
- **Group Chats** — membership mean ~5 groups/contact; size mean ~4; 5–50 msgs/year
- **Replies, tapbacks, attachments** — including one intentionally missing file
- **orphaned.jsonl** — synthetic orphaned conversation
"#,
        seed = cfg.seed,
        corpus_sentences = corpus_sentences,
        contact_count = stats.contacts,
        group_count = stats.groups,
        conversation_count = stats.conversation_files,
        message_count = stats.messages,
        attachment_count = stats.attachment_refs,
    );
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
