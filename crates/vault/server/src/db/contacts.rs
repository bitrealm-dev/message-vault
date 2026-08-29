//! Address book loading (VCF or vCard CSV) and contact/group/handle links.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use contacts::{
    ContactsFormat, detect_contacts_format, extract_tags, parse_vcf, read_vcard_csv_rows,
    strip_tags,
};
use sqlx::{AnyConnection, Connection};

/// Bump `contacts.last_modified` after an address-book shape change.
pub async fn touch_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
) -> Result<()> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query("UPDATE contacts SET last_modified = $1 WHERE id = $2 AND account_id = $3")
        .bind(now)
        .bind(contact_id)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Contact linked to a handle via `contact_handles`, if any.
pub async fn contact_id_for_handle(
    conn: &mut AnyConnection,
    account_id: &str,
    handle_id: i64,
) -> Result<Option<i64>> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT contact_id FROM contact_handles WHERE account_id = $1 AND handle_id = $2",
    )
    .bind(account_id)
    .bind(handle_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(found)
}

/// Counts from loading an address book into the vault.
#[derive(Debug, Default)]
pub struct ContactLoadStats {
    /// Contacts written.
    pub contacts: u64,
    /// Phone handles linked to contacts.
    pub phones: u64,
    /// Contact–group links created.
    pub groups: u64,
    /// Emails restored into `account_emails`.
    pub emails_restored: u64,
    /// True when loading was skipped (contacts already loaded and not forced).
    pub skipped: bool,
    /// Phone handles written with a review note (ambiguous normalized form).
    pub phones_needing_review: u64,
}

#[derive(Debug)]
struct ContactDraft {
    /// (normalized handle, review note when the value is ambiguous).
    phones: Vec<(String, Option<String>)>,
    preferred_name: Option<String>,
    groups: Vec<String>,
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

/// Raw phone → (normalized, review note) under the guarded policy: E.164 when
/// the raw is unambiguous (`+`-prefixed, or a US national number), else
/// digits-as-is plus a reason — never a fabricated `+0…` value.
fn normalize_phone_guarded(num: &str) -> Option<(String, Option<String>)> {
    let trimmed = num.trim();
    if trimmed.is_empty() || trimmed.contains('@') {
        return None;
    }
    // No usable digits (e.g. a bare `+`): not a phone at all.
    phone::sanitize_number(trimmed)?;
    let guarded = phone::normalize_guarded(trimmed, phone::PhoneRegion::for_raw(trimmed));
    Some((guarded.normalized, guarded.note))
}

/// Phone handles from an address-book row as (normalized, review note) pairs;
/// emails are dropped.
fn phone_handles_only(handles: &[String]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for h in handles {
        if is_email_handle(h) {
            continue;
        }
        let Some((normalized, note)) = normalize_phone_guarded(h) else {
            continue;
        };
        if !out.iter().any(|(p, _)| p == &normalized) {
            out.push((normalized, note));
        }
    }
    out
}

/// Phones on a contact, plus email `(handle_id, raw)` pairs for restore.
type ContactPhonesAndEmails = (HashSet<String>, Vec<(i64, String)>);

/// Emails attached to a contact, keyed for restore by that contact's phone set.
#[derive(Debug, Default)]
struct EmailSnapshot {
    /// One entry per contact that had emails: (phones on that contact, emails).
    /// Emails are (handle_id, raw) so restore can re-link the `handles` row.
    entries: Vec<ContactPhonesAndEmails>,
}

async fn snapshot_email_handles(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<EmailSnapshot> {
    let mut by_contact: HashMap<i64, ContactPhonesAndEmails> = HashMap::new();

    let rows: Vec<(i64, i64, String, String, String)> = sqlx::query_as(
        "SELECT ch.contact_id, h.id, h.raw, h.normalized, h.handle_type
         FROM contact_handles ch
         JOIN handles h ON h.id = ch.handle_id
         WHERE ch.account_id = $1
         ORDER BY ch.contact_id, h.handle_type, h.raw",
    )
    .bind(account_id)
    .fetch_all(&mut *conn)
    .await?;
    for (contact_id, handle_id, raw, normalized, handle_type) in rows {
        let entry = by_contact.entry(contact_id).or_default();
        if handle_type == "email" {
            entry.1.push((handle_id, raw));
        } else {
            entry.0.insert(normalized);
        }
    }
    Ok(EmailSnapshot {
        entries: by_contact
            .into_values()
            .filter(|(_, emails)| !emails.is_empty())
            .collect(),
    })
}

async fn restore_email_handles(
    conn: &mut AnyConnection,
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
            let found: Option<i64> = sqlx::query_scalar(
                "SELECT ch.contact_id
                 FROM contact_handles ch
                 JOIN handles h ON h.id = ch.handle_id
                 WHERE ch.account_id = $1 AND h.handle_type = 'phone' AND h.normalized = $2",
            )
            .bind(account_id)
            .bind(phone)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some(id) = found {
                contact_id = Some(id);
                break;
            }
        }
        let Some(id) = contact_id else {
            continue;
        };
        for (handle_id, email) in emails {
            let owner = contact_id_for_handle(conn, account_id, *handle_id).await?;
            if let Some(existing) = owner {
                if existing != id {
                    eprintln!(
                        "warning: email handle {email} already belongs to contact {existing}; not restoring onto {id}"
                    );
                }
                continue;
            }
            sqlx::query(
                "INSERT INTO contact_handles (account_id, handle_id, contact_id) VALUES ($1, $2, $3)",
            )
            .bind(account_id)
            .bind(handle_id)
            .bind(id)
            .execute(&mut *conn)
            .await?;
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
pub async fn load_contacts_if_needed(
    conn: &mut AnyConnection,
    contacts_path: Option<&Path>,
    overwrite: bool,
    account_id: &str,
) -> Result<ContactLoadStats> {
    crate::db::schema::ensure_vault_schema(conn).await?;
    crate::db::account_profile::ensure_account_row(conn, account_id).await?;

    let Some(path) = contacts_path else {
        return Ok(ContactLoadStats {
            skipped: true,
            ..Default::default()
        });
    };

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts WHERE account_id = $1")
        .bind(account_id)
        .fetch_one(&mut *conn)
        .await
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
            delete_account_contacts(conn, account_id).await?;
        }
        return Ok(ContactLoadStats::default());
    }

    let email_snapshot = if count > 0 && overwrite {
        snapshot_email_handles(conn, account_id).await?
    } else {
        EmailSnapshot::default()
    };

    delete_account_contacts(conn, account_id).await?;

    let format = contacts_file_format(path)?;
    let mut stats = match format {
        ContactsFormat::VcardCsv => load_from_vcard_csv(conn, path, account_id).await?,
        ContactsFormat::Vcf => load_from_vcf(conn, path, account_id).await?,
    };
    stats.emails_restored = restore_email_handles(conn, account_id, &email_snapshot).await?;
    if stats.emails_restored > 0 {
        eprintln!(
            "contacts: restored {} email handle(s) from previous DB (address book is phone-only)",
            stats.emails_restored
        );
    }
    Ok(stats)
}

async fn delete_account_contacts(conn: &mut AnyConnection, account_id: &str) -> Result<()> {
    sqlx::query(
        "DELETE FROM contact_group_members WHERE contact_id IN (SELECT id FROM contacts WHERE account_id = $1)",
    )
    .bind(account_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM contact_handles WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM contact_groups WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM contacts WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn load_from_vcard_csv(
    conn: &mut AnyConnection,
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
            groups: Vec::new(),
        });
    }

    insert_contact_drafts(conn, account_id, drafts).await
}

async fn load_from_vcf(
    conn: &mut AnyConnection,
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

        let mut groups = Vec::new();
        for tag in fn_tags {
            let t = tag.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("People") {
                continue;
            }
            groups.push(t.to_string());
        }
        for category in &card.categories {
            let t = category.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("People") {
                continue;
            }
            if !groups.iter().any(|g| g.eq_ignore_ascii_case(t)) {
                groups.push(t.to_string());
            }
        }

        drafts.push(ContactDraft {
            phones,
            preferred_name,
            groups,
        });
    }

    insert_contact_drafts(conn, account_id, drafts).await
}

fn collapse_inner_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn insert_contact_drafts(
    conn: &mut AnyConnection,
    account_id: &str,
    drafts: Vec<ContactDraft>,
) -> Result<ContactLoadStats> {
    let mut stats = ContactLoadStats::default();
    let drafts = merge_duplicate_phone_drafts(drafts);
    let mut tx = conn.begin().await?;

    for draft in drafts {
        // Insert contact
        let preferred_name = draft.preferred_name.as_deref().unwrap_or("Unknown");
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(account_id)
        .bind(preferred_name)
        .fetch_one(&mut *tx)
        .await?;
        stats.contacts += 1;

        for (phone, note) in &draft.phones {
            // Ensure handle exists; the note flags ambiguous values for review.
            sqlx::query(
                "INSERT INTO handles (account_id, raw, normalized, normalized_note, handle_type, service)
                 VALUES ($1, $2, $3, $4, 'phone', 'phone')
                 ON CONFLICT DO NOTHING",
            )
            .bind(account_id)
            .bind(phone)
            .bind(phone)
            .bind(note.as_deref())
            .execute(&mut *tx)
            .await?;
            let handle_id: i64 = sqlx::query_scalar(
                "SELECT id FROM handles
                 WHERE account_id = $1 AND normalized = $2 AND handle_type = 'phone' AND service = 'phone'",
            )
            .bind(account_id)
            .bind(phone)
            .fetch_one(&mut *tx)
            .await?;

            // Link contact to handle
            sqlx::query(
                "INSERT INTO contact_handles (account_id, handle_id, contact_id)
                 VALUES ($1, $2, $3)
                 ON CONFLICT DO NOTHING",
            )
            .bind(account_id)
            .bind(handle_id)
            .bind(contact_id)
            .execute(&mut *tx)
            .await?;
            stats.phones += 1;
            if note.is_some() {
                stats.phones_needing_review += 1;
            }
        }

        for group_name in &draft.groups {
            let group_id = ensure_group(&mut tx, account_id, group_name).await?;
            sqlx::query(
                "INSERT INTO contact_group_members (contact_id, group_id) VALUES ($1, $2)
                 ON CONFLICT DO NOTHING",
            )
            .bind(contact_id)
            .bind(group_id)
            .execute(&mut *tx)
            .await?;
            stats.groups += 1;
        }
    }

    tx.commit().await?;
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
    for group in from.groups {
        if !into
            .groups
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&group))
        {
            into.groups.push(group);
        }
    }
}

async fn ensure_group(conn: &mut AnyConnection, account_id: &str, name: &str) -> Result<i64> {
    sqlx::query(
        "INSERT INTO contact_groups (account_id, name) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(name)
    .execute(&mut *conn)
    .await?;
    let id: i64 =
        sqlx::query_scalar("SELECT id FROM contact_groups WHERE account_id = $1 AND name = $2")
            .bind(account_id)
            .bind(name)
            .fetch_one(&mut *conn)
            .await?;
    Ok(id)
}

/// Contacts are now resolved through the `handles` table during import (Task 10 of the
/// handle-identity-model plan); backfilling unknown contacts from conversation data and
/// filling empty names from participant hints happen there, not here.
#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ACCOUNT_ID: &str = "00000000-0000-0000-0000-000000000042";

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
            vec![
                ("+15551234567".to_string(), None),
                ("+15559876543".to_string(), None)
            ]
        );
    }

    #[tokio::test]
    async fn trunk_zero_phone_is_flagged_with_note() {
        let (pool, dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        crate::db::schema::ensure_vault_schema(&mut conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, username, preferred_name)
             VALUES ($1, 't', 'T')",
        )
        .bind(TEST_ACCOUNT_ID)
        .execute(&mut *conn)
        .await
        .unwrap();
        let vcf_path = dir.path().join("contacts.vcf");
        std::fs::write(
            &vcf_path,
            "BEGIN:VCARD\nVERSION:3.0\nFN:UK Peer\nN:Peer;UK;;;\nTEL:020 7946 0000\nEND:VCARD\n",
        )
        .unwrap();

        let stats = load_contacts_if_needed(&mut conn, Some(&vcf_path), true, TEST_ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(stats.phones, 1);
        assert_eq!(stats.phones_needing_review, 1);

        // Guarded policy: normalized mirrors the digits (no fabricated
        // +02079460000) and the handles row carries a review note.
        let (normalized, note): (String, Option<String>) = sqlx::query_as(
            "SELECT normalized, normalized_note FROM handles
             WHERE account_id = $1 AND handle_type = 'phone'",
        )
        .bind(TEST_ACCOUNT_ID)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(normalized, "02079460000");
        assert!(
            note.as_deref().is_some(),
            "trunk-zero phone must carry a review note"
        );
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

        // Vault's own export CSV (phones/first_name/last_name) is not an address book.
        let vault_export = dir.join("vault-export.csv");
        std::fs::write(
            &vault_export,
            "phones,first_name,last_name,label_1\n+15551234567,Ada,Lovelace,Family\n",
        )
        .unwrap();
        assert!(contacts_file_format(&vault_export).is_err());

        let vcf = dir.join("book.vcf");
        std::fs::write(
            &vcf,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\nTEL:+15551234567\nEND:VCARD\n",
        )
        .unwrap();
        assert_eq!(contacts_file_format(&vcf).unwrap(), ContactsFormat::Vcf);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn loads_vcard_csv_into_sqlite() {
        let (pool, dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        crate::db::schema::ensure_vault_schema(&mut conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, username, preferred_name)
             VALUES ($1, 't', 'T')",
        )
        .bind(TEST_ACCOUNT_ID)
        .execute(&mut *conn)
        .await
        .unwrap();
        let csv_path = dir.path().join("contacts.csv");
        std::fs::write(
            &csv_path,
            "First Name,Middle Name,Last Name,Mobile Phone,Home Phone\n\
             Ada,Augusta,Lovelace,+15551234567,+15559876543\n\
             NoPhone,,,+\n",
        )
        .unwrap();

        let stats = load_contacts_if_needed(&mut conn, Some(&csv_path), true, TEST_ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(stats.contacts, 1);
        assert_eq!(stats.phones, 2);

        let name: String =
            sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE account_id = $1")
                .bind(TEST_ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(name, "Ada Augusta Lovelace");
    }

    #[tokio::test]
    async fn loads_vcf_into_sqlite() {
        let (pool, dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        crate::db::schema::ensure_vault_schema(&mut conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, username, preferred_name)
             VALUES ($1, 't', 'T')",
        )
        .bind(TEST_ACCOUNT_ID)
        .execute(&mut *conn)
        .await
        .unwrap();
        let vcf_path = dir.path().join("contacts.vcf");
        std::fs::write(
            &vcf_path,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Augusta Lovelace\nN:Lovelace;Ada;Augusta;;\nTEL:+15551234567\nCATEGORIES:Family\nEND:VCARD\n\
             BEGIN:VCARD\nVERSION:3.0\nFN:Ada Duplicate\nN:Duplicate;Ada;;;\nTEL:+15551234567\nTEL:+15559876543\nCATEGORIES:Work\nEND:VCARD\n\
             BEGIN:VCARD\nVERSION:3.0\nFN:Mononym\nN:;Mononym;;;\nTEL:+15557654321\nCATEGORIES:Friends\nEND:VCARD\n",
        )
        .unwrap();

        let stats = load_contacts_if_needed(&mut conn, Some(&vcf_path), true, TEST_ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(stats.contacts, 2);
        assert_eq!(stats.phones, 3);
        assert_eq!(stats.groups, 3);

        let preferred_name: String = sqlx::query_scalar(
            "SELECT c.preferred_name FROM contacts c
             JOIN contact_handles ch ON ch.contact_id = c.id
             JOIN handles h ON h.id = ch.handle_id
             WHERE c.account_id = $1 AND h.normalized = '+15551234567'",
        )
        .bind(TEST_ACCOUNT_ID)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(preferred_name, "Ada Augusta Lovelace");

        let groups: Vec<String> = sqlx::query_scalar(
            "SELECT cg.name FROM contact_groups cg
             JOIN contact_group_members m ON m.group_id = cg.id
             WHERE cg.account_id = $1 ORDER BY cg.name",
        )
        .bind(TEST_ACCOUNT_ID)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            groups,
            vec![
                "Family".to_string(),
                "Friends".to_string(),
                "Work".to_string()
            ]
        );
    }
}
