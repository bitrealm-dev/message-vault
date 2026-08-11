//! Synthetic multi-source JSONL demo dataset for Message Vault.

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

const IMESSAGE_SOURCE: &str = "imessage";
const SBR_SOURCE: &str = "sms-backup-restore";
const WHATSAPP_SOURCE: &str = "whatsapp";

/// Generate (or regenerate) a demo bundle under `cfg.out`.
pub fn generate(cfg: &SeedConfig) -> Result<GenStats> {
    let out = Path::new(&cfg.out);
    let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);

    let imessage_staging = out.join("staging").join(IMESSAGE_SOURCE);
    let sbr_staging = out.join("staging").join(SBR_SOURCE);
    let whatsapp_staging = out.join("staging").join(WHATSAPP_SOURCE);
    let imessage_attachments = imessage_staging.join("attachments");
    let sbr_attachments = sbr_staging.join("attachments");
    let whatsapp_attachments = whatsapp_staging.join("attachments");
    let config_dir = out.join("config");

    fs::create_dir_all(&imessage_staging)?;
    fs::create_dir_all(&sbr_staging)?;
    fs::create_dir_all(&whatsapp_staging)?;
    fs::create_dir_all(&imessage_attachments)?;
    fs::create_dir_all(&sbr_attachments)?;
    fs::create_dir_all(&whatsapp_attachments)?;
    fs::create_dir_all(&config_dir)?;

    let corpus =
        corpus::Corpus::load_pride_and_prejudice().context("load public-domain message corpus")?;
    let names = names::NameBank::load_default().context("load name lists")?;

    let attachment_digests = assets::write_attachment_blobs(&imessage_attachments)?;
    // Same blobs under the Android/WhatsApp trees so relative attachment paths resolve on import.
    copy_dir_files(&imessage_attachments, &sbr_attachments)?;
    copy_dir_files(&imessage_attachments, &whatsapp_attachments)?;

    let roster = personas::build_roster(cfg, &names, &mut rng)?;
    contacts::write_vcf(&config_dir, &roster)?;
    contacts::write_config_toml(&config_dir)?;
    contacts::write_seed_toml(&config_dir)?;

    let stats = conversations::write_all(
        &imessage_staging,
        &sbr_staging,
        &whatsapp_staging,
        &roster,
        cfg,
        &corpus,
        &mut rng,
        &attachment_digests,
    )?;

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

fn copy_dir_files(from: &Path, to: &Path) -> Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = entry.file_name();
            fs::copy(&path, to.join(&name))
                .with_context(|| format!("copy {} → {}", path.display(), to.display()))?;
        }
    }
    Ok(())
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

Three staging trees simulate separate backups:

- `staging/imessage/` — Apple Messages-style export
- `staging/sms-backup-restore/` — Android SMS Backup & Restore–style export
- `staging/whatsapp/` — WhatsApp-style export for ~{whatsapp_pct}% of contacts (same phone, platform `whatsapp`)

Most conversations are single-source. A small set appears in both iMessage and Android so the
Sources panel and cross-source dedupe can be exercised. WhatsApp threads share the phone number
with Text message handles so the contact drawer shows both platforms.

Regenerate + import in one step:

```bash
cargo run --release -p message-vault-server -- reset-demo
```

Or regenerate the bundle only:

```bash
cargo run -p demo-seed -- --out demo
```

Config knobs live in `crates/vault/demo-seed/demo_seed.toml` (seed, contact count, rate/span
distributions, group membership, dual-source split, `whatsapp_contact_fraction`,
`apple_fallback_transport_fraction`). Message bodies are sampled from Pride and
Prejudice ({corpus_sentences} sentences) under `crates/vault/demo-seed/data/corpus/`. Names come from
`crates/vault/demo-seed/data/names/`.

## Contents (seed {seed})

| Item | Count |
|------|------:|
| Contacts (VCF) | {contact_count} |
| Groups | {group_count} |
| Conversation files | {conversation_count} |
| Messages | {message_count} |
| Attachment references | {attachment_count} |

## Exercises

- **Triple sources** — `imessage` vs `sms-backup-restore` vs `whatsapp`
- **Platform handles** — Text message + WhatsApp rows on the same contact
- **Transport mix** — SMS/RCS mixed into iMessage threads (~20% by default)
- **Contacts / labels / No Messages** — label memberships and zero-message rows
- **Unassigned** — handles with messages but no VCF row (phone + email)
- **Rate skew** — most 1:1 threads ~200–300 msgs/year (bursty days); rare whales up to ~12k/year
- **History** — typical first contact ~3–5 years ago; longest ~14 years; newest ~1 week
- **Group Chats** — membership mean ~5 groups/contact; size mean ~4; at least 10 groups with 8–20 participants; bursty days (several / none / a lot)
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
        whatsapp_pct = (cfg.sources.whatsapp_contact_fraction * 100.0).round() as i64,
    );
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
