//! Copy staged import rows into the production tables.

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Instant;

use anyhow::{Result, bail};
use rusqlite::{Connection, Transaction, params, params_from_iter};

use crate::db::schema;
use crate::db::sql::{SQLITE_IN_CHUNK, pair_placeholders};

use super::ImportMode;

#[derive(Debug, Default)]
pub(super) struct PromoteStats {
    pub(super) conversations: u64,
    pub(super) participants: u64,
    pub(super) messages: u64,
    pub(super) attachments: u64,
    pub(super) tapbacks: u64,
    pub(super) messages_deduped: u64,
    pub(super) messages_appended: u64,
}

pub(super) fn promote_append(
    conn: &mut Connection,
    mode: ImportMode,
    account_id: &str,
    fill_content_keys: bool,
    wipe_sources: &[String],
) -> Result<PromoteStats> {
    let mut stats = PromoteStats::default();
    let started = Instant::now();

    let tx = conn.transaction()?;

    if mode == ImportMode::Replace {
        for source in wipe_sources {
            println!("  sql:      deleting existing messages for source '{source}'…");
            let _ = io::stdout().flush();
            schema::delete_messages_for_source(&tx, account_id, source)?;
        }
        if !wipe_sources.is_empty() {
            println!("  sql:      wipe complete (inside promote transaction)");
            let _ = io::stdout().flush();
        }
    }

    // Staging→prod conversation id map for set-based inserts.
    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _promote_conv_map (
            staging_id INTEGER PRIMARY KEY,
            prod_id INTEGER NOT NULL
        );
        DELETE FROM _promote_conv_map;
        "#,
    )?;

    let staging_conv_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM staging_conversations WHERE account_id = ?1",
        params![account_id],
        |r| r.get(0),
    )?;
    promote_log(format_args!(
        "{staging_conv_count} staging conversations → production…"
    ));

    let max_conv_before: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM conversations", [], |r| {
            r.get(0)
        })?;
    tx.execute(
        r#"
        INSERT INTO conversations (
            account_id, chat_handle_id, conversation_type,
            group_title, exported_at, source_file
        )
        SELECT
            account_id, chat_handle_id, conversation_type,
            group_title, exported_at, source_file
        FROM staging_conversations
        WHERE account_id = ?1
        ON CONFLICT(account_id, chat_handle_id) DO UPDATE SET
            conversation_type = excluded.conversation_type,
            group_title = COALESCE(excluded.group_title, conversations.group_title),
            exported_at = COALESCE(excluded.exported_at, conversations.exported_at),
            source_file = excluded.source_file
        "#,
        params![account_id],
    )?;
    tx.execute(
        r#"
        INSERT INTO _promote_conv_map (staging_id, prod_id)
        SELECT sc.id, c.id
        FROM staging_conversations sc
        JOIN conversations c
          ON c.account_id = sc.account_id
         AND c.chat_handle_id = sc.chat_handle_id
        WHERE sc.account_id = ?1
        "#,
        params![account_id],
    )?;
    let new_conversations: i64 = tx.query_row(
        "SELECT COUNT(*) FROM _promote_conv_map WHERE prod_id > ?1",
        params![max_conv_before],
        |r| r.get(0),
    )?;
    stats.conversations = u64::try_from(new_conversations).unwrap_or(0);
    promote_log(format_args!(
        "conversations done (new={})  ({:.1}s)",
        stats.conversations,
        started.elapsed().as_secs_f64()
    ));

    let staging_part_count: i64 = tx.query_row(
        r#"
        SELECT COUNT(*) FROM staging_participants
        WHERE conversation_id IN (
            SELECT id FROM staging_conversations WHERE account_id = ?1
        )
        "#,
        params![account_id],
        |r| r.get(0),
    )?;
    promote_log(format_args!(
        "{staging_part_count} staging participants → production…"
    ));
    stats.participants = u64::try_from(tx.execute(
        r#"
        INSERT OR IGNORE INTO participants (conversation_id, handle_id, contact_id, name_alias)
        SELECT cm.prod_id, sp.handle_id, sp.contact_id, sp.name_alias
        FROM staging_participants sp
        JOIN _promote_conv_map cm ON cm.staging_id = sp.conversation_id
        "#,
        [],
    )?)
    .unwrap_or(0);
    promote_log(format_args!(
        "participants done (new={})  ({:.1}s)",
        stats.participants,
        started.elapsed().as_secs_f64()
    ));

    let total_msgs: i64 = tx.query_row(
        r#"
        SELECT COUNT(*) FROM staging_messages
        WHERE conversation_id IN (
            SELECT id FROM staging_conversations WHERE account_id = ?1
        )
        "#,
        params![account_id],
        |r| r.get(0),
    )?;
    promote_log(format_args!(
        "{total_msgs} staging messages → production ({})…",
        mode.as_str()
    ));

    // Skip per-row full-text search trigger work during bulk message/attachment
    // inserts; index once after.
    let phase = Instant::now();
    promote_log("pausing FTS triggers…");
    schema::drop_messages_fts_triggers(&tx)?;
    promote_phase_done(started, phase, "FTS triggers paused");

    let existing_msgs: i64 = tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    let drop_secondary = should_drop_messages_secondary_indexes(total_msgs, existing_msgs);
    if drop_secondary {
        let phase = Instant::now();
        promote_log(format_args!(
            "dropping secondary message indexes (staging={total_msgs} existing={existing_msgs})…"
        ));
        schema::drop_messages_secondary_indexes(&tx)?;
        promote_phase_done(started, phase, "secondary indexes dropped");
    } else {
        promote_log(format_args!(
            "keeping secondary message indexes (staging={total_msgs} existing={existing_msgs})"
        ));
    }

    // Highest message id that exists before this promotion inserts anything
    // (after the replace-mode wipe). Every row inserted below lands above it,
    // which is how full-text search indexing tells new rows apart from the already indexed
    // rows that `_promote_msg_map` also targets for attachments and tapbacks.
    let max_msg_id_before_promote: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;

    let msg_map = promote_messages_chunked(&tx, mode, account_id, total_msgs, &mut stats, started)?;

    if drop_secondary {
        let phase = Instant::now();
        promote_log("rebuilding secondary message indexes…");
        schema::create_messages_secondary_indexes(&tx)?;
        promote_phase_done(
            started,
            phase,
            format!(
                "secondary indexes rebuilt (inserted={} skipped={})",
                stats.messages, stats.messages_deduped
            ),
        );
    } else {
        promote_log(format_args!(
            "messages done (inserted={} skipped={})  (total {:.1}s)",
            stats.messages,
            stats.messages_deduped,
            started.elapsed().as_secs_f64()
        ));
    }

    let phase = Instant::now();
    promote_log(format_args!(
        "writing message id map ({} pairs)…",
        msg_map.len()
    ));
    fill_promote_msg_map(&tx, account_id, &msg_map)?;
    promote_phase_done(started, phase, "message id map written");

    let phase = Instant::now();
    promote_log("bulk-inserting attachments…");
    let att_inserted = tx.execute(
        r#"
        INSERT INTO attachments (
            message_id, path, original_name, mime_type, is_sticker, transcription,
            sha256, assets_path, size_bytes, missing_reason
        )
        SELECT
            mm.prod_id, sa.path, sa.original_name, sa.mime_type, sa.is_sticker, sa.transcription,
            sa.sha256, sa.assets_path, sa.size_bytes, sa.missing_reason
        FROM staging_attachments sa
        JOIN _promote_msg_map mm ON mm.staging_id = sa.message_id
        WHERE NOT EXISTS (
            SELECT 1
            FROM attachments a
            WHERE a.message_id = mm.prod_id
              AND a.path IS sa.path
              AND a.original_name IS sa.original_name
              AND a.mime_type IS sa.mime_type
              AND a.is_sticker = sa.is_sticker
              AND a.transcription IS sa.transcription
              AND a.sha256 IS sa.sha256
              AND a.assets_path IS sa.assets_path
              AND a.size_bytes IS sa.size_bytes
              AND a.missing_reason IS sa.missing_reason
        )
        "#,
        [],
    )?;
    stats.attachments = att_inserted as u64;
    promote_phase_done(
        started,
        phase,
        format!("attachments done (inserted={})", stats.attachments),
    );

    let phase = Instant::now();
    promote_log("bulk-inserting tapbacks…");
    let tap_inserted = tx.execute(
        r#"
        INSERT INTO tapbacks (
            message_id, part_index, kind, emoji, is_from_me, sender_handle_id
        )
        SELECT
            mm.prod_id, st.part_index, st.kind, st.emoji, st.is_from_me, st.sender_handle_id
        FROM staging_tapbacks st
        JOIN _promote_msg_map mm ON mm.staging_id = st.message_id
        WHERE NOT EXISTS (
            SELECT 1
            FROM tapbacks t
            WHERE t.message_id = mm.prod_id
              AND t.part_index = st.part_index
              AND t.kind = st.kind
              AND t.emoji IS st.emoji
              AND t.is_from_me = st.is_from_me
              AND t.sender_handle_id IS st.sender_handle_id
        )
        "#,
        [],
    )?;
    stats.tapbacks = tap_inserted as u64;
    promote_phase_done(
        started,
        phase,
        format!("tapbacks done (inserted={})", stats.tapbacks),
    );

    let phase = Instant::now();
    promote_log("bulk-indexing FTS for new messages…");
    let fts_indexed = schema::index_messages_fts_from_promote_map(&tx, max_msg_id_before_promote)?;
    schema::install_messages_fts_triggers(&tx)?;
    promote_phase_done(
        started,
        phase,
        format!("FTS indexed={fts_indexed} (triggers restored)"),
    );

    if fill_content_keys {
        let phase = Instant::now();
        promote_log("filling content keys…");
        let keys = crate::dedupe::fill_missing_content_keys(&tx, account_id)?;
        promote_phase_done(started, phase, format!("content keys filled={keys}"));
    }

    let phase = Instant::now();
    promote_log("committing transaction…");
    tx.commit()?;
    promote_phase_done(
        started,
        phase,
        format!(
            "committed  convs={} parts={} msgs={} atts={} taps={}",
            stats.conversations,
            stats.participants,
            stats.messages,
            stats.attachments,
            stats.tapbacks
        ),
    );

    Ok(stats)
}

/// Staging rows per set-based insert window (progress plus smaller write-ahead
/// log spikes).
const PROMOTE_MESSAGE_BATCH: i64 = 10_000;
/// Pairs per multi-row INSERT into `_promote_msg_map` (SQLite default max variables is 999).
/// Drop secondary indexes only for large promotes relative to the existing table.
const PROMOTE_INDEX_DROP_MIN_STAGING: i64 = 5_000;

/// Announce a promote phase. Flushed so piped output streams during long imports.
fn promote_log(msg: impl std::fmt::Display) {
    println!("  sql:      promote: {msg}");
    let _ = io::stdout().flush();
}

fn promote_phase_done(total: Instant, phase: Instant, msg: impl std::fmt::Display) {
    promote_log(format_args!(
        "{msg}  (phase {:.1}s, total {:.1}s)",
        phase.elapsed().as_secs_f64(),
        total.elapsed().as_secs_f64()
    ));
}

fn should_drop_messages_secondary_indexes(staging_count: i64, existing_count: i64) -> bool {
    staging_count >= PROMOTE_INDEX_DROP_MIN_STAGING
        && staging_count.saturating_mul(5) >= existing_count.max(1)
}

fn promote_messages_chunked(
    tx: &Transaction<'_>,
    mode: ImportMode,
    account_id: &str,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    let bounds: (Option<i64>, Option<i64>) = tx.query_row(
        r#"
        SELECT MIN(sm.id), MAX(sm.id)
        FROM staging_messages sm
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = ?1
        "#,
        params![account_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (Some(min_id), Some(max_id)) = bounds else {
        stats.messages = 0;
        stats.messages_appended = 0;
        stats.messages_deduped = 0;
        return Ok(HashMap::new());
    };

    if mode == ImportMode::Replace {
        promote_messages_replace_chunked(tx, account_id, min_id, max_id, total_msgs, stats, started)
    } else {
        promote_messages_append_chunked(tx, account_id, min_id, max_id, total_msgs, stats, started)
    }
}

fn promote_messages_replace_chunked(
    tx: &Transaction<'_>,
    account_id: &str,
    min_id: i64,
    max_id: i64,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    let mut msg_map = HashMap::new();
    let mut max_before: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;
    let mut inserted_total = 0u64;
    let mut lo = min_id - 1;
    let mut chunk_idx = 0u32;

    while lo < max_id {
        chunk_idx += 1;
        let hi = (lo + PROMOTE_MESSAGE_BATCH).min(max_id);
        let phase = Instant::now();
        promote_log(format_args!(
            "inserting messages chunk {chunk_idx} (staging id {}..{}, replace)…",
            lo + 1,
            hi
        ));

        let inserted = tx.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, service, subject, body, is_announcement, is_reply,
                thread_originator_guid, thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender_handle_id, sm.service, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
                sm.import_id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = ?1
              AND sm.id > ?2
              AND sm.id <= ?3
            ORDER BY sm.id
            "#,
            params![account_id, lo, hi],
        )?;
        inserted_total += inserted as u64;

        let staging_ids: Vec<i64> = tx
            .prepare(
                r#"
                SELECT sm.id
                FROM staging_messages sm
                JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
                WHERE sm.account_id = ?1
                  AND sm.id > ?2
                  AND sm.id <= ?3
                ORDER BY sm.id
                "#,
            )?
            .query_map(params![account_id, lo, hi], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        max_before = zip_new_message_ids(
            tx,
            &mut msg_map,
            staging_ids,
            account_id,
            max_before,
            |n, p| {
                format!(
                    "promote replace message id map mismatch: staging={n} new_prod={p} (chunk staging id {}..{hi})",
                    lo + 1
                )
            },
        )?;

        promote_phase_done(
            started,
            phase,
            format!("chunk {chunk_idx} inserted={inserted} running={inserted_total}/{total_msgs}"),
        );
        lo = hi;
    }

    stats.messages = inserted_total;
    stats.messages_appended = inserted_total;
    Ok(msg_map)
}

fn promote_messages_append_chunked(
    tx: &Transaction<'_>,
    account_id: &str,
    min_id: i64,
    max_id: i64,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    // Append: rely on partial unique index ix_messages_account_source_guid via
    // INSERT OR IGNORE. Correlated NOT EXISTS / JOIN anti-joins mis-plan onto
    // ix_messages_source and scan the whole source (~10s+ at 50k+ rows).
    let mut msg_map = HashMap::new();
    let mut inserted_total = 0u64;
    let mut lo = min_id - 1;
    let mut chunk_idx = 0u32;

    while lo < max_id {
        chunk_idx += 1;
        let hi = (lo + PROMOTE_MESSAGE_BATCH).min(max_id);
        let phase = Instant::now();
        promote_log(format_args!(
            "inserting messages chunk {chunk_idx} (staging id {}..{}, append)…",
            lo + 1,
            hi
        ));

        let inserted = tx.execute(
            r#"
            INSERT OR IGNORE INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, service, subject, body, is_announcement, is_reply,
                thread_originator_guid, thread_originator_part, num_replies, sort_order, import_id
            )
            SELECT
                cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
                sm.sender_handle_id, sm.service, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
                sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
                sm.import_id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = ?1
              AND sm.guid IS NOT NULL
              AND sm.guid != ''
              AND sm.id > ?2
              AND sm.id <= ?3
            ORDER BY sm.id
            "#,
            params![account_id, lo, hi],
        )?;
        inserted_total += inserted as u64;

        promote_phase_done(
            started,
            phase,
            format!("chunk {chunk_idx} inserted={inserted} running={inserted_total}/{total_msgs}"),
        );
        lo = hi;
    }

    // Null/empty guids are outside the partial unique index — always insert.
    let phase = Instant::now();
    promote_log("inserting messages with empty guids…");
    let empty_max_before: i64 =
        tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?;
    let inserted_empty = tx.execute(
        r#"
        INSERT INTO messages (
            conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
            sender_handle_id, service, subject, body, is_announcement, is_reply,
            thread_originator_guid, thread_originator_part, num_replies, sort_order, import_id
        )
        SELECT
            cm.prod_id, sm.account_id, sm.source, sm.guid, sm.timestamp, sm.timestamp_utc, sm.is_from_me,
            sm.sender_handle_id, sm.service, sm.subject, sm.body, sm.is_announcement, sm.is_reply,
            sm.thread_originator_guid, sm.thread_originator_part, sm.num_replies, sm.sort_order,
            sm.import_id
        FROM staging_messages sm
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = ?1
          AND (sm.guid IS NULL OR sm.guid = '')
        ORDER BY sm.id
        "#,
        params![account_id],
    )?;
    inserted_total += inserted_empty as u64;

    let empty_staging_ids: Vec<i64> = tx
        .prepare(
            r#"
            SELECT sm.id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = ?1
              AND (sm.guid IS NULL OR sm.guid = '')
            ORDER BY sm.id
            "#,
        )?
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    zip_new_message_ids(
        tx,
        &mut msg_map,
        empty_staging_ids,
        account_id,
        empty_max_before,
        |n, p| format!("promote append empty-guid id map mismatch: staging={n} new_prod={p}"),
    )?;
    promote_phase_done(
        started,
        phase,
        format!("empty-guid messages inserted={inserted_empty}"),
    );

    stats.messages = inserted_total;
    stats.messages_appended = inserted_total;
    stats.messages_deduped = (total_msgs as u64).saturating_sub(inserted_total);
    Ok(msg_map)
}

fn zip_new_message_ids(
    tx: &Transaction<'_>,
    msg_map: &mut HashMap<i64, i64>,
    staging_ids: Vec<i64>,
    account_id: &str,
    max_before: i64,
    mismatch: impl FnOnce(usize, usize) -> String,
) -> Result<i64> {
    let prod_ids: Vec<i64> = tx
        .prepare("SELECT id FROM messages WHERE id > ?1 AND account_id = ?2 ORDER BY id")?
        .query_map(params![max_before, account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if staging_ids.len() != prod_ids.len() {
        bail!("{}", mismatch(staging_ids.len(), prod_ids.len()));
    }
    for (staging_id, prod_id) in staging_ids.into_iter().zip(prod_ids) {
        msg_map.insert(staging_id, prod_id);
    }
    Ok(tx.query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))?)
}

fn fill_promote_msg_map(
    tx: &Transaction<'_>,
    account_id: &str,
    msg_map: &HashMap<i64, i64>,
) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _promote_msg_map (
            staging_id INTEGER PRIMARY KEY,
            prod_id INTEGER NOT NULL
        );
        DELETE FROM _promote_msg_map;
        "#,
    )?;
    if !msg_map.is_empty() {
        let pairs: Vec<(i64, i64)> = msg_map.iter().map(|(&s, &p)| (s, p)).collect();
        for chunk in pairs.chunks(SQLITE_IN_CHUNK) {
            let sql = format!(
                "INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES {}",
                pair_placeholders(chunk.len())
            );
            let mut stmt = tx.prepare(&sql)?;
            let mut vals: Vec<rusqlite::types::Value> = Vec::with_capacity(chunk.len() * 2);
            for &(staging_id, prod_id) in chunk {
                vals.push(staging_id.into());
                vals.push(prod_id.into());
            }
            stmt.execute(params_from_iter(vals))?;
        }
    }

    tx.execute(
        r#"
        INSERT OR REPLACE INTO _promote_msg_map (staging_id, prod_id)
        SELECT sm.id, m.id
        FROM staging_messages sm
        JOIN messages m
          ON m.account_id = sm.account_id
         AND m.source = sm.source
         AND m.guid = sm.guid
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = ?1
          AND sm.guid IS NOT NULL
          AND sm.guid != ''
        "#,
        params![account_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    #[test]
    fn promote_message_map_ignores_other_accounts() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY,
                account_id TEXT NOT NULL
            );
            INSERT INTO messages (id, account_id) VALUES
                (1, 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'),
                (2, 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb');
            "#,
        )
        .unwrap();
        let tx = conn.transaction().unwrap();
        let mut map = HashMap::new();

        zip_new_message_ids(&tx, &mut map, vec![101], TEST_ACCOUNT, 0, |n, p| {
            format!("unexpected mapping counts: staging={n} production={p}")
        })
        .unwrap();

        assert_eq!(map, HashMap::from([(101, 1)]));
    }
}
