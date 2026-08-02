use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use contacts::{
    ContactsFormat, detect_contacts_format, extract_tags, parse_vcf, read_vcard_csv_rows, strip_tags,
};
use phone::sanitize_number;
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
struct ContactDraft {
    phones: Vec<String>,
    preferred_name: Option<String>,
    labels: Vec<String>,
}

fn contacts_file_format(path: &Path) -> Result<ContactsFormat> {
    detect_contacts_format(path).map_err(|e| {
        if e.details.is_empty() {
            anyhow::anyhow!("{}", e.message)
        } else {
            anyhow::anyhow!("{} ({})", e.message, e.details.join("; "))
        }
    })
}

/// iMessage-style: any handle containing `@` is treated as email.
fn is_email_handle(handle: &str) -> bool {
    handle.contains('@')
}

/// Raw phone → E.164 when unambiguous enough for the shared `phone` crate.
fn to_e164(num: &str) -> Option<String> {
    let trimmed = num.trim();
    if trimmed.is_empty() || trimmed.contains('@') {
        return None;
    }
    sanitize_number(trimmed).map(|digits| phone::to_e164(&digits))
}

fn phone_handles_only(handles: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for h in handles {
        if is_email_handle(h) {
            continue;
        }
        let Some(e164) = to_e164(h) else {
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

/// Load contacts from an address book when the account table is empty or when
/// `overwrite` is true.
///
/// Accepted files: **VCF**, or **vCard CSV** (First Name, Last Name, Phone
/// columns — a contacts app VCF exported as CSV).
///
/// Pass `None` to skip address-book load (keep existing SQLite contacts).
/// On overwrite, email handles already in SQLite are snapshotted by phone set
/// and reattached after reload (address-book files are phone-oriented).
pub fn load_contacts_if_needed(
    conn: &mut Connection,
    contacts_path: Option<&Path>,
    overwrite: bool,
    account_id: &str,
) -> Result<ContactLoadStats> {
    crate::db::schema::ensure_contacts_schema(conn)?;
    crate::db::account_profile::ensure_account_row(conn, account_id)?;

    let Some(path) = contacts_path else {
        return Ok(ContactLoadStats {
            skipped: true,
            ..Default::default()
        });
    };

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

    if !path.exists() {
        eprintln!(
            "warning: contacts file not found at {}; leaving contacts empty",
            path.display()
        );
        if count > 0 && overwrite {
            delete_account_contacts(conn, account_id)?;
        }
        return Ok(ContactLoadStats::default());
    }

    let email_snapshot = if count > 0 && overwrite {
        snapshot_email_handles(conn, account_id)?
    } else {
        EmailSnapshot::default()
    };

    delete_account_contacts(conn, account_id)?;

    let format = contacts_file_format(path)?;
    let mut stats = match format {
        ContactsFormat::VcardCsv => load_from_vcard_csv(conn, path, account_id)?,
        ContactsFormat::Vcf => load_from_vcf(conn, path, account_id)?,
    };
    stats.emails_restored = restore_email_handles(conn, account_id, &email_snapshot)?;
    if stats.emails_restored > 0 {
        eprintln!(
            "contacts: restored {} email handle(s) from previous DB (address book is phone-only)",
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

fn load_from_vcard_csv(
    conn: &mut Connection,
    csv_path: &Path,
    account_id: &str,
) -> Result<ContactLoadStats> {
    let rows = read_vcard_csv_rows(csv_path)
        .with_context(|| format!("failed to read contacts CSV {}", csv_path.display()))?;
    let mut drafts = Vec::new();
    for row in rows {
        if row.phones.is_empty() {
            continue;
        }
        let mut name_parts = Vec::new();
        if !row.first.is_empty() {
            name_parts.push(row.first.as_str());
        }
        if !row.middle.is_empty() {
            name_parts.push(row.middle.as_str());
        }
        if !row.last.is_empty() {
            name_parts.push(row.last.as_str());
        }
        let preferred_name = {
            let joined = name_parts.join(" ");
            let collapsed = collapse_inner_whitespace(&joined);
            if collapsed.is_empty() {
                None
            } else {
                Some(collapsed)
            }
        };
        let phones = phone_handles_only(&row.phones);
        if phones.is_empty() {
            continue;
        }
        drafts.push(ContactDraft {
            phones,
            preferred_name,
            labels: Vec::new(),
        });
    }

    insert_contact_drafts(conn, account_id, drafts)
}

fn load_from_vcf(
    conn: &mut Connection,
    vcf_path: &Path,
    account_id: &str,
) -> Result<ContactLoadStats> {
    let cards = parse_vcf(vcf_path)?;
    let mut drafts = Vec::new();
    for card in cards {
        let phones = phone_handles_only(&card.phones);
        if phones.is_empty() {
            continue;
        }

        let (fn_stripped, fn_tags) = extract_tags(&card.fn_raw);
        let first = strip_tags(&card.n_given);
        let middle = strip_tags(&card.n_middle);
        let last = strip_tags(&card.n_family);

        let nickname = if last.is_empty()
            && !fn_stripped.is_empty()
            && !fn_stripped.contains(' ')
            && (first.is_empty() || first == fn_stripped)
        {
            Some(fn_stripped.clone())
        } else {
            None
        };

        let preferred_name = if let Some(nick) = nickname {
            Some(nick)
        } else {
            let mut parts = Vec::new();
            if !first.is_empty() {
                parts.push(first.as_str());
            }
            if !middle.is_empty() {
                parts.push(middle.as_str());
            }
            if !last.is_empty() {
                parts.push(last.as_str());
            }
            let from_n = collapse_inner_whitespace(&parts.join(" "));
            if !from_n.is_empty() {
                Some(from_n)
            } else {
                let from_fn = collapse_inner_whitespace(&fn_stripped);
                if from_fn.is_empty() {
                    None
                } else {
                    Some(from_fn)
                }
            }
        };

        let mut labels = Vec::new();
        for tag in fn_tags {
            let t = tag.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("People") {
                continue;
            }
            labels.push(t.to_string());
        }
        for category in &card.categories {
            let t = category.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("People") {
                continue;
            }
            if !labels.iter().any(|l| l.eq_ignore_ascii_case(t)) {
                labels.push(t.to_string());
            }
        }

        drafts.push(ContactDraft {
            phones,
            preferred_name,
            labels,
        });
    }

    insert_contact_drafts(conn, account_id, drafts)
}

fn collapse_inner_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn insert_contact_drafts(
    conn: &mut Connection,
    account_id: &str,
    drafts: Vec<ContactDraft>,
) -> Result<ContactLoadStats> {
    let mut stats = ContactLoadStats::default();
    let drafts = merge_duplicate_phone_drafts(drafts);
    let tx = conn.transaction()?;

    for draft in drafts {
        let preferred = draft.phones[0].clone();
        tx.execute(
            r#"
            INSERT INTO contacts (
                account_id, preferred_name, preferred_handle
            ) VALUES (?1, ?2, ?3)
            "#,
            params![account_id, draft.preferred_name, preferred],
        )?;
        let contact_id = tx.last_insert_rowid();
        stats.contacts += 1;

        for phone in &draft.phones {
            tx.execute(
                "INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?1, ?2, ?3)",
                params![account_id, phone, contact_id],
            )?;
            stats.phones += 1;
        }

        let labels = draft.labels;
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

/// Merge address-book rows that share any phone, including transitive overlaps.
fn merge_duplicate_phone_drafts(drafts: Vec<ContactDraft>) -> Vec<ContactDraft> {
    let mut merged: Vec<ContactDraft> = Vec::new();
    for mut draft in drafts {
        let mut matching: Vec<usize> = merged
            .iter()
            .enumerate()
            .filter(|(_, existing)| {
                existing
                    .phones
                    .iter()
                    .any(|phone| draft.phones.contains(phone))
            })
            .map(|(index, _)| index)
            .collect();
        if matching.is_empty() {
            merged.push(draft);
            continue;
        }

        let target = matching.remove(0);
        merge_contact_draft(&mut merged[target], draft);
        for index in matching.into_iter().rev() {
            draft = merged.remove(index);
            let adjusted_target = if index < target { target - 1 } else { target };
            merge_contact_draft(&mut merged[adjusted_target], draft);
        }
    }
    merged
}

fn merge_contact_draft(into: &mut ContactDraft, from: ContactDraft) {
    if into.preferred_name.is_none() {
        into.preferred_name = from.preferred_name;
    }
    for phone in from.phones {
        if !into.phones.contains(&phone) {
            into.phones.push(phone);
        }
    }
    for label in from.labels {
        if !into
            .labels
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&label))
        {
            into.labels.push(label);
        }
    }
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
    to_e164(trimmed).unwrap_or_else(|| trimmed.to_string())
}

/// Create contacts for handles that have messages but no contact_handles row:
/// 1:1 handles, plus group participants who never had a 1:1 conversation.
/// Names come from participant `name_hint` / exporter `display_name` when present.
pub fn ensure_unknown_contacts(conn: &mut Connection, account_id: &str) -> Result<u64> {
    crate::db::schema::ensure_contacts_schema(conn)?;

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
    let owner_keys: HashSet<String> =
        crate::db::account_profile::load_account_profile(conn, account_id)
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
    let tx = conn.transaction()?;
    for handle in &handles {
        let preferred = handle.clone();
        let hint = best_name_hint_for_handle(&tx, account_id, handle)?;
        let preferred_name = hint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        tx.execute(
            r#"
            INSERT INTO contacts (
                account_id, preferred_name, preferred_handle
            ) VALUES (?1, ?2, ?3)
            "#,
            params![account_id, preferred_name, preferred],
        )?;
        let contact_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?1, ?2, ?3)",
            params![account_id, handle, contact_id],
        )?;
        created += 1;
    }
    tx.commit()?;

    Ok(created)
}

/// Fill empty contact preferred names from participant name hints (exporter display names).
///
/// Does not overwrite names the user (or contacts CSV) already set.
pub fn fill_empty_contact_names_from_participants(
    conn: &mut Connection,
    account_id: &str,
) -> Result<u64> {
    crate::db::schema::ensure_contacts_schema(conn)?;

    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            r#"
            SELECT c.id, ch.handle
            FROM contacts c
            JOIN contact_handles ch
              ON ch.contact_id = c.id AND ch.account_id = c.account_id
            WHERE c.account_id = ?1
              AND (c.preferred_name IS NULL OR TRIM(c.preferred_name) = '')
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
        let preferred_name = hint.trim();
        if preferred_name.is_empty() {
            continue;
        }
        let n = tx.execute(
            r#"
            UPDATE contacts
            SET preferred_name = ?2
            WHERE id = ?1
              AND account_id = ?3
              AND (preferred_name IS NULL OR TRIM(preferred_name) = '')
            "#,
            params![contact_id, preferred_name, account_id],
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
    let hints = stmt.query_map(params![account_id, handle], |row| row.get::<_, String>(0))?;
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
    if t.starts_with('+')
        && t.chars()
            .all(|c| c.is_ascii_digit() || "+ ().-".contains(c))
    {
        return true;
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    let stripped: String = t
        .chars()
        .filter(|c| !c.is_whitespace() && !"()+-.".contains(*c))
        .collect();
    digits.len() >= 7 && digits.len() == stripped.len()
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
        crate::db::schema::ensure_vault_schema(&conn).unwrap();

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
    fn accepts_vcard_csv_and_vcf_but_rejects_vault_csv() {
        let dir = std::env::temp_dir().join(format!(
            "mv-contacts-fmt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let vcard_csv = dir.join("contacts.csv");
        std::fs::write(
            &vcard_csv,
            "First Name,Last Name,Mobile Phone\nAda,Lovelace,+15551234567\n",
        )
        .unwrap();
        assert_eq!(
            contacts_file_format(&vcard_csv).unwrap(),
            ContactsFormat::VcardCsv
        );

        let current_export =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/contacts/current-labels.csv");
        assert!(contacts_file_format(&current_export).is_err());

        let vcf = dir.join("book.vcf");
        std::fs::write(
            &vcf,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\nTEL:+15551234567\nEND:VCARD\n",
        )
        .unwrap();
        assert_eq!(contacts_file_format(&vcf).unwrap(), ContactsFormat::Vcf);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loads_vcard_csv_into_sqlite() {
        let dir = std::env::temp_dir().join(format!(
            "mv-contacts-vcard-csv-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("vault.db");
        let csv_path = dir.join("contacts.csv");
        std::fs::write(
            &csv_path,
            "First Name,Middle Name,Last Name,Mobile Phone,Home Phone\n\
             Ada,Augusta,Lovelace,+15551234567,+15559876543\n\
             NoPhone,,,+\n",
        )
        .unwrap();

        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::schema::ensure_vault_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES (?1, 't', 0, 'T')",
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();

        let stats =
            load_contacts_if_needed(&mut conn, Some(&csv_path), true, TEST_ACCOUNT_ID).unwrap();
        assert_eq!(stats.contacts, 1);
        assert_eq!(stats.phones, 2);

        let name: String = conn
            .query_row(
                "SELECT preferred_name FROM contacts WHERE account_id = ?1",
                params![TEST_ACCOUNT_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Ada Augusta Lovelace");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loads_vcf_into_sqlite() {
        let dir = std::env::temp_dir().join(format!(
            "mv-contacts-vcf-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("vault.db");
        let vcf_path = dir.join("contacts.vcf");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/contacts/contact-contract.vcf"),
            &vcf_path,
        )
        .unwrap();

        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::schema::ensure_vault_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES (?1, 't', 0, 'T')",
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();

        let stats =
            load_contacts_if_needed(&mut conn, Some(&vcf_path), true, TEST_ACCOUNT_ID).unwrap();
        assert_eq!(stats.contacts, 2);
        assert_eq!(stats.phones, 3);
        assert_eq!(stats.labels, 3);

        let preferred_name: String = conn
            .query_row(
                "SELECT preferred_name FROM contacts
                 WHERE account_id = ?1 AND preferred_handle = '+15551234567'",
                params![TEST_ACCOUNT_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preferred_name, "Ada Augusta Lovelace");

        let labels: Vec<String> = conn
            .prepare(
                "SELECT cl.name FROM contact_labels cl
                 JOIN contact_label_members m ON m.label_id = cl.id
                 WHERE cl.account_id = ?1 ORDER BY cl.name",
            )
            .unwrap()
            .query_map(params![TEST_ACCOUNT_ID], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            labels,
            vec![
                "Family".to_string(),
                "Friends".to_string(),
                "Work".to_string()
            ]
        );

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
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
        let mut conn = seed_group_vault(&db_path);
        let created = ensure_unknown_contacts(&mut conn, TEST_ACCOUNT_ID).unwrap();
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
        let again = ensure_unknown_contacts(&mut conn, TEST_ACCOUNT_ID).unwrap();
        assert_eq!(again, 0);
        assert!(!dir.join("contacts.csv").exists());

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
