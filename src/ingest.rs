//! One-shot: optional CSV→vault NDJSON in staging → import → cross-source dedupe.
//!
//! Staging must already contain exporter output (CSV and/or vault NDJSON). Fill it
//! with [message-exporters](https://github.com/bitrealm-dev/message-exporters) or
//! another tool — this vault never runs exporters.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config::Config;
use crate::dedupe;
use crate::import::{self, ImportMode};

#[derive(Debug)]
pub struct IngestOptions {
    pub source_id: String,
    pub account_id: String,
    pub staging_dir: Option<PathBuf>,
    pub mode: ImportMode,
    pub overwrite_contacts: bool,
    pub skip_dedupe: bool,
    pub window_secs: i64,
}

#[derive(Debug, Default)]
pub struct IngestStats {
    pub staging_dir: PathBuf,
    pub import: import::ImportStats,
    pub dedupe: Option<dedupe::DedupeStats>,
}

/// Import from the source staging dir (optional CSV conversion), then soft-dedupe.
pub fn ingest(cfg: &Config, opts: &IngestOptions) -> Result<IngestStats> {
    let src = cfg.source(&opts.source_id)?;
    let staging = opts
        .staging_dir
        .clone()
        .unwrap_or_else(|| src.export_dir.clone());

    if !staging.is_dir() {
        bail!(
            "staging directory does not exist: {} \
             (fill it via message-exporters, then re-run ingest)",
            staging.display()
        );
    }

    let has_csv = staging_has_ext(&staging, "csv")?;
    let has_json = staging_has_ext(&staging, "json")?;
    if !has_csv && !has_json {
        bail!(
            "staging {} has no .json or .csv files to import \
             (fill it via message-exporters first)",
            staging.display()
        );
    }

    println!("Ingest source '{}'", src.id);
    println!("  account:  {}", opts.account_id);
    println!("  staging:  {}", staging.display());
    println!("  mode:     {}", opts.mode.as_str());

    if has_csv {
        println!("  phase:    csv→json");
        csv_to_ndjson_if_mapped(&staging, &src.id)?;
    }

    let assets = src.resolved_assets_dir_for_account(&cfg.paths, &opts.account_id);
    let (contacts_csv, exclude_csv) = cfg.paths.ensure_account_csvs(&opts.account_id)?;
    println!("  phase:    import → {}", cfg.paths.db.display());
    println!("  assets:   {}", assets.display());
    println!("  contacts: {}", contacts_csv.display());
    let import_stats = import::import_export(
        &staging,
        &cfg.paths.db,
        &assets,
        &contacts_csv,
        &exclude_csv,
        opts.overwrite_contacts,
        opts.mode,
        &src.id,
        &opts.account_id,
    )?;

    let dedupe_stats = if opts.skip_dedupe {
        println!("  phase:    dedupe skipped");
        None
    } else {
        println!("  phase:    cross-source dedupe");
        let priority: Vec<String> = cfg.sources.iter().map(|s| s.id.clone()).collect();
        Some(dedupe::run_dedupe(
            &cfg.paths.db,
            &opts.account_id,
            &priority,
            opts.window_secs,
        )?)
    };

    println!("  phase:    done");

    Ok(IngestStats {
        staging_dir: staging,
        import: import_stats,
        dedupe: dedupe_stats,
    })
}

/// Convert staging CSVs to vault NDJSON when a Python converter exists for `source_id`.
fn csv_to_ndjson_if_mapped(staging: &Path, source_id: &str) -> Result<()> {
    if !csv_ingest::has_converter(source_id) {
        if staging_has_ext(staging, "json")? {
            println!(
                "  csv→json: skipped (no converter for '{source_id}'; .json already present)"
            );
            return Ok(());
        }
        bail!(
            "staging has .csv but no csv-ingest converter for '{source_id}' \
             and no .json to import"
        );
    }
    let report = csv_ingest::convert_directory(staging, staging, source_id)?;
    println!(
        "  csv→json: conversations={} messages={} (source {source_id})",
        report.conversations, report.messages
    );
    if report.rows_skipped > 0 {
        println!("  csv→json: rows skipped={}", report.rows_skipped);
    }
    Ok(())
}

fn staging_has_ext(staging: &Path, ext: &str) -> Result<bool> {
    for entry in fs::read_dir(staging)
        .with_context(|| format!("failed to read staging {}", staging.display()))?
    {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, PathsConfig, SourceConfig};
    use crate::import::ImportMode;
    use crate::schema;
    use rusqlite::Connection;
    use std::io::Write;

    const TEST_ACCOUNT_ID: &str = "00000000-0000-0000-0000-000000000099";

    fn tempfile_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mv-ingest-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ingest_from_staging_ndjson_without_exporter() {
        let root = tempfile_dir("smoke");
        let staging = root.join("staging");
        let data = root.join("data");
        let config_dir = root.join("config");
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&config_dir).unwrap();

        let ndjson = concat!(
            r#"{"record":"conversation","schema":"vault","schema_version":1,"#,
            r#""chat_identifier":"+14075551234","service":"SMS","#,
            r#""conversation_type":"individual","#,
            r#""participants":[{"handle":"+14075551234","name_hint":"Alice"}],"#,
            r#""exported_at":"2024-01-01T00:00:00Z"}"#,
            "\n",
            r#"{"record":"message","guid":"ingest-smoke-001","#,
            r#""timestamp":"2021-01-01T00:00:00Z","timestamp_utc":"2021-01-01T00:00:00Z","#,
            r#""is_from_me":false,"sender":"+14075551234","service":"SMS","text":"smoke"}"#,
            "\n",
        );
        fs::write(staging.join("chat.json"), ndjson).unwrap();

        let contacts = config_dir.join("contacts.csv");
        {
            let mut f = fs::File::create(&contacts).unwrap();
            writeln!(f, "phones,first_name,last_name,exclude").unwrap();
        }
        let exclude = config_dir.join("exclude.csv");
        {
            let mut f = fs::File::create(&exclude).unwrap();
            writeln!(f, "handle").unwrap();
        }

        let db = data.join("vault.db");
        fs::create_dir_all(&data).unwrap();
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
            schema::ensure_vault_schema(&conn).unwrap();
            conn.execute(
                "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'test', 0)",
                [TEST_ACCOUNT_ID],
            )
            .unwrap();
        }

        let cfg = Config {
            owner: None,
            account: None,
            paths: PathsConfig {
                db: db.clone(),
                data_dir: data.clone(),
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
                contacts_csv: contacts,
                exclude_csv: exclude,
                export_dir: None,
            },
            sources: vec![SourceConfig {
                id: "go-sms-pro".into(),
                export_dir: staging.clone(),
                assets_dir: None,
                assets_converted_dir: None,
            }],
            server: None,
        };

        let stats = ingest(
            &cfg,
            &IngestOptions {
                source_id: "go-sms-pro".into(),
                account_id: TEST_ACCOUNT_ID.into(),
                staging_dir: None,
                mode: ImportMode::Replace,
                overwrite_contacts: false,
                skip_dedupe: true,
                window_secs: 2,
            },
        )
        .expect("ingest should succeed with staging NDJSON only");

        assert_eq!(stats.import.messages, 1);
        assert_eq!(stats.staging_dir, staging);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn staging_empty_fails_clearly() {
        let root = tempfile_dir("empty");
        let staging = root.join("staging");
        fs::create_dir_all(&staging).unwrap();
        let cfg = Config {
            owner: None,
            account: None,
            paths: PathsConfig {
                db: root.join("vault.db"),
                data_dir: root.join("data"),
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
                contacts_csv: root.join("contacts.csv"),
                exclude_csv: root.join("exclude.csv"),
                export_dir: None,
            },
            server: None,
            sources: vec![SourceConfig {
                id: "imessage".into(),
                export_dir: staging,
                assets_dir: None,
                assets_converted_dir: None,
            }],
        };
        let err = ingest(
            &cfg,
            &IngestOptions {
                source_id: "imessage".into(),
                account_id: TEST_ACCOUNT_ID.into(),
                staging_dir: None,
                mode: ImportMode::Replace,
                overwrite_contacts: false,
                skip_dedupe: true,
                window_secs: 2,
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("no .json or .csv"),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
