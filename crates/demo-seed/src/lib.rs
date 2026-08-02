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
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub use config::SeedConfig;
pub use conversations::GenStats;

/// Generate (or regenerate) a demo bundle under `cfg.out`.
pub fn generate(cfg: &SeedConfig) -> Result<GenStats> {
    let out = Path::new(&cfg.out);
    let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);

    let staging = out.join("staging/imessage");
    let attachments = staging.join("attachments");
    let config_dir = out.join("config");

    fs::create_dir_all(&staging)?;
    fs::create_dir_all(&attachments)?;
    fs::create_dir_all(&config_dir)?;

    let corpus =
        corpus::Corpus::load_pride_and_prejudice().context("load public-domain message corpus")?;
    let names = names::NameBank::load_default().context("load name lists")?;

    assets::write_attachment_blobs(&attachments)?;
    let roster = personas::build_roster(cfg, &names, &mut rng);
    contacts::write_vcf(&config_dir, &roster)?;
    contacts::write_config_toml(&config_dir)?;
    contacts::write_seed_toml(&config_dir)?;

    let stats =
        conversations::write_all(&staging, &attachments, &roster, cfg, &corpus, &mut rng)?;

    write_readme(out, &stats, cfg, corpus.len())?;

    println!("demo-seed: wrote {}", out.display());
    println!("  seed:          {}", cfg.seed);
    println!("  contacts:      {}", stats.contacts);
    println!("  groups:        {}", stats.groups);
    println!("  conversations: {}", stats.conversation_files);
    println!("  messages:      {}", stats.messages);
    println!("  attachments:   {}", stats.attachment_refs);
    println!("  corpus lines:  {}", corpus.len());
    Ok(stats)
}

/// Load `demo_seed.toml` (defaults beside the crate), apply `out` / `seed` overrides, generate.
pub fn generate_to(out: &Path, seed: Option<u64>) -> Result<GenStats> {
    let mut cfg = SeedConfig::load(&SeedConfig::default_path())?;
    cfg.out = out
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("demo out path is not UTF-8: {}", out.display()))?
        .to_string();
    if let Some(seed) = seed {
        cfg.seed = seed;
    }
    generate(&cfg)
}

fn write_readme(
    out: &Path,
    stats: &GenStats,
    cfg: &SeedConfig,
    corpus_sentences: usize,
) -> Result<()> {
    let path = out.join("README.md");
    let body = format!(
        r#"# Message Vault demo dataset

Committed message-ir JSONL bundle for local browsing without a real phone backup.

Regenerate + import in one step:

```bash
cargo run --release -- reset-demo
```

Or regenerate the bundle only:

```bash
cargo run -p demo-seed -- --out demo
```

Config knobs live in `crates/demo-seed/demo_seed.toml` (seed, contact count, rate/span
distributions, group membership). Message bodies are sampled from Pride and Prejudice
({corpus_sentences} sentences) under `crates/demo-seed/data/corpus/`. Names come from
`crates/demo-seed/data/names/`.

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
- **Rate skew** — most 1:1 threads ~200–300 msgs/year (bursty days); rare whales up to ~12k/year
- **History** — typical first contact ~3–5 years ago; longest ~14 years; newest ~1 week
- **Group Chats** — membership mean ~5 groups/contact; size mean ~4; bursty days (several / none / a lot)
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
