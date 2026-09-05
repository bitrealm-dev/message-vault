//! Copy staged import rows into the production tables.

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Instant;

use anyhow::{Result, bail};
use sqlx::Connection;
use sqlx::{Any, AnyConnection, Transaction};

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

/// Move the account's staged rows into the production tables, wiping `wipe_sources` first
/// in replace mode. Returns the promoted counts.
///
/// Everything runs in one transaction, taken with the write lock up front on
/// SQLite (IMMEDIATE) so two imports for different accounts cannot race into
/// SQLITE_BUSY at the first write; Postgres has no statement-level equivalent.
pub(super) async fn promote_append(
    conn: &mut AnyConnection,
    mode: ImportMode,
    account_id: &str,
    fill_content_keys: bool,
    wipe_sources: &[String],
) -> Result<PromoteStats> {
    // Stats on already-committed rows so this promote's guid join can use
    // the indexes. Failure is a warning; the import still runs.
    dialect::analyze_import_tables(conn).await;
    let engine = dialect::engine_of(conn);
    let tx = conn
        .begin_with(dialect::begin_immediate_sql(engine))
        .await?;
    let mut promote = Promote {
        tx,
        account_id,
        mode,
        engine,
        stats: PromoteStats::default(),
        started: Instant::now(),
    };
    if mode == ImportMode::Replace {
        promote.wipe_sources(wipe_sources).await?;
    }
    promote.promote_conversations().await?;
    promote.promote_participants().await?;
    let messages_before = promote.promote_messages().await?;
    promote.promote_attachments().await?;
    promote.promote_tapbacks().await?;
    promote.index_fts(messages_before).await?;
    if fill_content_keys {
        promote.fill_content_keys().await?;
    }
    promote.commit().await
}

/// One promotion in progress: the transaction it runs in, the account, and
/// the counts so far. Each phase is a method; they run in the order
/// [`promote_append`] calls them because later phases read the temp id maps
/// earlier ones write.
struct Promote<'a> {
    tx: Transaction<'a, Any>,
    account_id: &'a str,
    mode: ImportMode,
    engine: DbEngine,
    stats: PromoteStats,
    /// When the promotion began, for the total in every phase's log line.
    started: Instant,
}

/// Messages are inserted in staging-id ranges this wide, so one statement never
/// holds the whole batch and progress is visible on large imports.
const PROMOTE_MESSAGE_BATCH: i64 = 50_000;
/// Below this many staged messages the secondary indexes are cheaper to keep than to rebuild.
const PROMOTE_INDEX_DROP_MIN_STAGING: i64 = 5_000;

/// The column list and SELECT every staged-message insert shares; the caller
/// adds its WHERE tail and ordering. Rows are inserted in staging id order so
/// the production ids follow it, which the id-map zip relies on.
const INSERT_MESSAGES_FROM_STAGING: &str = r#"
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
"#;

/// The staged message ids the inserts above select, in the same order; the
/// caller adds the same WHERE tail.
const STAGED_MESSAGE_IDS: &str = r#"
        SELECT sm.id
        FROM staging_messages sm
        JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
        WHERE sm.account_id = $1
"#;

const IN_ID_RANGE: &str = " AND sm.id > $2 AND sm.id <= $3 ORDER BY sm.id";
const WITH_GUID_IN_ID_RANGE: &str =
    " AND sm.guid IS NOT NULL AND sm.guid != '' AND sm.id > $2 AND sm.id <= $3 ORDER BY sm.id";
const WITHOUT_GUID: &str = " AND (sm.guid IS NULL OR sm.guid = '') ORDER BY sm.id";

impl Promote<'_> {
    /// Log the start of a phase and return its clock.
    fn begin(&self, msg: impl std::fmt::Display) -> Instant {
        promote_log(msg);
        Instant::now()
    }

    /// Log the end of a phase with its own and the total elapsed time.
    fn done(&self, phase: Instant, msg: impl std::fmt::Display) {
        promote_phase_done(self.started, phase, msg);
    }

    /// `SELECT COALESCE(MAX(id), 0) FROM messages`: the watermark new rows land above.
    async fn max_message_id(&mut self) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM messages")
                .fetch_one(&mut *self.tx)
                .await?,
        )
    }

    /// Create, or empty, a temp table mapping staging ids to production ids.
    async fn reset_id_map(&mut self, table: &str) -> Result<()> {
        for stmt in schema::split_ddl(&format!(
            "CREATE TEMP TABLE IF NOT EXISTS {table} (staging_id BIGINT PRIMARY KEY, prod_id BIGINT NOT NULL); DELETE FROM {table};"
        )) {
            sqlx::query(&stmt).execute(&mut *self.tx).await?;
        }
        Ok(())
    }

    /// Replace mode: delete the account's existing rows for each source before
    /// anything is promoted, inside the same transaction as the inserts.
    async fn wipe_sources(&mut self, sources: &[String]) -> Result<()> {
        for source in sources {
            println!("  sql:      deleting existing messages for source '{source}'…");
            let _ = io::stdout().flush();
            schema::delete_messages_for_source(&mut self.tx, self.account_id, source).await?;
        }
        if !sources.is_empty() {
            println!("  sql:      wipe complete (inside promote transaction)");
            let _ = io::stdout().flush();
        }
        Ok(())
    }

    /// Upsert the staged conversations and write `_promote_conv_map`, the
    /// staging-to-production id map every later phase joins through.
    async fn promote_conversations(&mut self) -> Result<()> {
        self.reset_id_map("_promote_conv_map").await?;
        let staged: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM staging_conversations WHERE account_id = $1")
                .bind(self.account_id)
                .fetch_one(&mut *self.tx)
                .await?;
        let phase = self.begin(format_args!("{staged} staging conversations → production…"));
        let max_before: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM conversations")
            .fetch_one(&mut *self.tx)
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
        .bind(self.account_id)
        .execute(&mut *self.tx)
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
        .bind(self.account_id)
        .execute(&mut *self.tx)
        .await?;
        let new_conversations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _promote_conv_map WHERE prod_id > $1")
                .bind(max_before)
                .fetch_one(&mut *self.tx)
                .await?;
        self.stats.conversations = u64::try_from(new_conversations).unwrap_or(0);
        self.done(
            phase,
            format!("conversations done (new={})", self.stats.conversations),
        );
        Ok(())
    }

    /// Insert the staged participants under their production conversations.
    async fn promote_participants(&mut self) -> Result<()> {
        let staged: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM staging_participants
            WHERE conversation_id IN (
                SELECT id FROM staging_conversations WHERE account_id = $1
            )
            "#,
        )
        .bind(self.account_id)
        .fetch_one(&mut *self.tx)
        .await?;
        let phase = self.begin(format_args!("{staged} staging participants → production…"));
        self.stats.participants = sqlx::query(
            r#"
            INSERT INTO participants (conversation_id, handle_id, contact_id, name_alias)
            SELECT cm.prod_id, sp.handle_id, sp.contact_id, sp.name_alias
            FROM staging_participants sp
            JOIN _promote_conv_map cm ON cm.staging_id = sp.conversation_id
            ON CONFLICT DO NOTHING
            "#,
        )
        .execute(&mut *self.tx)
        .await?
        .rows_affected();
        self.done(
            phase,
            format!("participants done (new={})", self.stats.participants),
        );
        Ok(())
    }

    /// Insert the staged messages with the FTS triggers paused and, for a
    /// large batch, the secondary indexes dropped and rebuilt; then write
    /// `_promote_msg_map` for the child rows. Returns the highest message id
    /// that existed before the insert: every new row lands above it, which
    /// is how [`Self::index_fts`] tells new rows apart from already indexed
    /// ones that the map also names.
    async fn promote_messages(&mut self) -> Result<i64> {
        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM staging_messages
            WHERE conversation_id IN (
                SELECT id FROM staging_conversations WHERE account_id = $1
            )
            "#,
        )
        .bind(self.account_id)
        .fetch_one(&mut *self.tx)
        .await?;
        promote_log(format_args!(
            "{total} staging messages → production ({})…",
            self.mode.as_str()
        ));
        self.pause_fts_triggers().await?;

        let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&mut *self.tx)
            .await?;
        let rebuild_indexes = should_drop_messages_secondary_indexes(total, existing);
        if rebuild_indexes {
            let phase = self.begin(format_args!(
                "dropping secondary message indexes (staging={total} existing={existing})…"
            ));
            schema::drop_messages_secondary_indexes(&mut self.tx).await?;
            self.done(phase, "secondary indexes dropped");
        } else {
            promote_log(format_args!(
                "keeping secondary message indexes (staging={total} existing={existing})"
            ));
        }

        let messages_before = self.max_message_id().await?;
        let msg_map = self.insert_messages(total).await?;

        if rebuild_indexes {
            let phase = self.begin("rebuilding secondary message indexes…");
            schema::create_messages_secondary_indexes(&mut self.tx).await?;
            self.done(
                phase,
                format!(
                    "secondary indexes rebuilt (inserted={} skipped={})",
                    self.stats.messages, self.stats.messages_deduped
                ),
            );
        } else {
            promote_log(format_args!(
                "messages done (inserted={} skipped={})  (total {:.1}s)",
                self.stats.messages,
                self.stats.messages_deduped,
                self.started.elapsed().as_secs_f64()
            ));
        }

        let phase = self.begin(format_args!(
            "writing message id map ({} pairs)…",
            msg_map.len()
        ));
        self.fill_message_id_map(&msg_map).await?;
        self.done(phase, "message id map written");
        Ok(messages_before)
    }

    /// Skip per-row full-text search trigger work during the bulk inserts;
    /// [`Self::index_fts`] indexes once after. SQLite drops the sync triggers;
    /// Postgres disables every trigger on the message tables (same effect,
    /// and the triggers are simply re-enabled instead of reinstalled).
    async fn pause_fts_triggers(&mut self) -> Result<()> {
        let phase = self.begin("pausing FTS triggers…");
        if self.engine == DbEngine::Postgres {
            schema::disable_fts_triggers_pg(&mut self.tx).await?;
        } else {
            schema::drop_messages_fts_triggers(&mut self.tx).await?;
        }
        self.done(phase, "FTS triggers paused");
        Ok(())
    }

    /// Insert the staged messages in id-range chunks and return the
    /// staging-to-production id map. Nothing staged means nothing inserted.
    async fn insert_messages(&mut self, total: i64) -> Result<HashMap<i64, i64>> {
        let bounds: (Option<i64>, Option<i64>) = sqlx::query_as(
            r#"
            SELECT MIN(sm.id), MAX(sm.id)
            FROM staging_messages sm
            JOIN _promote_conv_map cm ON cm.staging_id = sm.conversation_id
            WHERE sm.account_id = $1
            "#,
        )
        .bind(self.account_id)
        .fetch_one(&mut *self.tx)
        .await?;
        let (Some(min_id), Some(max_id)) = bounds else {
            self.stats.messages = 0;
            self.stats.messages_appended = 0;
            self.stats.messages_deduped = 0;
            return Ok(HashMap::new());
        };
        if self.mode == ImportMode::Replace {
            self.insert_messages_replace(min_id, max_id, total).await
        } else {
            self.insert_messages_append(min_id, max_id, total).await
        }
    }

    /// Replace mode: every staged row is new, so each chunk is inserted and
    /// its production ids zipped onto the staged ids straight away.
    async fn insert_messages_replace(
        &mut self,
        min_id: i64,
        max_id: i64,
        total: i64,
    ) -> Result<HashMap<i64, i64>> {
        let mut msg_map = HashMap::new();
        let mut max_before = self.max_message_id().await?;
        let mut inserted_total = 0u64;
        for (chunk, lo, hi) in message_chunks(min_id, max_id) {
            let phase = self.begin(format_args!(
                "inserting messages chunk {chunk} (staging id {}..{hi}, replace)…",
                lo + 1
            ));
            let inserted = self.insert_messages_in_range(IN_ID_RANGE, lo, hi).await?;
            inserted_total += inserted;
            let staged = self.staged_message_ids_in_range(lo, hi).await?;
            max_before = self
                .zip_new_message_ids(&mut msg_map, staged, max_before, |n, p| {
                    format!(
                        "promote replace message id map mismatch: staging={n} new_prod={p} (chunk staging id {}..{hi})",
                        lo + 1
                    )
                })
                .await?;
            self.done(
                phase,
                format!("chunk {chunk} inserted={inserted} running={inserted_total}/{total}"),
            );
        }
        self.stats.messages = inserted_total;
        self.stats.messages_appended = inserted_total;
        Ok(msg_map)
    }

    /// Append mode: rows production already has are skipped through the
    /// partial unique index `ix_messages_account_source_guid` with
    /// `ON CONFLICT DO NOTHING`. (Correlated NOT EXISTS / JOIN anti-joins
    /// mis-plan onto `ix_messages_source` and scan the whole source, 10s+ at
    /// 50k rows.) Rows without a guid are outside that index and are always
    /// inserted, then zipped onto their staged ids; guid rows are mapped by
    /// the guid join in [`Self::fill_message_id_map`].
    async fn insert_messages_append(
        &mut self,
        min_id: i64,
        max_id: i64,
        total: i64,
    ) -> Result<HashMap<i64, i64>> {
        let mut msg_map = HashMap::new();
        let mut inserted_total = 0u64;
        for (chunk, lo, hi) in message_chunks(min_id, max_id) {
            let phase = self.begin(format_args!(
                "inserting messages chunk {chunk} (staging id {}..{hi}, append)…",
                lo + 1
            ));
            let sql = format!(
                "{INSERT_MESSAGES_FROM_STAGING}{WITH_GUID_IN_ID_RANGE} ON CONFLICT DO NOTHING"
            );
            let inserted = sqlx::query(&sql)
                .bind(self.account_id)
                .bind(lo)
                .bind(hi)
                .execute(&mut *self.tx)
                .await?
                .rows_affected();
            inserted_total += inserted;
            self.done(
                phase,
                format!("chunk {chunk} inserted={inserted} running={inserted_total}/{total}"),
            );
        }

        let phase = self.begin("inserting messages with empty guids…");
        let max_before = self.max_message_id().await?;
        let sql = format!("{INSERT_MESSAGES_FROM_STAGING}{WITHOUT_GUID}");
        let inserted_empty = sqlx::query(&sql)
            .bind(self.account_id)
            .execute(&mut *self.tx)
            .await?
            .rows_affected();
        inserted_total += inserted_empty;
        let sql = format!("{STAGED_MESSAGE_IDS}{WITHOUT_GUID}");
        let staged: Vec<i64> = sqlx::query_scalar(&sql)
            .bind(self.account_id)
            .fetch_all(&mut *self.tx)
            .await?;
        self.zip_new_message_ids(&mut msg_map, staged, max_before, |n, p| {
            format!("promote append empty-guid id map mismatch: staging={n} new_prod={p}")
        })
        .await?;
        self.done(
            phase,
            format!("empty-guid messages inserted={inserted_empty}"),
        );

        self.stats.messages = inserted_total;
        self.stats.messages_appended = inserted_total;
        self.stats.messages_deduped = (total as u64).saturating_sub(inserted_total);
        Ok(msg_map)
    }

    /// Insert the staged messages with ids in `lo..=hi` that match `tail`.
    async fn insert_messages_in_range(&mut self, tail: &str, lo: i64, hi: i64) -> Result<u64> {
        let sql = format!("{INSERT_MESSAGES_FROM_STAGING}{tail}");
        Ok(sqlx::query(&sql)
            .bind(self.account_id)
            .bind(lo)
            .bind(hi)
            .execute(&mut *self.tx)
            .await?
            .rows_affected())
    }

    /// The staged message ids in `lo..=hi`, in id order.
    async fn staged_message_ids_in_range(&mut self, lo: i64, hi: i64) -> Result<Vec<i64>> {
        let sql = format!("{STAGED_MESSAGE_IDS}{IN_ID_RANGE}");
        Ok(sqlx::query_scalar(&sql)
            .bind(self.account_id)
            .bind(lo)
            .bind(hi)
            .fetch_all(&mut *self.tx)
            .await?)
    }

    /// Pair the staged ids just promoted with the production ids that appeared
    /// above `max_before`, in order, and add them to the map. Returns the new
    /// highest message id, the next chunk's watermark.
    ///
    /// # Errors
    ///
    /// Returns the `mismatch` message when the two counts differ, which would
    /// mean rows were inserted out of staging order or by someone else.
    async fn zip_new_message_ids(
        &mut self,
        msg_map: &mut HashMap<i64, i64>,
        staged: Vec<i64>,
        max_before: i64,
        mismatch: impl FnOnce(usize, usize) -> String,
    ) -> Result<i64> {
        let prod_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM messages WHERE id > $1 AND account_id = $2 ORDER BY id",
        )
        .bind(max_before)
        .bind(self.account_id)
        .fetch_all(&mut *self.tx)
        .await?;
        if staged.len() != prod_ids.len() {
            bail!("{}", mismatch(staged.len(), prod_ids.len()));
        }
        msg_map.extend(staged.into_iter().zip(prod_ids));
        self.max_message_id().await
    }

    /// Load the staging-to-production message id map into `_promote_msg_map`
    /// for the SQL that rewrites child rows: the zipped pairs first, then
    /// every guid row by joining production on `(account, source, guid)`,
    /// which also maps the append-mode rows that were skipped as duplicates.
    async fn fill_message_id_map(&mut self, msg_map: &HashMap<i64, i64>) -> Result<()> {
        self.reset_id_map("_promote_msg_map").await?;
        let pairs: Vec<(i64, i64)> = msg_map.iter().map(|(&s, &p)| (s, p)).collect();
        for chunk in pairs.chunks(SQLITE_IN_CHUNK) {
            // Hand-numbered `$N` pairs — sqlx Any does no placeholder rewriting.
            let values: Vec<String> = (0..chunk.len())
                .map(|i| format!("(${}, ${})", i * 2 + 1, i * 2 + 2))
                .collect();
            let sql = format!(
                "INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES {}",
                values.join(",")
            );
            let mut q = sqlx::query(&sql);
            for &(staging_id, prod_id) in chunk {
                q = q.bind(staging_id).bind(prod_id);
            }
            q.execute(&mut *self.tx).await?;
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
        .bind(self.account_id)
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }

    /// Insert the staged attachments under their production messages,
    /// skipping any row production already has field for field.
    async fn promote_attachments(&mut self) -> Result<()> {
        let phase = self.begin("bulk-inserting attachments…");
        self.stats.attachments = sqlx::query(
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
        .execute(&mut *self.tx)
        .await?
        .rows_affected();
        self.done(
            phase,
            format!("attachments done (inserted={})", self.stats.attachments),
        );
        Ok(())
    }

    /// Insert the staged tapbacks under their production messages, skipping
    /// any row production already has field for field.
    async fn promote_tapbacks(&mut self) -> Result<()> {
        let phase = self.begin("bulk-inserting tapbacks…");
        self.stats.tapbacks = sqlx::query(
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
        .execute(&mut *self.tx)
        .await?
        .rows_affected();
        self.done(
            phase,
            format!("tapbacks done (inserted={})", self.stats.tapbacks),
        );
        Ok(())
    }

    /// Index the new messages (those above `messages_before`) for full-text
    /// search in one pass, then put the per-row triggers back.
    async fn index_fts(&mut self, messages_before: i64) -> Result<()> {
        let phase = self.begin("bulk-indexing FTS for new messages…");
        let indexed =
            schema::index_messages_fts_from_promote_map(&mut self.tx, messages_before).await?;
        if self.engine == DbEngine::Postgres {
            schema::enable_fts_triggers_pg(&mut self.tx).await?;
        } else {
            schema::install_messages_fts_triggers(&mut self.tx).await?;
        }
        self.done(phase, format!("FTS indexed={indexed} (triggers restored)"));
        Ok(())
    }

    /// Fill the dedupe content keys the new rows are missing.
    async fn fill_content_keys(&mut self) -> Result<()> {
        let phase = self.begin("filling content keys…");
        let keys = crate::dedupe::fill_missing_content_keys(&mut self.tx, self.account_id).await?;
        self.done(phase, format!("content keys filled={keys}"));
        Ok(())
    }

    /// Commit and return the counts.
    async fn commit(self) -> Result<PromoteStats> {
        let phase = self.begin("committing transaction…");
        let Promote {
            tx, stats, started, ..
        } = self;
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
}

/// The `(chunk number, lo exclusive, hi inclusive)` id ranges that cover
/// `min_id..=max_id` in batches of [`PROMOTE_MESSAGE_BATCH`].
fn message_chunks(min_id: i64, max_id: i64) -> impl Iterator<Item = (u32, i64, i64)> {
    let mut lo = min_id - 1;
    let mut chunk = 0u32;
    std::iter::from_fn(move || {
        if lo >= max_id {
            return None;
        }
        chunk += 1;
        let hi = (lo + PROMOTE_MESSAGE_BATCH).min(max_id);
        let range = (chunk, lo, hi);
        lo = hi;
        Some(range)
    })
}

/// One promote progress line, flushed so it shows while the next statement runs.
fn promote_log(msg: impl std::fmt::Display) {
    println!("  sql:      promote: {msg}");
    let _ = io::stdout().flush();
}

/// Log the end of one promote phase with its own and the total elapsed time.
fn promote_phase_done(total: Instant, phase: Instant, msg: impl std::fmt::Display) {
    promote_log(format_args!(
        "{msg}  (phase {:.1}s, total {:.1}s)",
        phase.elapsed().as_secs_f64(),
        total.elapsed().as_secs_f64()
    ));
}

/// True when the staged batch is large relative to the table, so dropping and rebuilding
/// the secondary indexes beats maintaining them row by row.
fn should_drop_messages_secondary_indexes(staging_count: i64, existing_count: i64) -> bool {
    staging_count >= PROMOTE_INDEX_DROP_MIN_STAGING
        && staging_count.saturating_mul(5) >= existing_count.max(1)
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

    /// Manual ANALYZE count and last_analyze on public.messages.
    async fn pg_messages_analyze_stat(conn: &mut AnyConnection) -> (i64, Option<String>) {
        let analyze_count: i64 = sqlx::query_scalar(
            "SELECT analyze_count FROM pg_stat_user_tables
             WHERE schemaname = 'public' AND relname = 'messages'",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let last_analyze: Option<String> = sqlx::query_scalar(
            "SELECT last_analyze::text FROM pg_stat_user_tables
             WHERE schemaname = 'public' AND relname = 'messages'",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        (analyze_count, last_analyze)
    }

    /// Postgres-gated: the promote path's disable→bulk-fill→enable FTS cycle
    /// on the real engine. Skips unless `MV_TEST_POSTGRES_URL` is set (CI
    /// service / `docker-compose.pg.yml`).
    #[tokio::test]
    async fn promote_fts_cycle_pg() {
        let Some(url) = crate::pg_test_url() else {
            return;
        };
        let _pg_guard = crate::acquire_pg_test_lock().await;
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
                 staging_id BIGINT PRIMARY KEY,
                 prod_id BIGINT NOT NULL
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
        let (analyze_before, last_analyze_before) = pg_messages_analyze_stat(&mut conn).await;
        let stats = promote_append(&mut conn, ImportMode::Append, TEST_ACCOUNT, false, &[])
            .await
            .unwrap();
        assert_eq!(stats.messages, 1, "one staged message must promote");
        let (analyze_after, last_analyze) = pg_messages_analyze_stat(&mut conn).await;
        assert!(
            analyze_after > analyze_before,
            "ANALYZE before BEGIN must increment analyze_count on messages (before={analyze_before}, after={analyze_after}, last_analyze_before={last_analyze_before:?})"
        );
        assert!(
            last_analyze.is_some(),
            "ANALYZE before BEGIN must set last_analyze on messages"
        );
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
    async fn promote_analyzes_import_tables_before_begin() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'promote-analyze')")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550300', '+15555550300', 'phone', 'phone')
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
            ) VALUES ($1, $2, 'individual', 'analyze.json')
            RETURNING id
            "#,
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO staging_messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'analyze-guid-1', '2020-01-01T00:00:00Z', 0, 0, 'analyzebody')
            "#,
        )
        .bind(staging_conv_id)
        .bind(TEST_ACCOUNT)
        .execute(&mut *conn)
        .await
        .unwrap();
        let first = promote_append(&mut conn, ImportMode::Append, TEST_ACCOUNT, false, &[])
            .await
            .unwrap();
        assert_eq!(first.messages, 1);
        let stat_rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_stat1 WHERE tbl IN ('messages', 'attachments', 'tapbacks')",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert!(
            stat_rows >= 1,
            "ANALYZE before BEGIN must write sqlite_stat1 for import tables"
        );

        sqlx::query(
            r#"
            INSERT INTO staging_messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'analyze-guid-2', '2020-01-01T00:00:01Z', 0, 1, 'second')
            "#,
        )
        .bind(staging_conv_id)
        .bind(TEST_ACCOUNT)
        .execute(&mut *conn)
        .await
        .unwrap();
        let second = promote_append(&mut conn, ImportMode::Append, TEST_ACCOUNT, false, &[])
            .await
            .unwrap();
        assert_eq!(second.messages, 1, "second promote must still insert");
    }

    /// Postgres-gated: ANALYZE on the shared database must change
    /// last_analyze before a second promote begins (stop/restart in miniature).
    #[tokio::test]
    async fn promote_analyzes_import_tables_before_begin_pg() {
        let Some(url) = crate::pg_test_url() else {
            return;
        };
        let _pg_guard = crate::acquire_pg_test_lock().await;
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new()
            .connect(&url)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'promote-analyze-pg')")
            .bind(TEST_ACCOUNT)
            .execute(&mut *conn)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550400', '+15555550400', 'phone', 'phone')
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
            ) VALUES ($1, $2, 'individual', 'analyze-pg.json')
            RETURNING id
            "#,
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO staging_messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'analyze-pg-guid-1', '2020-01-01T00:00:00Z', 0, 0, 'analyzebody')
            "#,
        )
        .bind(staging_conv_id)
        .bind(TEST_ACCOUNT)
        .execute(&mut *conn)
        .await
        .unwrap();
        let (analyze_before, last_analyze_before) = pg_messages_analyze_stat(&mut conn).await;
        let first = promote_append(&mut conn, ImportMode::Append, TEST_ACCOUNT, false, &[])
            .await
            .unwrap();
        assert_eq!(first.messages, 1);
        let (analyze_after, last_analyze) = pg_messages_analyze_stat(&mut conn).await;
        assert!(
            analyze_after > analyze_before,
            "ANALYZE before BEGIN must increment analyze_count on messages (before={analyze_before}, after={analyze_after}, last_analyze_before={last_analyze_before:?})"
        );
        assert!(
            last_analyze.is_some(),
            "ANALYZE before BEGIN must set last_analyze on messages before a second promote"
        );

        sqlx::query(
            r#"
            INSERT INTO staging_messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body
            ) VALUES ($1, $2, 'sms', 'analyze-pg-guid-2', '2020-01-01T00:00:01Z', 0, 1, 'second')
            "#,
        )
        .bind(staging_conv_id)
        .bind(TEST_ACCOUNT)
        .execute(&mut *conn)
        .await
        .unwrap();
        let second = promote_append(&mut conn, ImportMode::Append, TEST_ACCOUNT, false, &[])
            .await
            .unwrap();
        assert_eq!(second.messages, 1, "second promote must still insert");
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
        let mut promote = Promote {
            tx: conn.begin().await.unwrap(),
            account_id: TEST_ACCOUNT,
            mode: ImportMode::Append,
            engine: DbEngine::Sqlite,
            stats: PromoteStats::default(),
            started: Instant::now(),
        };
        let mut map = HashMap::new();

        promote
            .zip_new_message_ids(&mut map, vec![101], 0, |n, p| {
                format!("unexpected mapping counts: staging={n} production={p}")
            })
            .await
            .unwrap();

        assert_eq!(map, HashMap::from([(101, 1)]));
    }
}
