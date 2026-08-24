//! Cross-source content fingerprint and soft-hide dedupe.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::time::Instant;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::AnyConnection;
use sqlx::Connection;

use crate::db::schema;

/// Collapse whitespace so minor text differences do not split the same SMS.
pub fn normalize_body(body: Option<&str>) -> String {
    body.unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Stable chat identity for content keys.
///
/// 1:1 chats use the conversation handle's normalized form (E.164 phones,
/// lowercased emails, verbatim usernames). Groups use sorted normalized
/// participant handles so the same people across exporters (different
/// `chat_identifier`s) share one fingerprint.
pub fn chat_identity_for_content_key(
    chat_identifier: &str,
    group_handles: Option<&[String]>,
) -> String {
    match group_handles {
        Some(handles) if !handles.is_empty() => {
            let mut sorted: Vec<&str> = handles
                .iter()
                .map(|h| h.as_str())
                .filter(|h| !h.is_empty())
                .collect();
            sorted.sort_unstable();
            sorted.dedup();
            format!("group:{}", sorted.join("|"))
        }
        _ => chat_identifier.to_string(),
    }
}

/// Build a content key from chat + direction + sender + UTC epoch + body + attachment hashes.
///
/// Prefers `timestamp_utc`; falls back to local `timestamp` (offsets are applied).
/// For groups, pass the sorted-participant identity from [`chat_identity_for_content_key`].
/// Incoming group messages include the normalized sender so two peers sending the same
/// text at the same second do not collide; outgoing (`is_from_me`) uses an empty sender.
pub fn compute_content_key(
    chat_identifier: &str,
    is_from_me: bool,
    sender_normalized: Option<&str>,
    timestamp_utc: Option<&str>,
    timestamp: &str,
    body: Option<&str>,
    attachment_shas: &[String],
) -> String {
    let epoch = resolve_utc_secs(timestamp_utc, timestamp)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            timestamp_utc
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(timestamp)
                .to_string()
        });

    let mut shas: Vec<&str> = attachment_shas
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    shas.sort_unstable();
    shas.dedup();

    let sender = if is_from_me {
        ""
    } else {
        sender_normalized.map(str::trim).unwrap_or("")
    };

    let mut hasher = Sha256::new();
    hasher.update(chat_identifier.as_bytes());
    hasher.update(b"|");
    hasher.update(if is_from_me { b"1" } else { b"0" });
    hasher.update(b"|");
    hasher.update(sender.as_bytes());
    hasher.update(b"|");
    hasher.update(epoch.as_bytes());
    hasher.update(b"|");
    hasher.update(normalize_body(body).as_bytes());
    for sha in shas {
        hasher.update(b"|");
        hasher.update(sha.as_bytes());
    }
    crate::assets::hex_encode(&hasher.finalize())
}

fn resolve_utc_secs(timestamp_utc: Option<&str>, timestamp: &str) -> Option<i64> {
    timestamp_utc
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(parse_rfc3339_utc_secs)
        .or_else(|| parse_rfc3339_utc_secs(timestamp.trim()))
}

/// Counts reported by one cross-source dedupe pass.
#[derive(Debug, Default)]
pub struct DedupeStats {
    /// Content keys written (one per message; not a duplicate count).
    pub keys_filled: u64,
    /// Groups of messages sharing one content key.
    pub exact_groups: u64,
    /// Messages hidden as exact duplicates (all but the survivor per group).
    pub exact_flagged: u64,
    /// Messages flagged as near duplicates.
    pub near_flagged: u64,
}

/// Source preference for survivors: first imported source (min message id), then name.
pub async fn source_priority_from_db(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT m.source, MIN(m.id) AS first_id
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.account_id = $1
          AND m.source IS NOT NULL
          AND TRIM(m.source) != ''
        GROUP BY m.source
        ORDER BY first_id ASC, m.source ASC
        "#,
    )
    .bind(account_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(source,)| source).collect())
}

/// Recompute every content key, clear prior flags, then soft-hide cross-source duplicates.
///
/// Survivor preference: source imported first (min message id), then source name.
/// Optional `source_priority` overrides (tests); `None` loads order from the DB.
pub async fn dedupe_cross_source(
    conn: &mut AnyConnection,
    account_id: &str,
    source_priority: Option<&[String]>,
    near_window_secs: i64,
) -> Result<DedupeStats> {
    let owned_priority;
    let priority = match source_priority {
        Some(p) => p,
        None => {
            owned_priority = source_priority_from_db(conn, account_id).await?;
            owned_priority.as_slice()
        }
    };
    let mut stats = DedupeStats::default();
    let prio: HashMap<&str, usize> = priority
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let started = Instant::now();

    {
        println!("  dedupe:   recomputing content keys…");
        let _ = io::stdout().flush();
        let mut tx = conn.begin().await?;
        stats.keys_filled = recompute_all_content_keys(&mut tx, account_id).await?;
        sqlx::query(
            r#"
            UPDATE messages
            SET duplicate_of = NULL
            WHERE conversation_id IN (
                SELECT id FROM conversations WHERE account_id = $1
            )
            "#,
        )
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        println!(
            "  dedupe:   keys filled={}  ({:.1}s)",
            stats.keys_filled,
            started.elapsed().as_secs_f64()
        );
    }

    {
        println!("  dedupe:   pass A exact content_key…");
        let _ = io::stdout().flush();
        let mut tx = conn.begin().await?;
        let (groups, flagged) = flag_exact_content_key_dupes(&mut tx, account_id, &prio).await?;
        stats.exact_groups = groups;
        stats.exact_flagged = flagged;
        tx.commit().await?;
        println!(
            "  dedupe:   exact groups={} flagged={}  ({:.1}s)",
            stats.exact_groups,
            stats.exact_flagged,
            started.elapsed().as_secs_f64()
        );
    }

    {
        println!("  dedupe:   pass B near-time (±{near_window_secs}s)…");
        let _ = io::stdout().flush();
        let mut tx = conn.begin().await?;
        stats.near_flagged =
            flag_near_time_dupes(&mut tx, account_id, &prio, near_window_secs).await?;
        tx.commit().await?;
        println!(
            "  dedupe:   near flagged={}  ({:.1}s total)",
            stats.near_flagged,
            started.elapsed().as_secs_f64()
        );
    }

    Ok(stats)
}

/// Compute `content_key` for production rows that still lack one (after attachments exist).
pub async fn fill_missing_content_keys(conn: &mut AnyConnection, account_id: &str) -> Result<u64> {
    recompute_content_keys(conn, true, account_id).await
}

/// Rebuild every message `content_key` from current chat/time/body/attachments.
pub async fn recompute_all_content_keys(conn: &mut AnyConnection, account_id: &str) -> Result<u64> {
    recompute_content_keys(conn, false, account_id).await
}

async fn recompute_content_keys(
    conn: &mut AnyConnection,
    missing_only: bool,
    account_id: &str,
) -> Result<u64> {
    let filter = if missing_only {
        "WHERE (m.content_key IS NULL OR m.content_key = '') AND c.account_id = $1"
    } else {
        "WHERE c.account_id = $1"
    };
    let sql = format!(
        r#"
        SELECT m.id, m.conversation_id, h.normalized, c.conversation_type,
               m.is_from_me, m.timestamp_utc, m.timestamp, m.body,
               hs.normalized
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        JOIN handles h ON h.id = c.chat_handle_id
        LEFT JOIN handles hs ON hs.id = m.sender_handle_id
        {filter}
        ORDER BY m.id
        "#
    );
    type ExactDedupeRow = (
        i64,
        i64,
        String,
        String,
        i64,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<ExactDedupeRow> = sqlx::query_as(&sql)
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;

    if rows.is_empty() {
        return Ok(0);
    }

    // Sorted participant handles per group conversation (one shared identity
    // across import sources).
    let mut group_handles: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let p_rows: Vec<(i64, String)> = sqlx::query_as(
            r#"
            SELECT p.conversation_id, h.normalized
            FROM participants p
            JOIN conversations c ON c.id = p.conversation_id
            JOIN handles h ON h.id = p.handle_id
            WHERE c.account_id = $1
              AND h.normalized IS NOT NULL AND h.normalized != ''
            ORDER BY p.conversation_id, h.normalized
            "#,
        )
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;
        for (conversation_id, handle) in p_rows {
            group_handles
                .entry(conversation_id)
                .or_default()
                .push(handle);
        }
    }

    // One scan for attachment hashes belonging to this account's message id range.
    let min_id = rows.first().map(|r| r.0).unwrap_or(0);
    let max_id = rows.last().map(|r| r.0).unwrap_or(0);
    let mut shas_by_msg: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let att_rows: Vec<(i64, String)> = sqlx::query_as(
            r#"
            SELECT a.message_id, a.sha256
            FROM attachments a
            JOIN messages m ON m.id = a.message_id
            JOIN conversations c ON c.id = m.conversation_id
            WHERE c.account_id = $1
              AND a.message_id BETWEEN $2 AND $3
              AND a.sha256 IS NOT NULL AND a.sha256 != ''
            ORDER BY a.message_id
            "#,
        )
        .bind(account_id)
        .bind(min_id)
        .bind(max_id)
        .fetch_all(&mut *conn)
        .await?;
        for (message_id, sha) in att_rows {
            shas_by_msg.entry(message_id).or_default().push(sha);
        }
    }

    let empty: Vec<String> = Vec::new();
    let mut keys: Vec<(i64, String)> = Vec::with_capacity(rows.len());
    for (
        id,
        conversation_id,
        chat_id,
        conversation_type,
        is_from_me,
        ts_utc,
        ts,
        body,
        sender_norm,
    ) in rows
    {
        let shas = shas_by_msg.get(&id).unwrap_or(&empty);
        let identity = if conversation_type == "group" {
            chat_identity_for_content_key(
                &chat_id,
                group_handles.get(&conversation_id).map(|h| h.as_slice()),
            )
        } else {
            chat_id
        };
        let key = compute_content_key(
            &identity,
            is_from_me != 0,
            sender_norm.as_deref(),
            ts_utc.as_deref(),
            &ts,
            body.as_deref(),
            shas,
        );
        keys.push((id, key));
    }

    let filled = keys.len() as u64;
    for stmt in schema::split_ddl(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _content_keys (
            id INTEGER PRIMARY KEY,
            content_key TEXT NOT NULL
        );
        DELETE FROM _content_keys;
        "#,
    ) {
        sqlx::query(&stmt).execute(&mut *conn).await?;
    }
    {
        for (id, key) in &keys {
            sqlx::query("INSERT INTO _content_keys (id, content_key) VALUES ($1, $2)")
                .bind(id)
                .bind(key)
                .execute(&mut *conn)
                .await?;
        }
    }
    sqlx::query(
        r#"
        UPDATE messages AS m
        SET content_key = k.content_key
        FROM _content_keys AS k
        WHERE m.id = k.id
        "#,
    )
    .execute(&mut *conn)
    .await?;
    for stmt in schema::split_ddl("DROP TABLE IF EXISTS _content_keys;") {
        sqlx::query(&stmt).execute(&mut *conn).await?;
    }

    Ok(filled)
}

#[derive(Clone)]
struct Cand {
    id: i64,
    source: String,
    att_count: i64,
}

async fn flag_exact_content_key_dupes(
    conn: &mut AnyConnection,
    account_id: &str,
    prio: &HashMap<&str, usize>,
) -> Result<(u64, u64)> {
    // One scan of messages + one aggregated attachment pass, then group in Rust.
    // Avoids N round-trips (one SELECT + several UPDATEs per duplicate key).
    let rows: Vec<(i64, String, String, i64)> = sqlx::query_as(
        r#"
        SELECT m.id, m.source, m.content_key, COALESCE(ac.n, 0)
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        LEFT JOIN (
            SELECT a.message_id, COUNT(*) AS n
            FROM attachments a
            JOIN messages m2 ON m2.id = a.message_id
            JOIN conversations c2 ON c2.id = m2.conversation_id
            WHERE c2.account_id = $1
              AND a.sha256 IS NOT NULL AND a.sha256 != ''
            GROUP BY a.message_id
        ) ac ON ac.message_id = m.id
        WHERE c.account_id = $1
          AND m.content_key IS NOT NULL AND m.content_key != ''
        "#,
    )
    .bind(account_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut by_key: HashMap<String, Vec<Cand>> = HashMap::new();
    for (id, source, content_key, att_count) in rows {
        by_key.entry(content_key).or_default().push(Cand {
            id,
            source,
            att_count,
        });
    }

    let mut flags: Vec<(i64, i64)> = Vec::new(); // (loser_id, winner_id)
    let mut groups = 0u64;
    for cands in by_key.values() {
        let sources: HashSet<&str> = cands.iter().map(|c| c.source.as_str()).collect();
        if sources.len() < 2 {
            continue;
        }
        groups += 1;
        let winner = pick_winner(cands, prio);
        for c in cands {
            if c.id != winner {
                flags.push((c.id, winner));
            }
        }
    }
    let flagged = flags.len() as u64;

    if flags.is_empty() {
        return Ok((groups, 0));
    }

    apply_duplicate_flags(conn, "_pass_a_flags", &flags).await?;

    Ok((groups, flagged))
}

fn pick_winner(cands: &[Cand], prio: &HashMap<&str, usize>) -> i64 {
    cands
        .iter()
        .min_by(|a, b| {
            b.att_count
                .cmp(&a.att_count)
                .then_with(|| {
                    let pa = prio.get(a.source.as_str()).copied().unwrap_or(usize::MAX);
                    let pb = prio.get(b.source.as_str()).copied().unwrap_or(usize::MAX);
                    pa.cmp(&pb)
                })
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|c| c.id)
        .unwrap_or(cands[0].id)
}

/// Parse RFC3339 (second precision) into Unix UTC seconds, honoring Z / ±HH:MM offsets.
fn parse_rfc3339_utc_secs(ts: &str) -> Option<i64> {
    let s = ts.trim();
    if s.len() < 19 {
        return None;
    }
    let date = &s[..10];
    let tsep = s.as_bytes().get(10).copied()?;
    if tsep != b'T' && tsep != b't' {
        return None;
    }
    let time = &s[11..19];
    let (y, mo, d) = (
        date.get(0..4)?.parse::<i64>().ok()?,
        date.get(5..7)?.parse::<i64>().ok()?,
        date.get(8..10)?.parse::<i64>().ok()?,
    );
    let (h, mi, se) = (
        time.get(0..2)?.parse::<i64>().ok()?,
        time.get(3..5)?.parse::<i64>().ok()?,
        time.get(6..8)?.parse::<i64>().ok()?,
    );

    let mut rest = &s[19..];
    if rest.starts_with('.') {
        rest = rest[1..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset_secs = parse_offset_secs(rest)?;
    let local_as_utc = civil_to_unix_secs(y, mo, d, h, mi, se)?;
    Some(local_as_utc - offset_secs)
}

fn parse_offset_secs(rest: &str) -> Option<i64> {
    let rest = rest.trim();
    if rest.is_empty() || rest == "Z" || rest == "z" {
        return Some(0);
    }
    let sign = match rest.chars().next()? {
        '+' => 1i64,
        '-' => -1i64,
        _ => return None,
    };
    let body = &rest[1..];
    // HH:MM or HHMM
    let (oh, om) = if body.len() >= 5 && body.as_bytes().get(2) == Some(&b':') {
        (
            body.get(0..2)?.parse::<i64>().ok()?,
            body.get(3..5)?.parse::<i64>().ok()?,
        )
    } else if body.len() >= 4 {
        (
            body.get(0..2)?.parse::<i64>().ok()?,
            body.get(2..4)?.parse::<i64>().ok()?,
        )
    } else {
        return None;
    };
    Some(sign * (oh * 3600 + om * 60))
}

fn civil_to_unix_secs(y: i64, mo: i64, d: i64, h: i64, mi: i64, se: i64) -> Option<i64> {
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    if h > 23 || mi > 59 || se > 60 {
        return None;
    }
    // Days from civil date (Howard Hinnant) → Unix seconds.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

#[derive(Clone)]
struct NearRow {
    id: i64,
    source: String,
    is_from_me: i64,
    sender_norm: String,
    secs: i64,
    body_norm: String,
    att_fp: String,
    att_count: i64,
}

async fn flag_near_time_dupes(
    conn: &mut AnyConnection,
    account_id: &str,
    prio: &HashMap<&str, usize>,
    window_secs: i64,
) -> Result<u64> {
    // Preload unflagged messages + attachment fingerprints once, then cluster in Rust.
    // Avoids per-message attachment queries and per-candidate duplicate_of SELECTs.
    type NearDedupeRow = (
        i64,
        i64,
        String,
        i64,
        Option<String>,
        String,
        Option<String>,
        String,
    );
    let msg_rows: Vec<NearDedupeRow> = sqlx::query_as(
        r#"
        SELECT m.id, m.conversation_id, m.source, m.is_from_me, m.timestamp_utc, m.timestamp, m.body,
               COALESCE(hs.normalized, '')
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        LEFT JOIN handles hs ON hs.id = m.sender_handle_id
        WHERE c.account_id = $1
          AND m.duplicate_of IS NULL
        "#,
    )
    .bind(account_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut shas_by_msg: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let att_rows: Vec<(i64, String)> = sqlx::query_as(
            r#"
            SELECT a.message_id, a.sha256
            FROM attachments a
            JOIN messages m ON m.id = a.message_id
            JOIN conversations c ON c.id = m.conversation_id
            WHERE c.account_id = $1
              AND a.sha256 IS NOT NULL AND a.sha256 != ''
            ORDER BY a.message_id, a.sha256
            "#,
        )
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;
        for (message_id, sha) in att_rows {
            shas_by_msg.entry(message_id).or_default().push(sha);
        }
    }

    let empty: Vec<String> = Vec::new();
    let mut by_conv: HashMap<i64, Vec<NearRow>> = HashMap::new();
    for (id, conversation_id, source, is_from_me, ts_utc, ts, body, sender_norm) in msg_rows {
        let Some(secs) = resolve_utc_secs(ts_utc.as_deref(), &ts) else {
            continue;
        };
        let shas = shas_by_msg.get(&id).unwrap_or(&empty);
        let att_count = shas.len() as i64;
        let att_fp = shas.join(",");
        let sender = if is_from_me != 0 {
            String::new()
        } else {
            sender_norm
        };
        by_conv.entry(conversation_id).or_default().push(NearRow {
            id,
            source,
            is_from_me,
            sender_norm: sender,
            secs,
            body_norm: normalize_body(body.as_deref()),
            att_fp,
            att_count,
        });
    }

    let mut flagged_ids: HashSet<i64> = HashSet::new();
    let mut flags: Vec<(i64, i64)> = Vec::new(); // (loser_id, winner_id)

    for mut rows in by_conv.into_values() {
        rows.sort_by(|a, b| a.secs.cmp(&b.secs).then(a.id.cmp(&b.id)));

        for i in 0..rows.len() {
            if flagged_ids.contains(&rows[i].id) {
                continue;
            }

            let mut cluster: Vec<Cand> = vec![Cand {
                id: rows[i].id,
                source: rows[i].source.clone(),
                att_count: rows[i].att_count,
            }];

            for j in (i + 1)..rows.len() {
                if rows[j].secs - rows[i].secs > window_secs {
                    break;
                }
                if rows[j].is_from_me != rows[i].is_from_me {
                    continue;
                }
                if rows[j].sender_norm != rows[i].sender_norm {
                    continue;
                }
                if rows[j].source == rows[i].source {
                    continue;
                }
                let body_match =
                    !rows[i].body_norm.is_empty() && rows[j].body_norm == rows[i].body_norm;
                let att_match = !rows[i].att_fp.is_empty() && rows[j].att_fp == rows[i].att_fp;
                if !body_match && !att_match {
                    continue;
                }
                if flagged_ids.contains(&rows[j].id) {
                    continue;
                }
                cluster.push(Cand {
                    id: rows[j].id,
                    source: rows[j].source.clone(),
                    att_count: rows[j].att_count,
                });
            }

            let sources: HashSet<&str> = cluster.iter().map(|c| c.source.as_str()).collect();
            if sources.len() < 2 {
                continue;
            }
            let winner = pick_winner(&cluster, prio);
            for c in &cluster {
                if c.id == winner {
                    continue;
                }
                flagged_ids.insert(c.id);
                flags.push((c.id, winner));
            }
        }
    }

    let flagged = flags.len() as u64;
    if flags.is_empty() {
        return Ok(0);
    }

    apply_duplicate_flags(conn, "_pass_b_flags", &flags).await?;

    Ok(flagged)
}

async fn apply_duplicate_flags(
    conn: &mut AnyConnection,
    table: &str,
    flags: &[(i64, i64)],
) -> Result<()> {
    for stmt in schema::split_ddl(&format!(
        "CREATE TEMP TABLE IF NOT EXISTS {table} (
            id INTEGER PRIMARY KEY,
            winner INTEGER NOT NULL
        );
        DELETE FROM {table};"
    )) {
        sqlx::query(&stmt).execute(&mut *conn).await?;
    }
    {
        for (id, winner) in flags {
            sqlx::query(&format!("INSERT INTO {table} (id, winner) VALUES ($1, $2)"))
                .bind(id)
                .bind(winner)
                .execute(&mut *conn)
                .await?;
        }
    }
    sqlx::query(&format!(
        "UPDATE messages AS m
         SET duplicate_of = f.winner
         FROM {table} AS f
         WHERE m.id = f.id"
    ))
    .execute(&mut *conn)
    .await?;
    for stmt in schema::split_ddl(&format!("DROP TABLE IF EXISTS {table};")) {
        sqlx::query(&stmt).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Open DB helpers used by CLI.
pub async fn run_dedupe(
    db_path: &std::path::Path,
    account_id: &str,
    near_window_secs: i64,
) -> Result<DedupeStats> {
    let pool = crate::db::engine::open_pool_for_path(db_path)
        .await
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    let mut conn = pool.acquire().await?;
    crate::db::schema::ensure_vault_schema(&mut conn).await?;
    dedupe_cross_source(&mut conn, account_id, None, near_window_secs).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::engine;

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize_body(Some("  hi   mom \n")), "hi mom");
    }

    #[test]
    fn group_chat_identity_is_sorted_handles() {
        let handles = vec![
            "+14075550002".to_string(),
            "+14075550001".to_string(),
            "+14075550002".to_string(),
        ];
        assert_eq!(
            chat_identity_for_content_key("chat999", Some(&handles)),
            "group:+14075550001|+14075550002"
        );
        assert_eq!(chat_identity_for_content_key("chat999", None), "chat999");
    }

    #[test]
    fn content_key_stable_across_whitespace_and_utc_forms() {
        let a = compute_content_key(
            "+14075551212",
            true,
            None,
            Some("2015-03-12T18:04:22Z"),
            "x",
            Some("Running late"),
            &[],
        );
        let b = compute_content_key(
            "+14075551212",
            true,
            None,
            Some("2015-03-12T18:04:22+00:00"),
            "y",
            Some("  Running   late "),
            &[],
        );
        let c = compute_content_key(
            "+14075551212",
            true,
            None,
            None,
            "2015-03-12T14:04:22-04:00",
            Some("Running late"),
            &[],
        );
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn content_key_distinguishes_group_senders() {
        let alice = compute_content_key(
            "group:+1|+2",
            false,
            Some("+15555550001"),
            Some("2015-03-12T18:04:22Z"),
            "x",
            Some("same text"),
            &[],
        );
        let bob = compute_content_key(
            "group:+1|+2",
            false,
            Some("+15555550002"),
            Some("2015-03-12T18:04:22Z"),
            "x",
            Some("same text"),
            &[],
        );
        assert_ne!(alice, bob);
    }

    #[test]
    fn parse_rfc3339_applies_offset() {
        assert_eq!(
            parse_rfc3339_utc_secs("2015-03-12T18:04:22Z"),
            Some(1426183462)
        );
        assert_eq!(
            parse_rfc3339_utc_secs("2015-03-12T18:04:22+00:00"),
            Some(1426183462)
        );
        assert_eq!(
            parse_rfc3339_utc_secs("2015-03-12T14:04:22-04:00"),
            Some(1426183462)
        );
    }

    const TEST_ACCOUNT_ID: &str = "00000000-0000-0000-0000-000000000001";

    async fn setup_db(conn: &mut AnyConnection) {
        schema::ensure_vault_schema(conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username, read_only) VALUES ($1, 'test', 0)")
            .bind(TEST_ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO handles (account_id, raw, normalized, handle_type, service)
            VALUES ($1, '+14075551212', '+14075551212', 'phone', 'phone')
            "#,
        )
        .bind(TEST_ACCOUNT_ID)
        .execute(&mut *conn)
        .await
        .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "SELECT id FROM handles WHERE account_id = $1 AND normalized = '+14075551212'",
        )
        .bind(TEST_ACCOUNT_ID)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, exported_at, source_file
            )
            VALUES ($1, $2, 'individual', NULL, NULL, 't.json')
            "#,
        )
        .bind(TEST_ACCOUNT_ID)
        .bind(handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    struct InsertMsgArgs<'a> {
        source: &'a str,
        guid: &'a str,
        utc: &'a str,
        local: &'a str,
        from_me: i64,
        body: &'a str,
        sort_order: i64,
    }

    async fn insert_msg(conn: &mut AnyConnection, args: InsertMsgArgs<'_>) -> i64 {
        sqlx::query_scalar(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, subject, body, sort_order
            ) VALUES (1, $1, $2, $3, $4, $5, $6, NULL, NULL, $7, $8)
            RETURNING id
            "#,
        )
        .bind(TEST_ACCOUNT_ID)
        .bind(args.source)
        .bind(args.guid)
        .bind(args.local)
        .bind(args.utc)
        .bind(args.from_me)
        .bind(args.body)
        .bind(args.sort_order)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn integration_exact_flags_cross_source() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        setup_db(&mut conn).await;
        let a = insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "go-sms-pro",
                guid: "g1",
                utc: "2015-03-12T18:04:22Z",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 1,
                body: "Running late",
                sort_order: 0,
            },
        )
        .await;
        let b = insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "sms-backup-plus",
                guid: "g2",
                utc: "2015-03-12T18:04:22+00:00",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 1,
                body: "Running late",
                sort_order: 0,
            },
        )
        .await;
        let priority = ["go-sms-pro".into(), "sms-backup-plus".into()];
        let stats = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, Some(&priority), 2)
            .await
            .unwrap();
        assert_eq!(stats.exact_groups, 1);
        assert_eq!(stats.exact_flagged, 1);
        let dup: Option<i64> =
            sqlx::query_scalar("SELECT duplicate_of FROM messages WHERE id = $1")
                .bind(b)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(dup, Some(a));
        let keep: Option<i64> =
            sqlx::query_scalar("SELECT duplicate_of FROM messages WHERE id = $1")
                .bind(a)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(keep, None);
    }

    #[tokio::test]
    async fn integration_near_flags_within_window() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        setup_db(&mut conn).await;
        let a = insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "go-sms-pro",
                guid: "g1",
                utc: "2015-03-12T18:04:22Z",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 0,
                body: "On my way",
                sort_order: 0,
            },
        )
        .await;
        let b = insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "sms-backup-plus",
                guid: "g2",
                utc: "2015-03-12T18:04:24Z",
                local: "2015-03-12T14:04:24-04:00",
                from_me: 0,
                body: "On my way",
                sort_order: 1,
            },
        )
        .await;
        let priority = ["go-sms-pro".into(), "sms-backup-plus".into()];
        let stats = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, Some(&priority), 2)
            .await
            .unwrap();
        assert_eq!(stats.exact_flagged, 0);
        assert_eq!(stats.near_flagged, 1);
        let dup: Option<i64> =
            sqlx::query_scalar("SELECT duplicate_of FROM messages WHERE id = $1")
                .bind(b)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(dup, Some(a));
    }

    #[tokio::test]
    async fn integration_negative_far_apart_not_flagged() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        setup_db(&mut conn).await;
        insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "go-sms-pro",
                guid: "g1",
                utc: "2015-03-12T18:04:22Z",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 0,
                body: "On my way",
                sort_order: 0,
            },
        )
        .await;
        insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "sms-backup-plus",
                guid: "g2",
                utc: "2015-03-12T18:05:22Z",
                local: "2015-03-12T14:05:22-04:00",
                from_me: 0,
                body: "On my way",
                sort_order: 1,
            },
        )
        .await;
        let priority = ["go-sms-pro".into(), "sms-backup-plus".into()];
        let stats = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, Some(&priority), 2)
            .await
            .unwrap();
        assert_eq!(stats.exact_flagged, 0);
        assert_eq!(stats.near_flagged, 0);
        let hidden: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE duplicate_of IS NOT NULL")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(hidden, 0);
    }

    #[tokio::test]
    async fn integration_priority_prefers_first_imported_source() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        setup_db(&mut conn).await;
        // First row wins when priority is derived from min(message id) per source.
        let first_imported = insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "sms-backup-plus",
                guid: "g1",
                utc: "2015-03-12T18:04:22Z",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 1,
                body: "Hello",
                sort_order: 0,
            },
        )
        .await;
        let second_imported = insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "go-sms-pro",
                guid: "g2",
                utc: "2015-03-12T18:04:22Z",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 1,
                body: "Hello",
                sort_order: 1,
            },
        )
        .await;
        dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, None, 2)
            .await
            .unwrap();
        let dup_first: Option<i64> =
            sqlx::query_scalar("SELECT duplicate_of FROM messages WHERE id = $1")
                .bind(first_imported)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        let dup_second: Option<i64> =
            sqlx::query_scalar("SELECT duplicate_of FROM messages WHERE id = $1")
                .bind(second_imported)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(dup_first, None);
        assert_eq!(dup_second, Some(first_imported));
    }
}
