//! Copy staged import rows into the production tables.

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Instant;

use anyhow::{Result, bail};
use sqlx::AnyConnection;
use sqlx::Connection;

use crate::db::dialect;
use crate::db::engine::DbEngine;
use crate::db::schema;
use crate::db::sql::SQLITE_IN_CHUNK;

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

pub(super) async fn promote_append(
    conn: &mut AnyConnection,
    mode: ImportMode,
    account_id: &str,
    fill_content_keys: bool,
    wipe_sources: &[String],
) -> Result<PromoteStats> {
    let mut stats = PromoteStats::default();
    let started = Instant::now();

    let mut tx = conn.begin().await?;

    if mode == ImportMode::Replace {
        for source in wipe_sources {
            println!("  sql:      deleting existing messages for source '{source}'…");
            let _ = io::stdout().flush();
            schema::delete_messages_for_source(&mut tx, account_id, source).await?;
        }
        if !wipe_sources.is_empty() {
            println!("  sql:      wipe complete (inside promote transaction)");
            let _ = io::stdout().flush();
        }
    }

    // Staging→prod conversation id map for set-based inserts.
    for stmt in schema::split_ddl(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _promote_conv_map (
            staging_id INTEGER PRIMARY KEY,
            prod_id INTEGER NOT NULL
        );
        DELETE FROM _promote_conv_map;
        "#,
    ) {
        sqlx::query(&stmt).execute(&mut *tx).await?;
    }

    let staging_conv_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM staging_conversations WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&mut *tx)
            .await?;
    promote_log(format_args!(
        "{staging_conv_count} staging conversations → production…"
    ));

    let max_conv_before: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM conversations")
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        r#"
        INSERT INTO conversations (
            account_id, chat_handle_id, conversation_type,
            group_title, exported_at, source_file
        )
        SELECT
            account_id, chat_handle_id, conversation_type,
            group_title, exported_at, source_file
        FROM staging_conversations
        WHERE account_id = $1
        ON CONFLICT(account_id, chat_handle_id) DO UPDATE SET
            conversation_type = excluded.conversation_type,
            group_title = COALESCE(excluded.group_title, conversations.group_title),
            exported_at = COALESCE(excluded.exported_at, conversations.exported_at),
            source_file = excluded.source_file
        "#,
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO _promote_conv_map (staging_id, prod_id)
        SELECT sc.id, c.id
        FROM staging_conversations sc
        JOIN conversations c
          ON c.account_id = sc.account_id
         AND c.chat_handle_id = sc.chat_handle_id
        WHERE sc.account_id = $1
        "#,
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    let new_conversations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _promote_conv_map WHERE prod_id > $1")
            .bind(max_conv_before)
            .fetch_one(&mut *tx)
            .await?;
    stats.conversations = u64::try_from(new_conversations).unwrap_or(0);
    promote_log(format_args!(
        "conversations done (new={})  ({:.1}s)",
        stats.conversations,
        started.elapsed().as_secs_f64()
    ));

    let staging_part_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM staging_participants
        WHERE conversation_id IN (
            SELECT id FROM staging_conversations WHERE account_id = $1
        )
        "#,
    )
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;
    promote_log(format_args!(
        "{staging_part_count} staging participants → production…"
    ));
    stats.participants = sqlx::query(
        r#"
        INSERT INTO participants (conversation_id, handle_id, contact_id, name_alias)
        SELECT cm.prod_id, sp.handle_id, sp.contact_id, sp.name_alias
        FROM staging_participants sp
        JOIN _promote_conv_map cm ON cm.staging_id = sp.conversation_id
        ON CONFLICT DO NOTHING
        "#,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    promote_log(format_args!(
        "participants done (new={})  ({:.1}s)",
        stats.participants,
        started.elapsed().as_secs_f64()
    ));

    let total_msgs: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM staging_messages
        WHERE conversation_id IN (
            SELECT id FROM staging_conversations WHERE account_id = $1
        )
        "#,
    )
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;
    promote_log(format_args!(
        "{total_msgs} staging messages → production ({})…",
        mode.as_str()
    ));

    // Skip per-row full-text search trigger work during bulk message/attachment
    // inserts; index once after. SQLite drops the sync triggers; Postgres
    // disables every trigger on the message tables (same effect, and the
    // triggers are simply re-enabled instead of reinstalled).
    let engine = dialect::engine_of(&tx);
    let phase = Instant::now();
    promote_log("pausing FTS triggers…");
    if engine == DbEngine::Postgres {
        schema::disable_fts_triggers_pg(&mut tx).await?;
    } else {
        schema::drop_messages_fts_triggers(&mut tx).await?;
    }
    promote_phase_done(started, phase, "FTS triggers paused");

    let existing_msgs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
        .fetch_one(&mut *tx)
        .await?;
    let drop_secondary = should_drop_messages_secondary_indexes(total_msgs, existing_msgs);
    if drop_secondary {
        let phase = Instant::now();
        promote_log(format_args!(
            "dropping secondary message indexes (staging={total_msgs} existing={existing_msgs})…"
        ));
        schema::drop_messages_secondary_indexes(&mut tx).await?;
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
        sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM messages")
            .fetch_one(&mut *tx)
            .await?;

    let msg_map =
        promote_messages_chunked(&mut tx, mode, account_id, total_msgs, &mut stats, started)
            .await?;

    if drop_secondary {
        let phase = Instant::now();
        promote_log("rebuilding secondary message indexes…");
        schema::create_messages_secondary_indexes(&mut tx).await?;
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
    fill_promote_msg_map(&mut tx, account_id, &msg_map).await?;
    promote_phase_done(started, phase, "message id map written");

    let phase = Instant::now();
    promote_log("bulk-inserting attachments…");
    stats.attachments = sqlx::query(
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
              AND a.path IS NOT DISTINCT FROM sa.path
              AND a.original_name IS NOT DISTINCT FROM sa.original_name
              AND a.mime_type IS NOT DISTINCT FROM sa.mime_type
              AND a.is_sticker = sa.is_sticker
              AND a.transcription IS NOT DISTINCT FROM sa.transcription
              AND a.sha256 IS NOT DISTINCT FROM sa.sha256
              AND a.assets_path IS NOT DISTINCT FROM sa.assets_path
              AND a.size_bytes IS NOT DISTINCT FROM sa.size_bytes
              AND a.missing_reason IS NOT DISTINCT FROM sa.missing_reason
        )
        "#,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    promote_phase_done(
        started,
        phase,
        format!("attachments done (inserted={})", stats.attachments),
    );

    let phase = Instant::now();
    promote_log("bulk-inserting tapbacks…");
    stats.tapbacks = sqlx::query(
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
              AND t.emoji IS NOT DISTINCT FROM st.emoji
              AND t.is_from_me = st.is_from_me
              AND t.sender_handle_id IS NOT DISTINCT FROM st.sender_handle_id
        )
        "#,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    promote_phase_done(
        started,
        phase,
        format!("tapbacks done (inserted={})", stats.tapbacks),
    );

    let phase = Instant::now();
    promote_log("bulk-indexing FTS for new messages…");
    let fts_indexed =
        schema::index_messages_fts_from_promote_map(&mut tx, max_msg_id_before_promote).await?;
    if engine == DbEngine::Postgres {
        schema::enable_fts_triggers_pg(&mut tx).await?;
    } else {
        schema::install_messages_fts_triggers(&mut tx).await?;
    }
    promote_phase_done(
        started,
        phase,
        format!("FTS indexed={fts_indexed} (triggers restored)"),
    );

    if fill_content_keys {
        let phase = Instant::now();
        promote_log("filling content keys…");
        let keys = crate::dedupe::fill_missing_content_keys(&mut tx, account_id).await?;
        promote_phase_done(started, phase, format!("content keys filled={keys}"));
    }

    let phase = Instant::now();
    promote_log("committing transaction…");
    tx.commit().await?;
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

async fn promote_messages_chunked(
    tx: &mut AnyConnection,
    mode: ImportMode,
    account_id: &str,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    let bounds: (Option<i64>, Option<i64>) = sqlx::query_as(
        r#"
        SELECT MIN(sm.id), MAX(sm.id)
        FROM staging_messages sm
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = $1
        "#,
    )
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;
    let (Some(min_id), Some(max_id)) = bounds else {
        stats.messages = 0;
        stats.messages_appended = 0;
        stats.messages_deduped = 0;
        return Ok(HashMap::new());
    };

    if mode == ImportMode::Replace {
        promote_messages_replace_chunked(tx, account_id, min_id, max_id, total_msgs, stats, started)
            .await
    } else {
        promote_messages_append_chunked(tx, account_id, min_id, max_id, total_msgs, stats, started)
            .await
    }
}

async fn promote_messages_replace_chunked(
    tx: &mut AnyConnection,
    account_id: &str,
    min_id: i64,
    max_id: i64,
    total_msgs: i64,
    stats: &mut PromoteStats,
    started: Instant,
) -> Result<HashMap<i64, i64>> {
    let mut msg_map = HashMap::new();
    let mut max_before: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM messages")
        .fetch_one(&mut *tx)
        .await?;
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

        let inserted = sqlx::query(
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
            WHERE sm.account_id = $1
              AND sm.id > $2
              AND sm.id <= $3
            ORDER BY sm.id
            "#,
        )
        .bind(account_id)
        .bind(lo)
        .bind(hi)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        inserted_total += inserted;

        let staging_ids: Vec<i64> = sqlx::query_scalar(
            r#"
            SELECT sm.id
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = $1
              AND sm.id > $2
              AND sm.id <= $3
            ORDER BY sm.id
            "#,
        )
        .bind(account_id)
        .bind(lo)
        .bind(hi)
        .fetch_all(&mut *tx)
        .await?;
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
        )
        .await?;

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

async fn promote_messages_append_chunked(
    tx: &mut AnyConnection,
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

        let inserted = sqlx::query(
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
            WHERE sm.account_id = $1
              AND sm.guid IS NOT NULL
              AND sm.guid != ''
              AND sm.id > $2
              AND sm.id <= $3
            ORDER BY sm.id
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(account_id)
        .bind(lo)
        .bind(hi)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        inserted_total += inserted;

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
    let empty_max_before: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM messages")
        .fetch_one(&mut *tx)
        .await?;
    let inserted_empty = sqlx::query(
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
        WHERE sm.account_id = $1
          AND (sm.guid IS NULL OR sm.guid = '')
        ORDER BY sm.id
        "#,
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    inserted_total += inserted_empty;

    let empty_staging_ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT sm.id
        FROM staging_messages sm
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = $1
          AND (sm.guid IS NULL OR sm.guid = '')
        ORDER BY sm.id
        "#,
    )
    .bind(account_id)
    .fetch_all(&mut *tx)
    .await?;
    zip_new_message_ids(
        tx,
        &mut msg_map,
        empty_staging_ids,
        account_id,
        empty_max_before,
        |n, p| format!("promote append empty-guid id map mismatch: staging={n} new_prod={p}"),
    )
    .await?;
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

async fn zip_new_message_ids(
    tx: &mut AnyConnection,
    msg_map: &mut HashMap<i64, i64>,
    staging_ids: Vec<i64>,
    account_id: &str,
    max_before: i64,
    mismatch: impl FnOnce(usize, usize) -> String,
) -> Result<i64> {
    let prod_ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM messages WHERE id > $1 AND account_id = $2 ORDER BY id")
            .bind(max_before)
            .bind(account_id)
            .fetch_all(&mut *tx)
            .await?;
    if staging_ids.len() != prod_ids.len() {
        bail!("{}", mismatch(staging_ids.len(), prod_ids.len()));
    }
    for (staging_id, prod_id) in staging_ids.into_iter().zip(prod_ids) {
        msg_map.insert(staging_id, prod_id);
    }
    Ok(
        sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM messages")
            .fetch_one(&mut *tx)
            .await?,
    )
}

async fn fill_promote_msg_map(
    tx: &mut AnyConnection,
    account_id: &str,
    msg_map: &HashMap<i64, i64>,
) -> Result<()> {
    for stmt in schema::split_ddl(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _promote_msg_map (
            staging_id INTEGER PRIMARY KEY,
            prod_id INTEGER NOT NULL
        );
        DELETE FROM _promote_msg_map;
        "#,
    ) {
        sqlx::query(&stmt).execute(&mut *tx).await?;
    }
    if !msg_map.is_empty() {
        let pairs: Vec<(i64, i64)> = msg_map.iter().map(|(&s, &p)| (s, p)).collect();
        for chunk in pairs.chunks(SQLITE_IN_CHUNK) {
            // Hand-numbered `$N` pairs — sqlx Any does no placeholder rewriting.
            let mut sql =
                String::from("INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES ");
            for (i, _) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push_str(&format!("(${}, ${})", i * 2 + 1, i * 2 + 2));
            }
            let mut q = sqlx::query(&sql);
            for &(staging_id, prod_id) in chunk {
                q = q.bind(staging_id).bind(prod_id);
            }
            q.execute(&mut *tx).await?;
        }
    }

    sqlx::query(
        r#"
        INSERT INTO _promote_msg_map (staging_id, prod_id)
        SELECT sm.id, m.id
        FROM staging_messages sm
        JOIN messages m
          ON m.account_id = sm.account_id
         AND m.source = sm.source
         AND m.guid = sm.guid
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = $1
          AND sm.guid IS NOT NULL
          AND sm.guid != ''
        ON CONFLICT(staging_id) DO UPDATE SET prod_id = excluded.prod_id
        "#,
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    /// Full-text hit count under the Postgres 'simple' config.
    async fn pg_fts_hits(conn: &mut AnyConnection, needle: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE search_tsv @@ plainto_tsquery('simple', $1)",
        )
        .bind(needle)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    /// Postgres-gated: the promote path's disable→bulk-fill→enable FTS cycle
    /// on the real engine. Skips unless `MV_TEST_POSTGRES_URL` is set (CI
    /// service / `docker-compose.pg.yml`).
    #[tokio::test]
    async fn promote_fts_cycle_pg() {
        let Some(url) = crate::pg_test_url() else {
            return;
        };
        let _pg_guard = crate::PG_TEST_LOCK.lock().await;
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new()
            .connect(&url)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        // The Postgres test database is shared across runs; clear anything a
        // previous run left behind (the account FKs cascade). The username is
        // distinct from the other gated tests' 'alice' because the
        // case-insensitive username index is database-global.
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();
        // The shared identity sequence may hand out message ids inside the
        // search-parity corpus range (keys 1..=15): that test binds its keys
        // as explicit ids, so clear the range (the PG_TEST_LOCK above
        // serializes us against the other gated tests).
        sqlx::query("DELETE FROM messages WHERE id BETWEEN 1 AND 15")
            .execute(&mut *conn)
            .await
            .unwrap();

        // One account + handle + conversation, and a pre-existing message
        // below the promote watermark, indexed by the insert trigger.
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'promote-alice')")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone')
             RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let conversation_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type,
                group_title, exported_at, source_file
            ) VALUES ($1, $2, 'individual', NULL, NULL, 'promote.json')
            RETURNING id
            "#,
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let carriedover_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'carriedover', '2020-01-01T00:00:00Z', 0, 0, 'carriedover')
            RETURNING id
            "#,
        )
        .bind(conversation_id)
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(pg_fts_hits(&mut conn, "carriedover").await, 1);

        // ── The promote window, driven directly (this is exactly what
        // promote_append does between its staging inserts and the bulk fill):
        // all six by-name ALTERs execute — any wrong trigger name fails here.
        schema::disable_fts_triggers_pg(&mut conn).await.unwrap();

        // FK constraint triggers stay enabled during the window: an
        // attachment pointing at a missing message must fail loudly.
        let fk_err = sqlx::query(
            "INSERT INTO attachments (message_id, original_name)
             VALUES (99999999, 'dangling.jpg')",
        )
        .execute(&mut *conn)
        .await
        .unwrap_err();
        assert!(
            format!("{fk_err}").contains("foreign key"),
            "FK violation must fail loudly while the FTS triggers are disabled: {fk_err}"
        );

        // Raw inserts during the window skip per-row FTS work.
        let fresh_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'freshbody', '2020-01-01T00:00:00Z', 0, 0, 'freshbody')
            RETURNING id
            "#,
        )
        .bind(conversation_id)
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let unindexed: i64 = sqlx::query_scalar(
            "SELECT CASE WHEN search_tsv IS NULL THEN 1 ELSE 0 END FROM messages WHERE id = $1",
        )
        .bind(fresh_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            unindexed, 1,
            "raw insert during the disabled window must leave search_tsv NULL"
        );

        // The promote-map bulk fill touches exactly the rows above the
        // watermark (the temp map as promote fills it: staging id → prod id).
        sqlx::query(
            "CREATE TEMP TABLE IF NOT EXISTS _promote_msg_map (
                 staging_id INTEGER PRIMARY KEY,
                 prod_id INTEGER NOT NULL
             )",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query("DELETE FROM _promote_msg_map")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES ($1, $2), ($3, $4)")
            .bind(1i64)
            .bind(carriedover_id)
            .bind(2i64)
            .bind(fresh_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let indexed = schema::index_messages_fts_from_promote_map(&mut conn, carriedover_id)
            .await
            .unwrap();
        assert_eq!(
            indexed, 1,
            "bulk fill must index exactly the rows above the watermark"
        );
        assert_eq!(pg_fts_hits(&mut conn, "carriedover").await, 1);
        assert_eq!(pg_fts_hits(&mut conn, "freshbody").await, 1);

        // ── Enable restores the triggers: a post-enable insert is indexed.
        schema::enable_fts_triggers_pg(&mut conn).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'postenable', '2020-01-01T00:00:00Z', 0, 0, 'postenable')
            "#,
        )
        .bind(conversation_id)
        .bind(TEST_ACCOUNT)
        .execute(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            pg_fts_hits(&mut conn, "postenable").await,
            1,
            "insert trigger must fire again after enable"
        );

        // ── The promote branch end-to-end: staging rows → promote_append →
        // the bulk fill indexes the promoted rows above the watermark.
        let staged_handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550200', '+15555550200', 'phone', 'phone')
             RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let staging_conv_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO staging_conversations (
                account_id, chat_handle_id, conversation_type, source_file
            ) VALUES ($1, $2, 'individual', 'staged.json')
            RETURNING id
            "#,
        )
        .bind(TEST_ACCOUNT)
        .bind(staged_handle_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO staging_messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'staged-guid-1', '2020-01-01T00:00:00Z', 0, 0, 'stagedbody')
            "#,
        )
        .bind(staging_conv_id)
        .bind(TEST_ACCOUNT)
        .execute(&mut *conn)
        .await
        .unwrap();
        let stats = promote_append(&mut conn, ImportMode::Append, TEST_ACCOUNT, false, &[])
            .await
            .unwrap();
        assert_eq!(stats.messages, 1, "one staged message must promote");
        assert_eq!(
            pg_fts_hits(&mut conn, "stagedbody").await,
            1,
            "promoted rows above the watermark must be indexed"
        );

        // And the triggers still fire for brand-new rows after the promote.
        sqlx::query(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'afterpromote', '2020-01-01T00:00:00Z', 0, 0, 'afterpromote')
            "#,
        )
        .bind(conversation_id)
        .bind(TEST_ACCOUNT)
        .execute(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            pg_fts_hits(&mut conn, "afterpromote").await,
            1,
            "triggers must fire again after the promote cycle"
        );
    }

    #[tokio::test]
    async fn promote_message_map_ignores_other_accounts() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
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
        .execute(&mut *conn)
        .await
        .unwrap();
        let mut tx = conn.begin().await.unwrap();
        let mut map = HashMap::new();

        zip_new_message_ids(&mut tx, &mut map, vec![101], TEST_ACCOUNT, 0, |n, p| {
            format!("unexpected mapping counts: staging={n} production={p}")
        })
        .await
        .unwrap();

        assert_eq!(map, HashMap::from([(101, 1)]));
    }
}
