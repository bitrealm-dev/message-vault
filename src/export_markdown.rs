//! Obsidian bubble markdown export (1:1 people only).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, params, params_from_iter};

use crate::schema;
use crate::vault_owner::{self, VaultOwner};

#[derive(Debug, Default)]
pub struct ExportStats {
    pub people: u64,
    pub year_pages: u64,
    pub messages: u64,
    pub assets_copied: u64,
    pub assets_missing: u64,
}

/// Export combined (deduped) 1:1 threads as Obsidian bubble markdown under `out_dir`.
///
/// `assets_by_source` maps `messages.source` → on-disk originals root
/// (typically `data/<source>/assets`).
pub fn export_markdown(
    conn: &Connection,
    owner: &VaultOwner,
    account_id: &str,
    assets_by_source: &HashMap<String, PathBuf>,
    out_dir: &Path,
    snippet_css: &Path,
) -> Result<ExportStats> {
    let mut stats = ExportStats::default();

    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    let people_dir = out_dir.join("People");
    fs::create_dir_all(&people_dir)?;
    let assets_out = out_dir.join("_assets");
    fs::create_dir_all(&assets_out)?;

    install_snippet(out_dir, snippet_css)?;

    let contacts = list_export_contacts(conn, account_id)?;
    let mut person_index: Vec<(String, String)> = Vec::new(); // (folder, display)

    let total = contacts.len();
    for (idx, contact) in contacts.iter().enumerate() {
        let years = contact_yearly_threads(conn, account_id, &contact.phones)?;
        if years.is_empty() {
            continue;
        }

        let folder_name = sanitize_name(&contact.display_name);
        let person_dir = people_dir.join(&folder_name);
        fs::create_dir_all(&person_dir)?;

        let mut year_links: Vec<(i64, u64)> = Vec::new();
        for yt in &years {
            let page = render_year_page(
                conn,
                owner,
                account_id,
                contact,
                yt,
                assets_by_source,
                &assets_out,
                &person_dir,
                &mut stats,
            )?;
            let path = person_dir.join(format!("{}.md", yt.year));
            fs::write(&path, page)
                .with_context(|| format!("failed to write {}", path.display()))?;
            stats.year_pages += 1;
            year_links.push((yt.year, yt.message_count));
        }

        let hub = render_person_hub(&contact.display_name, &contact.phones, &year_links);
        let hub_path = person_dir.join(format!("_{folder_name}.md"));
        fs::write(&hub_path, hub)
            .with_context(|| format!("failed to write {}", hub_path.display()))?;

        person_index.push((folder_name, contact.display_name.clone()));
        stats.people += 1;

        let n = idx + 1;
        if n == 1 || n == total || n % 25 == 0 {
            println!(
                "  export:   [{n}/{total}] {}  years={}",
                contact.display_name,
                years.len()
            );
            let _ = io::stdout().flush();
        }
    }

    person_index.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

    fs::write(
        people_dir.join("index.md"),
        render_people_index(&person_index),
    )?;
    fs::write(
        out_dir.join("index.md"),
        render_root_index(person_index.len()),
    )?;

    Ok(stats)
}

pub fn run_export(
    db_path: &Path,
    account_id: &str,
    assets_by_source: &HashMap<String, PathBuf>,
    out_dir: &Path,
    snippet_css: &Path,
) -> Result<ExportStats> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    schema::ensure_vault_schema(&conn)?;
    schema::ensure_messages_schema(&conn)?;
    schema::ensure_contacts_schema(&conn)?;
    let owner = vault_owner::load_vault_owner(&conn, account_id)?;
    export_markdown(
        &conn,
        &owner,
        account_id,
        assets_by_source,
        out_dir,
        snippet_css,
    )
}

fn install_snippet(out_dir: &Path, snippet_css: &Path) -> Result<()> {
    let dest_dir = out_dir.join(".obsidian").join("snippets");
    fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join("message-vault-bubbles.css");
    fs::copy(snippet_css, &dest).with_context(|| {
        format!(
            "failed to copy snippet {} → {}",
            snippet_css.display(),
            dest.display()
        )
    })?;
    Ok(())
}

#[derive(Debug, Clone)]
struct ExportContact {
    #[allow(dead_code)]
    id: i64,
    display_name: String,
    phones: Vec<String>,
}

#[derive(Debug, Clone)]
struct YearThread {
    year: i64,
    message_count: u64,
    date_start: String,
    date_end: String,
    conversation_ids: Vec<i64>,
}

#[derive(Debug, Clone)]
struct ExportMessage {
    #[allow(dead_code)]
    id: i64,
    source: String,
    timestamp: String,
    is_from_me: bool,
    sender_name: String,
    body: Option<String>,
    attachments: Vec<ExportAttachment>,
    tapbacks: Vec<ExportTapback>,
}

#[derive(Debug, Clone)]
struct ExportAttachment {
    mime_type: Option<String>,
    original_name: Option<String>,
    assets_path: Option<String>,
    sha256: Option<String>,
}

#[derive(Debug, Clone)]
struct ExportTapback {
    kind: String,
    emoji: Option<String>,
    is_from_me: bool,
    sender: Option<String>,
}

fn list_export_contacts(conn: &Connection, account_id: &str) -> Result<Vec<ExportContact>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, first_name, last_name, preferred_handle
        FROM contacts
        WHERE account_id = ?1
        ORDER BY
          LOWER(COALESCE(NULLIF(TRIM(last_name), ''), first_name, preferred_handle, '')),
          LOWER(COALESCE(NULLIF(TRIM(first_name), ''), preferred_handle, ''))
        "#,
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;

    let mut out = Vec::new();
    let mut phone_stmt =
        conn.prepare("SELECT handle FROM contact_handles WHERE contact_id = ?1")?;
    for row in rows {
        let (id, first, last, preferred) = row?;
        let phones: Vec<String> = phone_stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if phones.is_empty() {
            continue;
        }
        let display_name = display_name(
            first.as_deref(),
            last.as_deref(),
            preferred.as_deref().or(phones.first().map(|s| s.as_str())),
        );
        out.push(ExportContact {
            id,
            display_name,
            phones,
        });
    }
    Ok(out)
}

fn display_name(first: Option<&str>, last: Option<&str>, fallback: Option<&str>) -> String {
    let parts: Vec<&str> = [first, last]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if !parts.is_empty() {
        return parts.join(" ");
    }
    fallback.unwrap_or("Unknown").to_string()
}

fn contact_yearly_threads(
    conn: &Connection,
    account_id: &str,
    phones: &[String],
) -> Result<Vec<YearThread>> {
    if phones.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat_n("?", phones.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
        SELECT CAST(substr(m.timestamp, 1, 4) AS INTEGER) AS year,
               COUNT(*) AS message_count,
               MIN(substr(m.timestamp, 1, 10)) AS date_start,
               MAX(substr(m.timestamp, 1, 10)) AS date_end,
               GROUP_CONCAT(DISTINCT c.id) AS conversation_ids
        FROM conversations c
        JOIN messages m ON m.conversation_id = c.id
        WHERE c.account_id = ?
          AND c.conversation_type = 'individual'
          AND c.chat_identifier IN ({placeholders})
          AND m.duplicate_of IS NULL
        GROUP BY year
        ORDER BY year ASC
        "#
    );
    let mut params: Vec<rusqlite::types::Value> =
        vec![rusqlite::types::Value::Text(account_id.to_string())];
    params.extend(
        phones
            .iter()
            .map(|p| rusqlite::types::Value::Text(p.clone())),
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (year, count, date_start, date_end, ids) = row?;
        if year <= 0 {
            continue;
        }
        let conversation_ids: Vec<i64> = ids
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        out.push(YearThread {
            year,
            message_count: count as u64,
            date_start,
            date_end,
            conversation_ids,
        });
    }
    Ok(out)
}

fn load_year_messages(
    conn: &Connection,
    owner: &VaultOwner,
    account_id: &str,
    contact: &ExportContact,
    yt: &YearThread,
) -> Result<Vec<ExportMessage>> {
    if yt.conversation_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat_n("?", yt.conversation_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
        SELECT m.id, m.source, m.timestamp, m.is_from_me, m.sender, m.body,
               c.first_name, c.last_name, c.preferred_handle, p.name_hint
        FROM messages m
        LEFT JOIN contact_handles cp ON cp.account_id = ? AND cp.handle = m.sender
        LEFT JOIN contacts c ON c.id = cp.contact_id AND c.account_id = ?
        LEFT JOIN participants p
          ON p.conversation_id = m.conversation_id AND p.handle = m.sender
        WHERE m.conversation_id IN ({placeholders})
          AND CAST(substr(m.timestamp, 1, 4) AS INTEGER) = ?
          AND m.duplicate_of IS NULL
        ORDER BY m.timestamp, m.sort_order
        "#
    );
    let mut params: Vec<rusqlite::types::Value> = vec![
        rusqlite::types::Value::Text(account_id.to_string()),
        rusqlite::types::Value::Text(account_id.to_string()),
    ];
    params.extend(
        yt.conversation_ids
            .iter()
            .map(|id| rusqlite::types::Value::Integer(*id)),
    );
    params.push(rusqlite::types::Value::Integer(yt.year));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
        ))
    })?;

    let mut att_stmt = conn.prepare(
        r#"
        SELECT mime_type, original_name, assets_path, sha256
        FROM attachments WHERE message_id = ?1
        "#,
    )?;
    let mut tap_stmt = conn.prepare(
        r#"
        SELECT kind, emoji, is_from_me, sender
        FROM tapbacks WHERE message_id = ?1
        ORDER BY id
        "#,
    )?;

    let mut out = Vec::new();
    for row in rows {
        let (
            id,
            source,
            timestamp,
            is_from_me,
            sender,
            body,
            first_name,
            last_name,
            preferred_handle,
            name_hint,
        ) = row?;
        let from_me = is_from_me != 0;
        let sender_name = if from_me {
            owner.display_name.clone()
        } else {
            let mut name = display_name(
                first_name.as_deref(),
                last_name.as_deref(),
                preferred_handle
                    .as_deref()
                    .or(sender.as_deref())
                    .or(Some(contact.display_name.as_str())),
            );
            if name == preferred_handle.as_deref().unwrap_or("")
                || name == sender.as_deref().unwrap_or("")
            {
                if let Some(hint) = name_hint
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    if !looks_like_phone(hint) {
                        name = hint.to_string();
                    }
                }
            }
            // Prefer the contact display name for the peer in 1:1 exports.
            if name != owner.display_name {
                name = contact.display_name.clone();
            }
            name
        };

        let attachments: Vec<ExportAttachment> = att_stmt
            .query_map(params![id], |r| {
                Ok(ExportAttachment {
                    mime_type: r.get(0)?,
                    original_name: r.get(1)?,
                    assets_path: r.get(2)?,
                    sha256: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let tapbacks: Vec<ExportTapback> = tap_stmt
            .query_map(params![id], |r| {
                Ok(ExportTapback {
                    kind: r.get(0)?,
                    emoji: r.get(1)?,
                    is_from_me: r.get::<_, i64>(2)? != 0,
                    sender: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        out.push(ExportMessage {
            id,
            source,
            timestamp,
            is_from_me: from_me,
            sender_name,
            body,
            attachments,
            tapbacks,
        });
    }
    Ok(out)
}

fn looks_like_phone(value: &str) -> bool {
    let t = value.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with('+')
        && t.chars()
            .all(|c| c.is_ascii_digit() || "+ ()-.".contains(c))
    {
        return true;
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.len() >= 7 && digits.len() == t.chars().filter(|c| !c.is_whitespace()).count()
}

fn render_year_page(
    conn: &Connection,
    owner: &VaultOwner,
    account_id: &str,
    contact: &ExportContact,
    yt: &YearThread,
    assets_by_source: &HashMap<String, PathBuf>,
    assets_out: &Path,
    person_dir: &Path,
    stats: &mut ExportStats,
) -> Result<String> {
    let messages = load_year_messages(conn, owner, account_id, contact, yt)?;
    stats.messages += messages.len() as u64;

    let sources: BTreeSet<&str> = messages.iter().map(|m| m.source.as_str()).collect();
    let sources_yaml = sources
        .iter()
        .map(|s| format!("\"{}\"", escape_yaml(s)))
        .collect::<Vec<_>>()
        .join(", ");

    let numbers_yaml = contact
        .phones
        .iter()
        .map(|p| format!("\"{}\"", escape_yaml(p)))
        .collect::<Vec<_>>()
        .join(", ");

    let title = format!("{} ({})", contact.display_name, yt.year);
    let mut lines: Vec<String> = Vec::new();
    lines.push("---".into());
    lines.push(format!("title: \"{}\"", escape_yaml(&title)));
    lines.push("cssclasses:".into());
    lines.push("  - \"imessage-archive\"".into());
    lines.push(format!("sources: [{sources_yaml}]"));
    lines.push("type: individual".into());
    lines.push(format!("year: {}", yt.year));
    lines.push(format!(
        "participants: [\"{}\"]",
        escape_yaml(&contact.display_name)
    ));
    lines.push(format!("numbers: [{numbers_yaml}]"));
    lines.push(format!("date_range: {} — {}", yt.date_start, yt.date_end));
    lines.push("---".into());
    lines.push(String::new());
    lines.push(format!(
        "# {} | {} - {} · {} messages",
        contact.display_name,
        yt.date_start,
        yt.date_end,
        messages.len()
    ));
    lines.push(String::new());
    let meta = contact
        .phones
        .iter()
        .map(|p| format!("{}, {}", p, contact.display_name))
        .collect::<Vec<_>>()
        .join("; ");
    lines.push(format!(
        "<p class=\"archive-meta\">{}</p>",
        html_escape(&meta)
    ));
    lines.push(String::new());
    lines.push("<div class=\"imessage-archive\">".into());
    lines.push("<div class=\"messages\">".into());

    let mut last_month: Option<u32> = None;
    let mut copied: HashSet<String> = HashSet::new();
    for msg in &messages {
        if let Some(month) = parse_month(&msg.timestamp) {
            if last_month != Some(month) {
                lines.push(format!(
                    "<div class=\"month-label\">{}</div>",
                    month_name(month)
                ));
                last_month = Some(month);
            }
        }
        lines.push(render_message_html(
            msg,
            owner,
            assets_by_source,
            assets_out,
            person_dir,
            &mut copied,
            stats,
        )?);
    }

    lines.push("</div>".into());
    lines.push("</div>".into());
    lines.push(String::new());
    Ok(lines.join("\n"))
}

fn render_message_html(
    msg: &ExportMessage,
    owner: &VaultOwner,
    assets_by_source: &HashMap<String, PathBuf>,
    assets_out: &Path,
    person_dir: &Path,
    copied: &mut HashSet<String>,
    stats: &mut ExportStats,
) -> Result<String> {
    let css_class = if msg.is_from_me { "sent" } else { "received" };
    let ts = format_timestamp_display(&msg.timestamp);
    let mut inner = String::new();
    inner.push_str(&format!(
        "<div class=\"meta-line\">{} · {}</div>",
        html_escape(&ts),
        html_escape(&msg.sender_name)
    ));

    if let Some(body) = msg.body.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let html_body = html_escape(body).replace('\n', "<br>");
        inner.push_str(&format!("<div class=\"bubble\">{html_body}</div>"));
    }

    for att in &msg.attachments {
        inner.push_str(&render_attachment_html(
            att,
            &msg.source,
            assets_by_source,
            assets_out,
            person_dir,
            copied,
            stats,
        )?);
    }

    if !msg.tapbacks.is_empty() {
        inner.push_str("<div class=\"tapbacks\">");
        for tap in &msg.tapbacks {
            let who = if tap.is_from_me {
                owner.display_name.as_str()
            } else {
                tap.sender
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Someone")
            };
            let label = tapback_label(&tap.kind, tap.emoji.as_deref());
            inner.push_str(&format!(
                "<div class=\"tapback\"><b>{}</b> by {}</div>",
                html_escape(&label),
                html_escape(who)
            ));
        }
        inner.push_str("</div>");
    }

    Ok(format!("<div class=\"message {css_class}\">{inner}</div>"))
}

fn render_attachment_html(
    att: &ExportAttachment,
    source: &str,
    assets_by_source: &HashMap<String, PathBuf>,
    assets_out: &Path,
    person_dir: &Path,
    copied: &mut HashSet<String>,
    stats: &mut ExportStats,
) -> Result<String> {
    let Some(rel) = att.assets_path.as_deref().filter(|s| !s.is_empty()) else {
        let name = att.original_name.as_deref().unwrap_or("attachment");
        return Ok(format!(
            "<p class=\"attachment-link\">{}</p>",
            html_escape(name)
        ));
    };

    let Some(root) = assets_by_source.get(source) else {
        stats.assets_missing += 1;
        let name = att.original_name.as_deref().unwrap_or(rel);
        return Ok(format!(
            "<p class=\"attachment-link\">{}</p>",
            html_escape(name)
        ));
    };

    let src_path = root.join(rel);
    if !src_path.is_file() {
        stats.assets_missing += 1;
        let name = att.original_name.as_deref().unwrap_or(rel);
        return Ok(format!(
            "<p class=\"attachment-link\">{}</p>",
            html_escape(name)
        ));
    }

    let dest_rel = normalize_asset_rel(rel, att.sha256.as_deref());
    if copied.insert(dest_rel.clone()) {
        let dest = assets_out.join(&dest_rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if !dest.is_file() {
            fs::copy(&src_path, &dest).with_context(|| {
                format!(
                    "failed to copy attachment {} → {}",
                    src_path.display(),
                    dest.display()
                )
            })?;
            stats.assets_copied += 1;
        }
    }

    let href = pathdiff_to_assets(person_dir, &dest_rel);
    let name = att.original_name.as_deref().unwrap_or("attachment");
    let mime = att.mime_type.as_deref().unwrap_or("");
    if mime.starts_with("image/") {
        Ok(format!(
            "<img class=\"attachment\" src=\"{}\" alt=\"{}\" loading=\"lazy\" decoding=\"async\">",
            html_escape(&href),
            html_escape(name)
        ))
    } else if mime.starts_with("video/") {
        Ok(format!(
            "<video class=\"attachment\" src=\"{}\" controls preload=\"metadata\"></video>",
            html_escape(&href)
        ))
    } else {
        Ok(format!(
            "<p class=\"attachment-link\"><a href=\"{}\">{}</a></p>",
            html_escape(&href),
            html_escape(name)
        ))
    }
}

fn normalize_asset_rel(rel: &str, sha: Option<&str>) -> String {
    // Prefer content-addressed layout: <sha[:2]>/<filename>
    if let Some(sha) = sha.filter(|s| s.len() >= 2) {
        let file = Path::new(rel)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(rel);
        return format!("{}/{}", &sha[..2], file);
    }
    rel.replace('\\', "/")
}

fn pathdiff_to_assets(person_dir: &Path, dest_rel: &str) -> String {
    // person_dir is <out>/People/<Name>, assets at <out>/_assets/<dest_rel>
    let _ = person_dir;
    format!("../../_assets/{}", dest_rel.replace('\\', "/"))
}

fn tapback_label(kind: &str, emoji: Option<&str>) -> String {
    match kind {
        "loved" => "Loved".into(),
        "liked" => "Liked".into(),
        "disliked" => "Disliked".into(),
        "laughed" => "Laughed".into(),
        "emphasized" => "Emphasized".into(),
        "questioned" => "Questioned".into(),
        "emoji" => emoji.unwrap_or("Reacted").to_string(),
        "sticker" => "Sticker".into(),
        other => {
            let mut s = other.to_string();
            if let Some(c) = s.get_mut(0..1) {
                c.make_ascii_uppercase();
            }
            s
        }
    }
}

fn render_person_hub(display_name: &str, phones: &[String], years: &[(i64, u64)]) -> String {
    let numbers = phones.join(", ");
    let mut lines = Vec::new();
    lines.push("---".into());
    lines.push(format!("title: \"{}\"", escape_yaml(display_name)));
    lines.push("cssclasses:".into());
    lines.push("  - \"message-vault-person\"".into());
    lines.push("---".into());
    lines.push(String::new());
    lines.push(format!("# {display_name}"));
    lines.push(String::new());
    if !numbers.is_empty() {
        lines.push(format!("**Numbers:** {numbers}"));
        lines.push(String::new());
    }
    lines.push("## Yearly Messages".into());
    lines.push(String::new());
    for (year, count) in years.iter().rev() {
        lines.push(format!("- [[{year}]] — {count} messages"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_people_index(people: &[(String, String)]) -> String {
    let mut lines = vec![
        "---".into(),
        "title: \"People\"".into(),
        "---".into(),
        String::new(),
        "# People".into(),
        String::new(),
    ];
    for (folder, display) in people {
        lines.push(format!("- [[{folder}/_{folder}|{display}]]"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_root_index(people_count: usize) -> String {
    format!(
        r#"---
title: "Message Vault"
---

# Message Vault

Exported 1:1 conversations (combined / soft-deduped view).

- [[People/index|People]] ({people_count})

Enable the CSS snippet **message-vault-bubbles** under Settings → Appearance → CSS snippets.
"#
    )
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "Unknown".into()
    } else {
        trimmed.to_string()
    }
}

fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn parse_month(ts: &str) -> Option<u32> {
    // YYYY-MM-...
    if ts.len() < 7 {
        return None;
    }
    ts.get(5..7)?.parse().ok()
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

fn format_timestamp_display(ts: &str) -> String {
    // Keep date + time when present; strip timezone suffix for readability.
    let base = ts.trim();
    if base.len() >= 19 {
        let date = &base[..10];
        let hour: i32 = base.get(11..13).and_then(|s| s.parse().ok()).unwrap_or(0);
        let minute = base.get(14..16).unwrap_or("00");
        let (h12, ampm) = match hour {
            0 => (12, "AM"),
            1..=11 => (hour, "AM"),
            12 => (12, "PM"),
            _ => (hour - 12, "PM"),
        };
        return format!("{date} {h12}:{minute} {ampm}");
    }
    if base.len() >= 10 {
        return base[..10].to_string();
    }
    base.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mv-export-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const TEST_ACCOUNT_ID: &str = "00000000-0000-0000-0000-000000000001";

    #[test]
    fn export_person_year_hides_dupes_and_copies_assets() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_messages_schema(&conn).unwrap();
        schema::ensure_contacts_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'test', 0)",
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO vault_owners (account_id, first_name, last_name, display_name) VALUES (?1, 'Matt', 'Beisser', 'Matt Beisser')",
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();

        conn.execute(
            r#"
            INSERT INTO contacts (account_id, first_name, last_name, exclude, preferred_handle)
            VALUES (?1, 'Zach', 'Henson', 0, '+18285532527')
            "#,
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();
        let contact_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?1, ?2, ?3)",
            params![TEST_ACCOUNT_ID, "+18285532527", contact_id],
        )
        .unwrap();
        conn.execute(
            r#"
            INSERT INTO conversations (
                account_id, chat_identifier, service, conversation_type, group_title, exported_at, source_file
            )
            VALUES (?1, '+18285532527', 'SMS', 'individual', NULL, NULL, 't.json')
            "#,
            params![TEST_ACCOUNT_ID],
        )
        .unwrap();
        let conv_id = conn.last_insert_rowid();
        conn.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender, body, sort_order
            ) VALUES (?1, ?2, 'imessage', 'g1', '2023-06-01T10:00:00-04:00', '2023-06-01T14:00:00Z', 1,
                      NULL, 'Hello 2023', 0)
            "#,
            params![conv_id, TEST_ACCOUNT_ID],
        )
        .unwrap();
        conn.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender, body, sort_order
            ) VALUES (?1, ?2, 'imessage', 'g2', '2024-01-07T23:38:00-05:00', '2024-01-08T04:38:00Z', 1,
                      NULL, 'Meet you tomorrow?', 0)
            "#,
            params![conv_id, TEST_ACCOUNT_ID],
        )
        .unwrap();
        let kept = conn.last_insert_rowid();
        conn.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, timestamp_utc, is_from_me,
                sender, body, sort_order, duplicate_of
            ) VALUES (?1, ?2, 'go-sms-pro', 'g3', '2024-01-07T23:38:00-05:00', '2024-01-08T04:38:00Z', 1,
                      NULL, 'HIDDEN DUPE BODY', 1, ?3)
            "#,
            params![conv_id, TEST_ACCOUNT_ID, kept],
        )
        .unwrap();

        let tmp = tempfile_dir();
        let assets_root = tmp.join("imessage_assets");
        fs::create_dir_all(assets_root.join("ab")).unwrap();
        let sha = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let asset_rel = format!("ab/{sha}.txt");
        fs::write(assets_root.join(&asset_rel), b"pic").unwrap();
        conn.execute(
            r#"
            INSERT INTO attachments (message_id, path, original_name, mime_type, sha256, assets_path)
            VALUES (?1, 'x', 'note.txt', 'text/plain', ?2, ?3)
            "#,
            params![kept, sha, asset_rel],
        )
        .unwrap();

        let out = tmp.join("export");
        let snippet = tmp.join("snippet.css");
        fs::write(&snippet, "/* test */").unwrap();
        let owner = VaultOwner {
            first_name: "Matt".into(),
            last_name: "Beisser".into(),
            display_name: "Matt Beisser".into(),
            phones: vec!["+19412660605".into()],
            emails: vec![],
        };
        let mut assets = HashMap::new();
        assets.insert("imessage".into(), assets_root);

        let stats =
            export_markdown(&conn, &owner, TEST_ACCOUNT_ID, &assets, &out, &snippet).unwrap();
        assert_eq!(stats.people, 1);
        assert_eq!(stats.year_pages, 2);
        assert_eq!(stats.messages, 2); // soft-hidden excluded
        assert_eq!(stats.assets_copied, 1);

        let page_2024 = fs::read_to_string(out.join("People/Zach Henson/2024.md")).unwrap();
        assert!(page_2024.contains("imessage-archive"));
        assert!(page_2024.contains("class=\"bubble\">Meet you tomorrow?</div>"));
        assert!(!page_2024.contains("HIDDEN DUPE BODY"));
        assert!(page_2024.contains("../../_assets/ab/"));

        let hub = fs::read_to_string(out.join("People/Zach Henson/_Zach Henson.md")).unwrap();
        assert!(hub.contains("[[2023]]"));
        assert!(hub.contains("[[2024]]"));
        assert!(hub.contains("message-vault-person"));

        assert!(
            out.join(".obsidian/snippets/message-vault-bubbles.css")
                .is_file()
        );
        assert!(out.join(format!("_assets/ab/{sha}.txt")).is_file());

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn sanitize_strips_path_chars() {
        assert_eq!(sanitize_name("A/B:C"), "A_B_C");
    }
}
