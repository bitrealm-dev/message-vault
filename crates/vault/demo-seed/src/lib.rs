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
use std::path::{Path, PathBuf};

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
    let parent = out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create demo output parent {}", parent.display()))?;
    let prepared = tempfile::Builder::new()
        .prefix(".demo-seed-")
        .tempdir_in(parent)
        .with_context(|| format!("create temporary demo bundle beside {}", out.display()))?;
    let replacement = prepare_and_replace(out, prepared.path(), |root| generate_into(cfg, root));
    let stats = match replacement {
        Ok(stats) => stats,
        Err(error) if prepared.path().join(".previous-active").exists() => {
            let kept = prepared.keep();
            return Err(error.context(format!(
                "demo bundle rollback was incomplete; prepared output and backups were kept at {}",
                kept.display()
            )));
        }
        Err(error) => return Err(error),
    };

    println!("demo-seed: wrote {}", out.display());
    println!("  seed:          {}", cfg.seed);
    println!("  contacts:      {}", stats.contacts);
    println!("  groups:        {}", stats.groups);
    println!("  conversations: {}", stats.conversation_files);
    println!("  messages:      {}", stats.messages);
    println!("  attachments:   {}", stats.attachment_refs);
    Ok(stats)
}

fn generate_into(cfg: &SeedConfig, out: &Path) -> Result<GenStats> {
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

    Ok(stats)
}

fn prepare_and_replace<F>(active: &Path, prepared: &Path, prepare: F) -> Result<GenStats>
where
    F: FnOnce(&Path) -> Result<GenStats>,
{
    if active == prepared {
        anyhow::bail!("active and prepared demo roots must differ");
    }
    let stats = prepare(prepared)?;
    validate_generated_bundle(prepared)?;
    replace_generated_paths(active, prepared)?;
    Ok(stats)
}

fn validate_generated_bundle(root: &Path) -> Result<()> {
    for source in [IMESSAGE_SOURCE, SBR_SOURCE, WHATSAPP_SOURCE] {
        let staging = root.join("staging").join(source);
        if !staging.is_dir() {
            anyhow::bail!("prepared demo bundle is missing {}", staging.display());
        }
    }
    for relative in [
        Path::new("config/config.toml"),
        Path::new("config/seed.toml"),
        Path::new("config/contacts.vcf"),
        Path::new("README.md"),
    ] {
        let path = root.join(relative);
        if !path.is_file() {
            anyhow::bail!("prepared demo bundle is missing {}", path.display());
        }
    }
    validate_tree_files(root)
}

fn validate_tree_files(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read prepared directory {}", directory.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let bytes = fs::read(&path)
                .with_context(|| format!("read prepared demo file {}", path.display()))?;
            if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                let text = std::str::from_utf8(&bytes)
                    .with_context(|| format!("decode prepared JSONL {}", path.display()))?;
                for (index, line) in text.lines().enumerate() {
                    serde_json::from_str::<serde_json::Value>(line)
                        .with_context(|| format!("parse {} line {}", path.display(), index + 1))?;
                }
            }
        }
    }
    Ok(())
}

fn replace_generated_paths(active: &Path, prepared: &Path) -> Result<()> {
    replace_generated_paths_with(active, prepared, |source, destination| {
        fs::rename(source, destination).with_context(|| {
            format!(
                "rename generated demo path {} to {}",
                source.display(),
                destination.display()
            )
        })
    })
}

fn replace_generated_paths_with<F>(active: &Path, prepared: &Path, mut rename: F) -> Result<()>
where
    F: FnMut(&Path, &Path) -> Result<()>,
{
    const GENERATED_PATHS: [&str; 3] = ["staging", "config", "README.md"];

    fs::create_dir_all(active)
        .with_context(|| format!("create active demo root {}", active.display()))?;
    let backup = prepared.join(".previous-active");
    fs::create_dir(&backup)
        .with_context(|| format!("create demo replacement backup {}", backup.display()))?;

    let mut backed_up = Vec::<PathBuf>::new();
    let mut installed = Vec::<PathBuf>::new();
    let replacement = (|| -> Result<()> {
        for name in GENERATED_PATHS {
            let destination = active.join(name);
            if destination.exists() {
                rename(&destination, &backup.join(name)).with_context(|| {
                    format!(
                        "move existing demo path {} into backup",
                        destination.display()
                    )
                })?;
                backed_up.push(PathBuf::from(name));
            }
        }
        for name in GENERATED_PATHS {
            let source = prepared.join(name);
            let destination = active.join(name);
            rename(&source, &destination).with_context(|| {
                format!(
                    "install prepared demo path {} at {}",
                    source.display(),
                    destination.display()
                )
            })?;
            installed.push(PathBuf::from(name));
        }
        Ok(())
    })();

    if let Err(error) = replacement {
        let mut rollback_errors = Vec::new();
        for name in installed.iter().rev() {
            if let Err(rollback_error) = remove_path_if_exists(&active.join(name)) {
                rollback_errors.push(format!(
                    "remove installed {}: {rollback_error:#}",
                    active.join(name).display()
                ));
            }
        }
        for name in backed_up.iter().rev() {
            if let Err(rollback_error) = rename(&backup.join(name), &active.join(name)) {
                rollback_errors.push(format!(
                    "restore previous demo path {}: {rollback_error:#}",
                    active.join(name).display()
                ));
            }
        }
        if rollback_errors.is_empty() {
            if let Err(cleanup_error) = fs::remove_dir_all(&backup) {
                eprintln!(
                    "warning: restored the previous demo bundle but could not remove backup {}: {cleanup_error}",
                    backup.display()
                );
            }
            return Err(error.context("replace generated demo bundle"));
        }
        return Err(anyhow::anyhow!(
            "replace generated demo bundle: {error:#}; rollback incomplete; backups kept at {}: {}",
            backup.display(),
            rollback_errors.join("; ")
        ));
    }

    if let Err(cleanup_error) = fs::remove_dir_all(&backup) {
        eprintln!(
            "warning: installed the generated demo bundle but could not remove backup {}: {cleanup_error}",
            backup.display()
        );
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    } else if path.exists() {
        fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
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
cargo run -p demo-seed
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_generation_preserves_existing_bundle() {
        let temp = tempfile::tempdir().expect("create test directory");
        let active = temp.path().join("active");
        let prepared = temp.path().join("prepared");
        let sentinel = active
            .join("staging")
            .join(IMESSAGE_SOURCE)
            .join("sentinel.jsonl");
        fs::create_dir_all(sentinel.parent().expect("sentinel parent"))
            .expect("create active staging");
        let original = b"existing demo bytes\n";
        fs::write(&sentinel, original).expect("write sentinel");

        let result = prepare_and_replace(&active, &prepared, |root| {
            fs::create_dir_all(root.join("staging").join(IMESSAGE_SOURCE))?;
            fs::write(
                root.join("staging")
                    .join(IMESSAGE_SOURCE)
                    .join("partial.jsonl"),
                b"partial replacement\n",
            )?;
            anyhow::bail!("injected preparation failure");
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&sentinel).expect("read sentinel"), original);
    }

    #[test]
    fn replacement_failure_at_each_generated_path_restores_all_old_paths() {
        for failing_install in 1..=3 {
            let temp = tempfile::tempdir().expect("create test directory");
            let active = temp.path().join("active");
            let prepared = temp.path().join("prepared");
            write_replacement_fixture(&active, b"old");
            write_replacement_fixture(&prepared, b"new");
            let mut installs = 0;

            let result = replace_generated_paths_with(&active, &prepared, |source, destination| {
                if source.starts_with(&prepared) && destination.starts_with(&active) {
                    installs += 1;
                    if installs == failing_install {
                        anyhow::bail!("injected install failure {failing_install}");
                    }
                }
                fs::rename(source, destination).map_err(Into::into)
            });

            assert!(result.is_err(), "install {failing_install} must fail");
            assert_replacement_fixture(&active, b"old");
        }
    }

    #[test]
    fn rollback_attempts_all_restorations_after_one_restore_fails() {
        let temp = tempfile::tempdir().expect("create test directory");
        let active = temp.path().join("active");
        let prepared = temp.path().join("prepared");
        write_replacement_fixture(&active, b"old");
        write_replacement_fixture(&prepared, b"new");
        let mut installs = 0;
        let mut restored_staging = false;

        let result = replace_generated_paths_with(&active, &prepared, |source, destination| {
            if source.starts_with(&prepared) && destination.starts_with(&active) {
                installs += 1;
                if installs == 3 {
                    anyhow::bail!("injected README install failure");
                }
            }
            if source.ends_with(".previous-active/config") {
                anyhow::bail!("injected config restore failure");
            }
            if source.ends_with(".previous-active/staging") {
                restored_staging = true;
            }
            fs::rename(source, destination).map_err(Into::into)
        });

        let error = result.expect_err("replacement must fail").to_string();
        assert!(
            restored_staging,
            "staging restoration must still be attempted"
        );
        assert!(error.contains("injected config restore failure"));
        assert!(prepared.join(".previous-active/config").exists());
    }

    fn write_replacement_fixture(root: &Path, marker: &[u8]) {
        fs::create_dir_all(root.join("staging")).expect("create staging fixture");
        fs::create_dir_all(root.join("config")).expect("create config fixture");
        fs::write(root.join("staging/marker"), marker).expect("write staging fixture");
        fs::write(root.join("config/marker"), marker).expect("write config fixture");
        fs::write(root.join("README.md"), marker).expect("write README fixture");
    }

    fn assert_replacement_fixture(root: &Path, marker: &[u8]) {
        assert_eq!(
            fs::read(root.join("staging/marker")).expect("staging"),
            marker
        );
        assert_eq!(
            fs::read(root.join("config/marker")).expect("config"),
            marker
        );
        assert_eq!(fs::read(root.join("README.md")).expect("README"), marker);
    }
}
