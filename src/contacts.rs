use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Default)]
pub struct ContactLoadStats {
    pub contacts: u64,
    pub phones: u64,
    pub labels: u64,
    pub emails_restored: u64,
    pub skipped: bool,
}

#[derive(Debug)]
struct ContactCsvRow {
    phones: String,
    first_name: String,
    last_name: String,
    exclude: String,
    labels: Vec<String>,
}

/// iMessage-style: any handle containing `@` is treated as email.
fn is_email_handle(handle: &str) -> bool {
    handle.contains('@')
}

fn phone_handles_only(handles: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for h in handles {
        if is_email_handle(h) {
            continue;
        }
        let Some(e164) = crate::phone::to_e164(h) else {
            continue;
        };
        if !out.iter().any(|p| p == &e164) {
            out.push(e164);
        }
    }
    out
}

/// Emails attached to a contact, keyed for restore by that contact's phone set.
#[derive(Debug, Default)]
struct EmailSnapshot {
    /// One entry per contact that had emails: (phones on that contact, emails).
    entries: Vec<(HashSet<String>, Vec<String>)>,
}

fn snapshot_email_handles(conn: &Connection, account_id: &str) -> Result<EmailSnapshot> {
    let mut by_contact: HashMap<i64, (HashSet<String>, Vec<String>)> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT contact_id, handle FROM contact_handles WHERE account_id = ?1 ORDER BY contact_id, handle",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (contact_id, handle) = row?;
        let entry = by_contact.entry(contact_id).or_default();
        if is_email_handle(&handle) {
            entry.1.push(handle);
        } else {
            entry.0.insert(handle);
        }
    }
    Ok(EmailSnapshot {
        entries: by_contact
            .into_values()
            .filter(|(_, emails)| !emails.is_empty())
            .collect(),
    })
}

fn restore_email_handles(
    conn: &Connection,
    account_id: &str,
    snapshot: &EmailSnapshot,
) -> Result<u64> {
    if snapshot.entries.is_empty() {
        return Ok(0);
    }

    let mut restored = 0u64;
    for (phones, emails) in &snapshot.entries {
        let mut contact_id: Option<i64> = None;
        for phone in phones {
            let found: Option<i64> = conn
                .query_row(
                    "SELECT contact_id FROM contact_handles WHERE account_id = ?1 AND handle = ?2",
                    params![account_id, phone],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(id) = found {
                contact_id = Some(id);
                break;
            }
        }
        let Some(id) = contact_id else {
            continue;
        };
        for email in emails {
            let owner: Option<i64> = conn
                .query_row(
                    "SELECT contact_id FROM contact_handles WHERE account_id = ?1 AND handle = ?2",
                    params![account_id, email],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(existing) = owner {
                if existing != id {
                    eprintln!(
                        "warning: email handle {email} already belongs to contact {existing}; not restoring onto {id}"
                    );
                }
                continue;
            }
            conn.execute(
                "INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?1, ?2, ?3)",
                params![account_id, email, id],
            )?;
            restored += 1;
        }
    }
    Ok(restored)
}

/// Load contacts from CSV when the account table is empty or when `overwrite` is true.
///
/// On overwrite, email handles already in SQLite are snapshotted by phone set
/// and reattached after CSV reload (contacts.csv is phone-only).
pub fn load_contacts_if_needed(
    conn: &mut Connection,
    csv_path: &Path,
    overwrite: bool,
    account_id: &str,
) -> Result<ContactLoadStats> {
    crate::schema::ensure_contacts_schema(conn)?;
    crate::vault_owner::ensure_account_row(conn, account_id)?;
    if !crate::schema::contacts_schema_ready(conn)? {
        eprintln!("contacts: schema not current; recreating tables before CSV load");
        crate::schema::recreate_contacts(conn)?;
    }

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM contacts WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if count > 0 && !overwrite {
        return Ok(ContactLoadStats {
            skipped: true,
            ..Default::default()
        });
    }

    let email_snapshot = if count > 0 && overwrite {
        snapshot_email_handles(conn, account_id)?
    } else {
        EmailSnapshot::default()
    };

    delete_account_contacts(conn, account_id)?;

    if !csv_path.exists() {
        eprintln!(
            "warning: contacts CSV not found at {}; leaving contacts empty",
            csv_path.display()
        );
        return Ok(ContactLoadStats::default());
    }

    let mut stats = load_from_csv(conn, csv_path, account_id)?;
    stats.emails_restored = restore_email_handles(conn, account_id, &email_snapshot)?;
    if stats.emails_restored > 0 {
        eprintln!(
            "contacts: restored {} email handle(s) from previous DB (CSV is phone-only)",
            stats.emails_restored
        );
    }
    Ok(stats)
}

fn delete_account_contacts(conn: &Connection, account_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM contact_label_members WHERE contact_id IN (SELECT id FROM contacts WHERE account_id = ?1)",
        params![account_id],
    )?;
    conn.execute(
        "DELETE FROM contact_handles WHERE account_id = ?1",
        params![account_id],
    )?;
    conn.execute(
        "DELETE FROM contact_labels WHERE account_id = ?1",
        params![account_id],
    )?;
    conn.execute(
        "DELETE FROM contacts WHERE account_id = ?1",
        params![account_id],
    )?;
    Ok(())
}

fn load_from_csv(
    conn: &mut Connection,
    csv_path: &Path,
    account_id: &str,
) -> Result<ContactLoadStats> {
    let file = File::open(csv_path)
        .with_context(|| format!("failed to open contacts CSV {}", csv_path.display()))?;
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(file);
    let headers = reader
        .headers()
        .with_context(|| format!("failed to read contacts CSV header in {}", csv_path.display()))?
        .clone();
    let col = |name: &str| -> Option<usize> {
        headers.iter().position(|h| h == name)
    };
    let phones_i = col("phones").ok_or_else(|| {
        anyhow::anyhow!("contacts CSV missing phones column ({})", csv_path.display())
    })?;
    let first_i = col("first_name");
    let last_i = col("last_name");
    let exclude_i = col("exclude").ok_or_else(|| {
        anyhow::anyhow!("contacts CSV missing exclude column ({})", csv_path.display())
    })?;
    let label_slots = label_column_slots(&headers);
    if label_slots.is_empty() {
        bail!(
            "contacts CSV missing label_N (or legacy group_N) columns ({})",
            csv_path.display()
        );
    }

    let mut stats = ContactLoadStats::default();
    let mut seen_phones: HashSet<String> = HashSet::new();
    let tx = conn.transaction()?;

    for (row_no, result) in reader.records().enumerate() {
        let row_no = row_no + 2; // header is line 1
        let record = result.with_context(|| {
            format!(
                "failed to parse contacts CSV row {row_no} in {}",
                csv_path.display()
            )
        })?;
        let field = |i: usize| -> String {
            record.get(i).unwrap_or("").trim().to_string()
        };
        let row = ContactCsvRow {
            phones: field(phones_i),
            first_name: first_i.map(field).unwrap_or_default(),
            last_name: last_i.map(field).unwrap_or_default(),
            exclude: field(exclude_i),
            labels: row_labels_from_slots(&record, &label_slots),
        };

        let raw_handles = split_list(&row.phones);
        for h in &raw_handles {
            if is_email_handle(h) {
                eprintln!(
                    "warning: contacts CSV row {row_no}: skipping email handle {h} (emails are DB-only)"
                );
            }
        }
        let phones = phone_handles_only(&raw_handles);

        if phones.is_empty() {
            bail!(
                "contacts CSV row {row_no}: phones is required ({})",
                csv_path.display()
            );
        }

        for phone in &phones {
            if !seen_phones.insert(phone.clone()) {
                bail!(
                    "contacts CSV: duplicate phone {phone} (row {row_no} in {})",
                    csv_path.display()
                );
            }
        }

        let preferred = phones[0].clone();
        let legacy_inactive = parse_bool(&row.exclude);
        let first_name = empty_to_none(&row.first_name);
        let last_name = empty_to_none(&row.last_name);
        let preferred_name = join_preferred_name(first_name.as_deref(), last_name.as_deref());

        tx.execute(
            r#"
            INSERT INTO contacts (
                account_id, first_name, last_name, preferred_name, exclude, preferred_handle
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                account_id,
                first_name,
                last_name,
                preferred_name,
                0,
                preferred
            ],
        )?;
        let contact_id = tx.last_insert_rowid();
        stats.contacts += 1;

        for phone in &phones {
            tx.execute(
                "INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?1, ?2, ?3)",
                params![account_id, phone, contact_id],
            )?;
            stats.phones += 1;
        }

        let mut labels = row.labels.clone();
        labels.retain(|label| {
            !label.eq_ignore_ascii_case("Active") && !label.eq_ignore_ascii_case("Inactive")
        });
        let status_label = if legacy_inactive {
            "Inactive"
        } else {
            "Active"
        };
        labels.push(status_label.to_string());
        for label_name in &labels {
            let label_id = ensure_label(&tx, account_id, label_name)?;
            tx.execute(
                "INSERT OR IGNORE INTO contact_label_members (contact_id, label_id) VALUES (?1, ?2)",
                params![contact_id, label_id],
            )?;
            stats.labels += 1;
        }
    }

    tx.commit()?;
    Ok(stats)
}

fn ensure_label(conn: &Connection, account_id: &str, name: &str) -> Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO contact_labels (account_id, name) VALUES (?1, ?2)",
        params![account_id, name],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM contact_labels WHERE account_id = ?1 AND name = ?2",
        params![account_id, name],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// Normalized comparison key for owner-handle matching (E.164 phone / lowercased email).
fn handle_match_key(handle: &str) -> String {
    let trimmed = handle.trim();
    if is_email_handle(trimmed) {
        return trimmed.to_lowercase();
    }
    crate::phone::to_e164(trimmed).unwrap_or_else(|| trimmed.to_string())
}

/// Create contacts for handles that have messages but no contact_handles row:
/// 1:1 handles, plus group participants who never had a 1:1 conversation.
/// Names come from participant `name_hint` / exporter `display_name` when present.
/// Phone handles are appended to `contacts.csv`; emails stay DB-only.
pub fn ensure_unknown_contacts(
    conn: &mut Connection,
    account_id: &str,
    contacts_csv: &Path,
) -> Result<u64> {
    crate::schema::ensure_contacts_schema(conn)?;

    let has_trash: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'trashed_handles'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    let has_trashed_conversations: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'trashed_conversations'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    let trash_sql = if has_trash {
        "AND NOT EXISTS (
           SELECT 1 FROM trashed_handles th
           WHERE th.handle = c.chat_identifier AND th.account_id = c.account_id
         )"
    } else {
        ""
    };

    let sql = format!(
        "SELECT DISTINCT c.chat_identifier
         FROM conversations c
         JOIN messages m ON m.conversation_id = c.id
         WHERE c.account_id = ?1
           AND c.conversation_type = 'individual'
           AND NOT EXISTS (
             SELECT 1 FROM contact_handles cp
             WHERE cp.handle = c.chat_identifier AND cp.account_id = c.account_id
           )
           {trash_sql}
         ORDER BY c.chat_identifier"
    );

    let mut handles: Vec<String> = {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![account_id], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out
    };

    // Group participants with no 1:1 thread are never reached by the query above,
    // so their group messages would belong to no contact.
    let group_trash_handle_sql = if has_trash {
        "AND NOT EXISTS (
           SELECT 1 FROM trashed_handles th
           WHERE th.handle = p.handle AND th.account_id = c.account_id
         )"
    } else {
        ""
    };
    let group_trash_conv_sql = if has_trashed_conversations {
        "AND NOT EXISTS (
           SELECT 1 FROM trashed_conversations tc
           WHERE tc.conversation_id = c.id AND tc.account_id = c.account_id
         )"
    } else {
        ""
    };
    let group_sql = format!(
        "SELECT DISTINCT p.handle
         FROM participants p
         JOIN conversations c ON c.id = p.conversation_id
         WHERE c.account_id = ?1
           AND c.conversation_type = 'group'
           AND trim(coalesce(p.handle, '')) <> ''
           AND NOT EXISTS (
             SELECT 1 FROM contact_handles cp
             WHERE cp.handle = p.handle AND cp.account_id = c.account_id
           )
           AND EXISTS (
             SELECT 1 FROM messages m WHERE m.conversation_id = c.id
           )
           {group_trash_handle_sql}
           {group_trash_conv_sql}
         ORDER BY p.handle"
    );
    {
        let mut stmt = conn.prepare(&group_sql)?;
        let rows = stmt.query_map(params![account_id], |row| row.get::<_, String>(0))?;
        let mut seen: HashSet<String> = handles.iter().cloned().collect();
        for row in rows {
            let handle = row?;
            if seen.insert(handle.clone()) {
                handles.push(handle);
            }
        }
    }

    // The account holder is a participant in their own groups.
    let owner_keys: HashSet<String> = crate::vault_owner::load_vault_owner(conn, account_id)
        .map(|owner| {
            owner
                .phones
                .iter()
                .chain(owner.emails.iter())
                .map(|h| handle_match_key(h))
                .collect()
        })
        .unwrap_or_default();
    if !owner_keys.is_empty() {
        handles.retain(|handle| !owner_keys.contains(&handle_match_key(handle)));
    }

    if handles.is_empty() {
        return Ok(0);
    }

    let mut created = 0u64;
    let mut created_named: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    let tx = conn.transaction()?;
    for handle in &handles {
        let preferred = handle.clone();
        let hint = best_name_hint_for_handle(&tx, account_id, handle)?;
        let (first_name, last_name) = split_display_name(hint.as_deref());
        // Full hint is the preferred display name; first/last stay for CSV/search.
        let preferred_name = hint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| join_preferred_name(first_name.as_deref(), last_name.as_deref()));
        tx.execute(
            r#"
            INSERT INTO contacts (
                account_id, first_name, last_name, preferred_name, exclude, preferred_handle
            ) VALUES (?1, ?2, ?3, ?4, 0, ?5)
            "#,
            params![account_id, first_name, last_name, preferred_name, preferred],
        )?;
        let contact_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?1, ?2, ?3)",
            params![account_id, handle, contact_id],
        )?;
        created += 1;
        created_named.push((handle.clone(), first_name, last_name));
    }
    tx.commit()?;

    for (handle, first_name, last_name) in &created_named {
        if is_email_handle(handle) {
            continue;
        }
        let csv_phone = crate::phone::to_e164(handle).unwrap_or_else(|| handle.clone());
        if let Err(err) = append_contact_csv_row(
            contacts_csv,
            &csv_phone,
            first_name.as_deref(),
            last_name.as_deref(),
        ) {
            eprintln!(
                "warning: could not append {csv_phone} to {}: {err}",
                contacts_csv.display()
            );
        }
    }

    Ok(created)
}

/// Fill empty contact first/last names from participant name hints (exporter display names).
///
/// Does not overwrite names the user (or contacts CSV) already set.
pub fn fill_empty_contact_names_from_participants(
    conn: &mut Connection,
    account_id: &str,
) -> Result<u64> {
    crate::schema::ensure_contacts_schema(conn)?;

    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            r#"
            SELECT c.id, ch.handle
            FROM contacts c
            JOIN contact_handles ch
              ON ch.contact_id = c.id AND ch.account_id = c.account_id
            WHERE c.account_id = ?1
              AND (
                (
                  (c.first_name IS NULL OR TRIM(c.first_name) = '')
                  AND (c.last_name IS NULL OR TRIM(c.last_name) = '')
                )
                OR (c.preferred_name IS NULL OR TRIM(c.preferred_name) = '')
              )
            ORDER BY c.id, ch.handle
            "#,
        )?;
        let mapped = stmt.query_map(params![account_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            out.push(row?);
        }
        out
    };

    // One best hint per contact (prefer longer useful hint across its handles).
    let mut best: HashMap<i64, String> = HashMap::new();
    for (contact_id, handle) in rows {
        let Some(hint) = best_name_hint_for_handle(conn, account_id, &handle)? else {
            continue;
        };
        best.entry(contact_id)
            .and_modify(|existing| {
                if hint.len() > existing.len() {
                    *existing = hint.clone();
                }
            })
            .or_insert(hint);
    }

    if best.is_empty() {
        return Ok(0);
    }

    let mut filled = 0u64;
    let tx = conn.transaction()?;
    for (contact_id, hint) in best {
        let (first_name, last_name) = split_display_name(Some(&hint));
        if first_name.is_none() && last_name.is_none() {
            continue;
        }
        let preferred_name = hint.trim().to_string();
        let n = tx.execute(
            r#"
            UPDATE contacts
            SET first_name = COALESCE(NULLIF(TRIM(first_name), ''), ?2),
                last_name = COALESCE(NULLIF(TRIM(last_name), ''), ?3),
                preferred_name = COALESCE(NULLIF(TRIM(preferred_name), ''), ?5)
            WHERE id = ?1
              AND account_id = ?4
              AND (
                (first_name IS NULL OR TRIM(first_name) = '')
                AND (last_name IS NULL OR TRIM(last_name) = '')
                OR (preferred_name IS NULL OR TRIM(preferred_name) = '')
              )
            "#,
            params![
                contact_id,
                first_name,
                last_name,
                account_id,
                preferred_name
            ],
        )?;
        filled += n as u64;
    }
    tx.commit()?;
    Ok(filled)
}

fn best_name_hint_for_handle(
    conn: &Connection,
    account_id: &str,
    handle: &str,
) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT p.name_hint
        FROM participants p
        JOIN conversations c ON c.id = p.conversation_id
        WHERE c.account_id = ?1
          AND p.handle = ?2
          AND p.name_hint IS NOT NULL
          AND TRIM(p.name_hint) != ''
        ORDER BY LENGTH(TRIM(p.name_hint)) DESC, p.name_hint ASC
        "#,
    )?;
    let hints = stmt.query_map(params![account_id, handle], |row| {
        row.get::<_, String>(0)
    })?;
    for hint in hints {
        let hint = hint?;
        if let Some(useful) = useful_name_hint(&hint, handle) {
            return Ok(Some(useful));
        }
    }
    Ok(None)
}

/// Prefer a real display hint; ignore phones and placeholder "(Unknown)" labels.
fn useful_name_hint(hint: &str, handle: &str) -> Option<String> {
    let t = hint.trim();
    if t.is_empty() {
        return None;
    }
    if looks_like_phone(t) {
        return None;
    }
    if t.eq_ignore_ascii_case(handle) {
        return None;
    }
    if matches!(t.to_ascii_lowercase().as_str(), "unknown" | "(unknown)") {
        return None;
    }
    Some(t.to_string())
}

fn looks_like_phone(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with('+') && t.chars().all(|c| c.is_ascii_digit() || "+ ().-".contains(c)) {
        return true;
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    let stripped: String = t
        .chars()
        .filter(|c| !c.is_whitespace() && !"()+-.".contains(*c))
        .collect();
    digits.len() >= 7 && digits.len() == stripped.len()
}

/// Split `"Annette Gubert"` → (`Annette`, `Gubert`); single token → first only.
fn join_preferred_name(first_name: Option<&str>, last_name: Option<&str>) -> Option<String> {
    let first = first_name.map(str::trim).filter(|s| !s.is_empty());
    let last = last_name.map(str::trim).filter(|s| !s.is_empty());
    match (first, last) {
        (Some(f), Some(l)) => Some(format!("{f} {l}")),
        (Some(f), None) => Some(f.to_string()),
        (None, Some(l)) => Some(l.to_string()),
        (None, None) => None,
    }
}

fn split_display_name(hint: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(raw) = hint.map(str::trim).filter(|s| !s.is_empty()) else {
        return (None, None);
    };
    let mut parts = raw.split_whitespace();
    let Some(first) = parts.next() else {
        return (None, None);
    };
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        (Some(first.to_string()), None)
    } else {
        (Some(first.to_string()), Some(rest.join(" ")))
    }
}

fn append_contact_csv_row(
    csv_path: &Path,
    phone: &str,
    first_name: Option<&str>,
    last_name: Option<&str>,
) -> Result<()> {
    use std::io::Write;

    if !csv_path.exists() {
        bail!("contacts CSV not found at {}", csv_path.display());
    }
    let raw = std::fs::read_to_string(csv_path)
        .with_context(|| format!("failed to read {}", csv_path.display()))?;
    let header_line = raw.lines().next().unwrap_or("");
    let header: Vec<&str> = header_line.split(',').collect();
    let phones_i = header
        .iter()
        .position(|h| *h == "phones")
        .ok_or_else(|| anyhow::anyhow!("contacts CSV missing phones column"))?;
    let exclude_i = header
        .iter()
        .position(|h| *h == "exclude")
        .ok_or_else(|| anyhow::anyhow!("contacts CSV missing exclude column"))?;
    let first_i = header.iter().position(|h| *h == "first_name");
    let last_i = header.iter().position(|h| *h == "last_name");

    let mut cols: Vec<String> = header.iter().map(|_| String::new()).collect();
    cols[phones_i] = phone.to_string();
    cols[exclude_i] = "false".to_string();
    if let (Some(i), Some(name)) = (first_i, first_name) {
        cols[i] = name.to_string();
    }
    if let (Some(i), Some(name)) = (last_i, last_name) {
        cols[i] = name.to_string();
    }

    let line = cols
        .iter()
        .map(|c| {
            if c.contains(',') || c.contains('"') || c.contains('\n') {
                format!("\"{}\"", c.replace('"', "\"\""))
            } else {
                c.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(csv_path)
        .with_context(|| format!("failed to open {}", csv_path.display()))?;
    if !raw.is_empty() && !raw.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    writeln!(file, "{line}")?;
    Ok(())
}

/// Ordered label slots: prefer `label_N` over legacy `group_N` for each index.
fn label_column_slots(headers: &csv::StringRecord) -> Vec<(Option<usize>, Option<usize>)> {
    let mut max_n = 0usize;
    for h in headers.iter() {
        if let Some(n) = parse_numbered_column(h, "label_") {
            max_n = max_n.max(n);
        } else if let Some(n) = parse_numbered_column(h, "group_") {
            max_n = max_n.max(n);
        }
    }
    if max_n == 0 {
        return Vec::new();
    }
    (1..=max_n)
        .map(|n| {
            let label = headers
                .iter()
                .position(|h| h == format!("label_{n}"));
            let group = headers
                .iter()
                .position(|h| h == format!("group_{n}"));
            (label, group)
        })
        .filter(|(label, group)| label.is_some() || group.is_some())
        .collect()
}

fn parse_numbered_column(header: &str, prefix: &str) -> Option<usize> {
    let rest = header.strip_prefix(prefix)?;
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: usize = rest.parse().ok()?;
    (n >= 1).then_some(n)
}

fn row_labels_from_slots(
    record: &csv::StringRecord,
    slots: &[(Option<usize>, Option<usize>)],
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (label_i, group_i) in slots {
        let label = label_i
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .trim();
        let group = group_i
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .trim();
        let raw = if !label.is_empty() { label } else { group };
        if raw.is_empty() {
            continue;
        }
        let key = raw.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        out.push(raw.to_string());
    }
    out
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn empty_to_none(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn parse_bool(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_detection() {
        assert!(is_email_handle("a@b.com"));
        assert!(!is_email_handle("+15551234567"));
        assert_eq!(
            phone_handles_only(&[
                "+15551234567".into(),
                "a@b.com".into(),
                "+15559876543".into()
            ]),
            vec!["+15551234567", "+15559876543"]
        );
    }

    #[test]
    fn join_preferred_name_parts() {
        assert_eq!(
            join_preferred_name(Some("Ann"), Some("Lee")).as_deref(),
            Some("Ann Lee")
        );
        assert_eq!(join_preferred_name(Some("Madonna"), None).as_deref(), Some("Madonna"));
        assert_eq!(join_preferred_name(None, Some("Prince")).as_deref(), Some("Prince"));
        assert_eq!(join_preferred_name(None, None), None);
    }

    #[test]
    fn split_display_name_parts() {
        assert_eq!(
            split_display_name(Some("Annette Gubert")),
            (Some("Annette".into()), Some("Gubert".into()))
        );
        assert_eq!(
            split_display_name(Some("Madonna")),
            (Some("Madonna".into()), None)
        );
        assert_eq!(
            split_display_name(Some("  Mary Ann  Smith ")),
            (Some("Mary".into()), Some("Ann Smith".into()))
        );
        assert_eq!(split_display_name(None), (None, None));
    }

    #[test]
    fn useful_name_hint_filters_phones() {
        assert_eq!(
            useful_name_hint("Annette Gubert", "+19124011522").as_deref(),
            Some("Annette Gubert")
        );
        assert_eq!(useful_name_hint("+19124011522", "+19124011522"), None);
        assert_eq!(useful_name_hint("(Unknown)", "+19124011522"), None);
    }

    const TEST_ACCOUNT_ID: &str = "00000000-0000-0000-0000-000000000042";
    const OWNER_PHONE: &str = "+15555550100";
    const GROUP_ONLY_PHONE: &str = "+15555550111";
    const DIRECT_PHONE: &str = "+15555550222";

    /// Group with the owner, a group-only participant, and a 1:1 participant.
    fn seed_group_vault(db_path: &Path) -> Connection {
        let conn = Connection::open(db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::schema::ensure_vault_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES (?1, 'test', 0, 'Vault Owner')",
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO account_phones (account_id, phone) VALUES (?1, ?2)",
            params![TEST_ACCOUNT_ID, OWNER_PHONE],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO conversations (
                 account_id, chat_identifier, service, conversation_type,
                 group_title, exported_at, source_file
             ) VALUES (?1, 'chat-1', 'SMS', 'group', 'Crew', NULL, 't.json')",
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();
        let group_id = conn.last_insert_rowid();
        for (handle, hint) in [
            (OWNER_PHONE, "Vault Owner"),
            (GROUP_ONLY_PHONE, "Group Only"),
            (DIRECT_PHONE, "Direct Friend"),
        ] {
            conn.execute(
                "INSERT INTO participants (conversation_id, handle, name_hint)
                 VALUES (?1, ?2, ?3)",
                params![group_id, handle, hint],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO messages (
                 conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
             ) VALUES (?1, ?2, 'imessage', 'g-group', '2023-06-01T10:00:00Z', 0, 0, 'hi crew')",
            params![group_id, TEST_ACCOUNT_ID],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO conversations (
                 account_id, chat_identifier, service, conversation_type,
                 group_title, exported_at, source_file
             ) VALUES (?1, ?2, 'SMS', 'individual', NULL, NULL, 't.json')",
            params![TEST_ACCOUNT_ID, DIRECT_PHONE],
        )
        .unwrap();
        let direct_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO messages (
                 conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
             ) VALUES (?1, ?2, 'imessage', 'g-direct', '2023-06-02T10:00:00Z', 0, 0, 'hi there')",
            params![direct_id, TEST_ACCOUNT_ID],
        )
        .unwrap();

        conn
    }

    #[test]
    fn ensure_unknown_contacts_covers_group_only_participants() {
        let dir = std::env::temp_dir().join(format!(
            "mv-contacts-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("vault.db");
        let csv_path = dir.join("contacts.csv");

        let mut conn = seed_group_vault(&db_path);
        let created = ensure_unknown_contacts(&mut conn, TEST_ACCOUNT_ID, &csv_path).unwrap();
        assert_eq!(created, 2, "group-only and 1:1 handles both get contacts");

        let handles: Vec<String> = conn
            .prepare("SELECT handle FROM contact_handles WHERE account_id = ?1 ORDER BY handle")
            .unwrap()
            .query_map(params![TEST_ACCOUNT_ID], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(handles.iter().any(|h| h == GROUP_ONLY_PHONE));
        assert!(handles.iter().any(|h| h == DIRECT_PHONE));
        assert!(
            !handles.iter().any(|h| h == OWNER_PHONE),
            "the account holder must not become a contact: {handles:?}"
        );

        // Second run is a no-op now that every handle has a contact.
        let again = ensure_unknown_contacts(&mut conn, TEST_ACCOUNT_ID, &csv_path).unwrap();
        assert_eq!(again, 0);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
