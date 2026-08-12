//! Cross-source content fingerprint and soft-hide dedupe.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::time::Instant;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

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

/// Build a content key from chat + UTC epoch seconds + direction + normalized body + attachment hashes.
///
/// Prefers `timestamp_utc`; falls back to local `timestamp` (offsets are applied).
/// For groups, pass the sorted-participant identity from [`chat_identity_for_content_key`].
pub fn compute_content_key(
    chat_identifier: &str,
    is_from_me: bool,
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

    let mut hasher = Sha256::new();
    hasher.update(chat_identifier.as_bytes());
    hasher.update(b"|");
    hasher.update(if is_from_me { b"1" } else { b"0" });
    hasher.update(b"|");
    hasher.update(epoch.as_bytes());
    hasher.update(b"|");
    hasher.update(normalize_body(body).as_bytes());
    for sha in shas {
        hasher.update(b"|");
        hasher.update(sha.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn resolve_utc_secs(timestamp_utc: Option<&str>, timestamp: &str) -> Option<i64> {
    timestamp_utc
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(parse_rfc3339_utc_secs)
        .or_else(|| parse_rfc3339_utc_secs(timestamp.trim()))
}

#[derive(Debug, Default)]
pub struct DedupeStats {
    pub keys_filled: u64,
    pub exact_groups: u64,
    pub exact_flagged: u64,
    pub near_flagged: u64,
}

/// Source preference for survivors: first imported source (min message id), then name.
pub fn source_priority_from_db(conn: &Connection, account_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT m.source, MIN(m.id) AS first_id
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.account_id = ?1
          AND m.source IS NOT NULL
          AND TRIM(m.source) != ''
        GROUP BY m.source
        ORDER BY first_id ASC, m.source ASC
        "#,
    )?;
    let rows = stmt.query_map(params![account_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Recompute every content key, clear prior flags, then soft-hide cross-source duplicates.
///
/// Survivor preference: source imported first (min message id), then source name.
/// Optional `source_priority` overrides (tests); `None` loads order from the DB.
pub fn dedupe_cross_source(
    conn: &mut Connection,
    account_id: &str,
    source_priority: Option<&[String]>,
    near_window_secs: i64,
) -> Result<DedupeStats> {
    let owned_priority;
    let priority = match source_priority {
        Some(p) => p,
        None => {
            owned_priority = source_priority_from_db(conn, account_id)?;
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
        let tx = conn.transaction()?;
        stats.keys_filled = recompute_all_content_keys(&tx, account_id)?;
        tx.execute(
            r#"
            UPDATE messages
            SET duplicate_of = NULL
            WHERE conversation_id IN (
                SELECT id FROM conversations WHERE account_id = ?1
            )
            "#,
            params![account_id],
        )?;
        tx.commit()?;
        println!(
            "  dedupe:   keys filled={}  ({:.1}s)",
            stats.keys_filled,
            started.elapsed().as_secs_f64()
        );
    }

    {
        println!("  dedupe:   pass A exact content_key…");
        let _ = io::stdout().flush();
        let tx = conn.transaction()?;
        let (groups, flagged) = flag_exact_content_key_dupes(&tx, account_id, &prio)?;
        stats.exact_groups = groups;
        stats.exact_flagged = flagged;
        tx.commit()?;
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
        let tx = conn.transaction()?;
        stats.near_flagged = flag_near_time_dupes(&tx, account_id, &prio, near_window_secs)?;
        tx.commit()?;
        println!(
            "  dedupe:   near flagged={}  ({:.1}s total)",
            stats.near_flagged,
            started.elapsed().as_secs_f64()
        );
    }

    Ok(stats)
}

/// Compute `content_key` for production rows that still lack one (after attachments exist).
pub fn fill_missing_content_keys(conn: &Connection, account_id: &str) -> Result<u64> {
    recompute_content_keys(conn, true, account_id)
}

/// Rebuild every message `content_key` from current chat/time/body/attachments.
pub fn recompute_all_content_keys(conn: &Connection, account_id: &str) -> Result<u64> {
    recompute_content_keys(conn, false, account_id)
}

fn recompute_content_keys(conn: &Connection, missing_only: bool, account_id: &str) -> Result<u64> {
    let filter = if missing_only {
        "WHERE (m.content_key IS NULL OR m.content_key = '') AND c.account_id = ?1"
    } else {
        "WHERE c.account_id = ?1"
    };
    let sql = format!(
        r#"
        SELECT m.id, m.conversation_id, h.normalized, c.conversation_type,
               m.is_from_me, m.timestamp_utc, m.timestamp, m.body
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        JOIN handles h ON h.id = c.chat_handle_id
        {filter}
        ORDER BY m.id
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    type ExactDedupeRow = (
        i64,
        i64,
        String,
        String,
        i64,
        Option<String>,
        String,
        Option<String>,
    );
    let rows: Vec<ExactDedupeRow> = stmt
        .query_map(params![account_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if rows.is_empty() {
        return Ok(0);
    }

    // Sorted participant handles per group conversation (canonical cross-source identity).
    let mut group_handles: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let mut p_stmt = conn.prepare(
            r#"
            SELECT p.conversation_id, h.normalized
            FROM participants p
            JOIN conversations c ON c.id = p.conversation_id
            JOIN handles h ON h.id = p.handle_id
            WHERE c.account_id = ?1
              AND h.normalized IS NOT NULL AND h.normalized != ''
            ORDER BY p.conversation_id, h.normalized
            "#,
        )?;
        let p_rows = p_stmt.query_map(params![account_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in p_rows {
            let (conversation_id, handle) = row?;
            group_handles
                .entry(conversation_id)
                .or_default()
                .push(handle);
        }
    }

    // One scan for all attachment hashes instead of per-message queries.
    let min_id = rows.first().map(|r| r.0).unwrap_or(0);
    let max_id = rows.last().map(|r| r.0).unwrap_or(0);
    let mut shas_by_msg: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let mut att_stmt = conn.prepare(
            r#"
            SELECT message_id, sha256
            FROM attachments
            WHERE message_id BETWEEN ?1 AND ?2
              AND sha256 IS NOT NULL AND sha256 != ''
            ORDER BY message_id
            "#,
        )?;
        let att_rows = att_stmt.query_map(params![min_id, max_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in att_rows {
            let (message_id, sha) = row?;
            shas_by_msg.entry(message_id).or_default().push(sha);
        }
    }

    let empty: Vec<String> = Vec::new();
    let mut keys: Vec<(i64, String)> = Vec::with_capacity(rows.len());
    for (id, conversation_id, chat_id, conversation_type, is_from_me, ts_utc, ts, body) in rows {
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
            ts_utc.as_deref(),
            &ts,
            body.as_deref(),
            shas,
        );
        keys.push((id, key));
    }

    let filled = keys.len() as u64;
    conn.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _content_keys (
            id INTEGER PRIMARY KEY,
            content_key TEXT NOT NULL
        );
        DELETE FROM _content_keys;
        "#,
    )?;
    {
        let mut ins =
            conn.prepare("INSERT INTO _content_keys (id, content_key) VALUES (?1, ?2)")?;
        for (id, key) in &keys {
            ins.execute(params![id, key])?;
        }
    }
    conn.execute(
        r#"
        UPDATE messages AS m
        SET content_key = k.content_key
        FROM _content_keys AS k
        WHERE m.id = k.id
        "#,
        [],
    )?;
    conn.execute_batch("DROP TABLE IF EXISTS _content_keys;")?;

    Ok(filled)
}

#[derive(Clone)]
struct Cand {
    id: i64,
    source: String,
    att_count: i64,
}

fn flag_exact_content_key_dupes(
    conn: &Connection,
    account_id: &str,
    prio: &HashMap<&str, usize>,
) -> Result<(u64, u64)> {
    // One scan of messages + one aggregated attachment pass, then group in Rust.
    // Avoids N round-trips (one SELECT + several UPDATEs per duplicate key).
    let mut stmt = conn.prepare(
        r#"
        SELECT m.id, m.source, m.content_key, IFNULL(ac.n, 0)
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        LEFT JOIN (
            SELECT message_id, COUNT(*) AS n
            FROM attachments
            WHERE sha256 IS NOT NULL AND sha256 != ''
            GROUP BY message_id
        ) ac ON ac.message_id = m.id
        WHERE c.account_id = ?1
          AND m.content_key IS NOT NULL AND m.content_key != ''
        "#,
    )?;

    let mut by_key: HashMap<String, Vec<Cand>> = HashMap::new();
    {
        let rows = stmt.query_map(params![account_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (id, source, content_key, att_count) = row?;
            by_key.entry(content_key).or_default().push(Cand {
                id,
                source,
                att_count,
            });
        }
    }
    drop(stmt);

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

    conn.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _pass_a_flags (
            id INTEGER PRIMARY KEY,
            winner INTEGER NOT NULL
        );
        DELETE FROM _pass_a_flags;
        "#,
    )?;
    {
        let mut ins = conn.prepare("INSERT INTO _pass_a_flags (id, winner) VALUES (?1, ?2)")?;
        for (id, winner) in &flags {
            ins.execute(params![id, winner])?;
        }
    }
    conn.execute(
        r#"
        UPDATE messages AS m
        SET duplicate_of = f.winner
        FROM _pass_a_flags AS f
        WHERE m.id = f.id
        "#,
        [],
    )?;
    conn.execute_batch("DROP TABLE IF EXISTS _pass_a_flags;")?;

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
    secs: i64,
    body_norm: String,
    att_fp: String,
    att_count: i64,
}

fn flag_near_time_dupes(
    conn: &Connection,
    account_id: &str,
    prio: &HashMap<&str, usize>,
    window_secs: i64,
) -> Result<u64> {
    // Preload unflagged messages + attachment fingerprints once, then cluster in Rust.
    // Avoids per-message attachment queries and per-candidate duplicate_of SELECTs.
    let mut msg_stmt = conn.prepare(
        r#"
        SELECT m.id, m.conversation_id, m.source, m.is_from_me, m.timestamp_utc, m.timestamp, m.body
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.account_id = ?1
          AND m.duplicate_of IS NULL
        "#,
    )?;
    type NearDedupeRow = (
        i64,
        i64,
        String,
        i64,
        Option<String>,
        String,
        Option<String>,
    );
    let msg_rows: Vec<NearDedupeRow> = msg_stmt
        .query_map(params![account_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(msg_stmt);

    let mut shas_by_msg: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let mut att_stmt = conn.prepare(
            r#"
            SELECT message_id, sha256
            FROM attachments
            WHERE sha256 IS NOT NULL AND sha256 != ''
            ORDER BY message_id, sha256
            "#,
        )?;
        let att_rows = att_stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in att_rows {
            let (message_id, sha) = row?;
            shas_by_msg.entry(message_id).or_default().push(sha);
        }
    }

    let empty: Vec<String> = Vec::new();
    let mut by_conv: HashMap<i64, Vec<NearRow>> = HashMap::new();
    for (id, conversation_id, source, is_from_me, ts_utc, ts, body) in msg_rows {
        let Some(secs) = resolve_utc_secs(ts_utc.as_deref(), &ts) else {
            continue;
        };
        let shas = shas_by_msg.get(&id).unwrap_or(&empty);
        let att_count = shas.len() as i64;
        let att_fp = shas.join(",");
        by_conv.entry(conversation_id).or_default().push(NearRow {
            id,
            source,
            is_from_me,
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

    conn.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS _pass_b_flags (
            id INTEGER PRIMARY KEY,
            winner INTEGER NOT NULL
        );
        DELETE FROM _pass_b_flags;
        "#,
    )?;
    {
        let mut ins = conn.prepare("INSERT INTO _pass_b_flags (id, winner) VALUES (?1, ?2)")?;
        for (id, winner) in &flags {
            ins.execute(params![id, winner])?;
        }
    }
    conn.execute(
        r#"
        UPDATE messages AS m
        SET duplicate_of = f.winner
        FROM _pass_b_flags AS f
        WHERE m.id = f.id
        "#,
        [],
    )?;
    conn.execute_batch("DROP TABLE IF EXISTS _pass_b_flags;")?;

    Ok(flagged)
}

/// Open DB helpers used by CLI.
pub fn run_dedupe(
    db_path: &std::path::Path,
    account_id: &str,
    near_window_secs: i64,
) -> Result<DedupeStats> {
    let mut conn = Connection::open(db_path)
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    crate::db::schema::ensure_vault_schema(&conn)?;
    dedupe_cross_source(&mut conn, account_id, None, near_window_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

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
            Some("2015-03-12T18:04:22Z"),
            "x",
            Some("Running late"),
            &[],
        );
        let b = compute_content_key(
            "+14075551212",
            true,
            Some("2015-03-12T18:04:22+00:00"),
            "y",
            Some("  Running   late "),
            &[],
        );
        let c = compute_content_key(
            "+14075551212",
            true,
            None,
            "2015-03-12T14:04:22-04:00",
            Some("Running late"),
            &[],
        );
        assert_eq!(a, b);
        assert_eq!(a, c);
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

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'test', 0)",
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();
        conn.execute(
            r#"
            INSERT INTO handles (account_id, raw, normalized, handle_type, service)
            VALUES (?1, '+14075551212', '+14075551212', 'phone', 'phone')
            "#,
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();
        let handle_id: i64 = conn
            .query_row(
                "SELECT id FROM handles WHERE account_id = ?1 AND normalized = '+14075551212'",
                params![TEST_ACCOUNT_ID],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, group_title, exported_at, source_file
            )
            VALUES (?1, ?2, 'individual', NULL, NULL, 't.json')
            "#,
            params![TEST_ACCOUNT_ID, handle_id],
        )
        .unwrap();
        conn
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_msg(
        conn: &Connection,
        source: &str,
        guid: &str,
        utc: &str,
        local: &str,
        from_me: i64,
        body: &str,
        sort_order: i64,
    ) -> i64 {
        conn.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender_handle_id, subject, body, sort_order
            ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7, ?8)
            "#,
            params![
                TEST_ACCOUNT_ID,
                source,
                guid,
                local,
                utc,
                from_me,
                body,
                sort_order
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn integration_exact_flags_cross_source() {
        let mut conn = setup_db();
        let a = insert_msg(
            &conn,
            "go-sms-pro",
            "g1",
            "2015-03-12T18:04:22Z",
            "2015-03-12T14:04:22-04:00",
            1,
            "Running late",
            0,
        );
        let b = insert_msg(
            &conn,
            "sms-backup-plus",
            "g2",
            "2015-03-12T18:04:22+00:00",
            "2015-03-12T14:04:22-04:00",
            1,
            "Running late",
            0,
        );
        let priority = ["go-sms-pro".into(), "sms-backup-plus".into()];
        let stats = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, Some(&priority), 2).unwrap();
        assert_eq!(stats.exact_groups, 1);
        assert_eq!(stats.exact_flagged, 1);
        let dup: Option<i64> = conn
            .query_row(
                "SELECT duplicate_of FROM messages WHERE id = ?1",
                params![b],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dup, Some(a));
        let keep: Option<i64> = conn
            .query_row(
                "SELECT duplicate_of FROM messages WHERE id = ?1",
                params![a],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(keep, None);
    }

    #[test]
    fn integration_near_flags_within_window() {
        let mut conn = setup_db();
        let a = insert_msg(
            &conn,
            "go-sms-pro",
            "g1",
            "2015-03-12T18:04:22Z",
            "2015-03-12T14:04:22-04:00",
            0,
            "On my way",
            0,
        );
        let b = insert_msg(
            &conn,
            "sms-backup-plus",
            "g2",
            "2015-03-12T18:04:24Z",
            "2015-03-12T14:04:24-04:00",
            0,
            "On my way",
            1,
        );
        let priority = ["go-sms-pro".into(), "sms-backup-plus".into()];
        let stats = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, Some(&priority), 2).unwrap();
        assert_eq!(stats.exact_flagged, 0);
        assert_eq!(stats.near_flagged, 1);
        let dup: Option<i64> = conn
            .query_row(
                "SELECT duplicate_of FROM messages WHERE id = ?1",
                params![b],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dup, Some(a));
    }

    #[test]
    fn integration_negative_far_apart_not_flagged() {
        let mut conn = setup_db();
        insert_msg(
            &conn,
            "go-sms-pro",
            "g1",
            "2015-03-12T18:04:22Z",
            "2015-03-12T14:04:22-04:00",
            0,
            "On my way",
            0,
        );
        insert_msg(
            &conn,
            "sms-backup-plus",
            "g2",
            "2015-03-12T18:05:22Z",
            "2015-03-12T14:05:22-04:00",
            0,
            "On my way",
            1,
        );
        let priority = ["go-sms-pro".into(), "sms-backup-plus".into()];
        let stats = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, Some(&priority), 2).unwrap();
        assert_eq!(stats.exact_flagged, 0);
        assert_eq!(stats.near_flagged, 0);
        let hidden: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE duplicate_of IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hidden, 0);
    }

    #[test]
    fn integration_priority_prefers_first_imported_source() {
        let mut conn = setup_db();
        // First row wins when priority is derived from min(message id) per source.
        let first_imported = insert_msg(
            &conn,
            "sms-backup-plus",
            "g1",
            "2015-03-12T18:04:22Z",
            "2015-03-12T14:04:22-04:00",
            1,
            "Hello",
            0,
        );
        let second_imported = insert_msg(
            &conn,
            "go-sms-pro",
            "g2",
            "2015-03-12T18:04:22Z",
            "2015-03-12T14:04:22-04:00",
            1,
            "Hello",
            1,
        );
        dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, None, 2).unwrap();
        let dup_first: Option<i64> = conn
            .query_row(
                "SELECT duplicate_of FROM messages WHERE id = ?1",
                params![first_imported],
                |row| row.get(0),
            )
            .unwrap();
        let dup_second: Option<i64> = conn
            .query_row(
                "SELECT duplicate_of FROM messages WHERE id = ?1",
                params![second_imported],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dup_first, None);
        assert_eq!(dup_second, Some(first_imported));
    }
}
