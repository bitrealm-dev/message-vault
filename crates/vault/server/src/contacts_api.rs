//! Contact list/detail used by `GET /v1/export/contacts`,
//! `GET|POST /v1/export/contacts/{id}`, and `POST /v1/export/contacts/summaries`.

use std::collections::{HashMap, HashSet};

use anyhow::{Result as AnyResult, bail};
use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::HeaderMap;
use message_ir::HandleType;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;

use crate::db::contacts::{self, contact_id_for_handle};
use crate::db::dialect::{engine_of, group_concat_unit_separator, like_ci_numbered};
use crate::db::engine::DbEngine;
use crate::db::handles::infer_handle_type_from_shape;
use crate::db::sql::in_placeholders;
use crate::export_api::ExportQueryError;
use crate::server::{ApiError, AppState, require_full_access, resolve_auth};

pub use crate::page_limits::{DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT, MAX_LIST_OFFSET};

/// One bound value in the dynamic list query. sqlx Any has no
/// user-constructible dynamic value, so binds ride this enum and are chained
/// onto the statement in order at execution time.
enum Bind {
    Text(String),
}

/// One page of the contact list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContactListPage {
    /// Contacts on this page.
    pub contacts: Vec<ContactSummary>,
    /// Total contacts matching the query.
    pub total: u64,
    /// Page size used.
    pub limit: usize,
    /// Page offset used.
    pub offset: usize,
}

/// Contact row for the list: name, handles, groups.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContactSummary {
    /// Contact id.
    pub id: i64,
    /// Contact display name.
    pub name: String,
    /// Number of handles linked to the contact.
    pub handle_count: u64,
    /// Normalized (and raw when distinct) handle values for client-side filter.
    #[serde(default)]
    pub handles: Vec<String>,
    /// When the contact’s address-book shape last changed (`datetime('now')`).
    pub last_modified: String,
    /// Group names on this contact (A–Z).
    #[serde(default)]
    pub groups: Vec<String>,
}

/// One handle on a contact with service and message stats.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContactHandleInfo {
    /// Normalized handle value.
    pub handle: String,
    /// Platform service, e.g. `whatsapp`, when the handle is linked with one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Per-service alias from the address book, when linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_alias: Option<String>,
    /// Date of the first message involving this handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// Date of the last message involving this handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    /// 1:1 conversations this handle appears in.
    pub individual_conversations: u64,
    /// Group conversations this handle appears in.
    pub group_conversations: u64,
    /// Messages in 1:1 conversations involving this handle.
    pub individual_message_count: u64,
    /// Messages in group conversations involving this handle.
    pub group_message_count: u64,
}

/// A handle value plus optional platform service.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ContactHandlePayload {
    /// Handle value to link.
    pub handle: String,
    /// Platform service (`phone`, `email`, or `whatsapp`); inferred when omitted.
    #[serde(default)]
    pub service: Option<String>,
}

/// The previous and new handle values for a link change.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ContactUpdateHandlePayload {
    /// Handle value currently linked.
    pub previous_handle: String,
    /// Replacement handle value.
    pub handle: String,
    /// Platform service for the new handle.
    #[serde(default)]
    pub service: Option<String>,
}

/// The handle to unlink.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ContactRemoveHandlePayload {
    /// Handle value to unlink.
    pub handle: String,
    /// Platform service, when the handle is linked with one.
    #[serde(default)]
    pub service: Option<String>,
}

/// Body for `POST /v1/export/contacts/{id}`. Exactly one mutation field should be set.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ContactMutationBody {
    /// New display name; `None` leaves it unchanged.
    #[serde(default)]
    pub name: Option<String>,
    /// Handle link to add.
    #[serde(default)]
    pub add_handle: Option<ContactHandlePayload>,
    /// Handle link to replace.
    #[serde(default)]
    pub update_handle: Option<ContactUpdateHandlePayload>,
    /// Handle link to remove.
    #[serde(default)]
    pub remove_handle: Option<ContactRemoveHandlePayload>,
}

/// Full contact view: every handle with stats, plus totals across them.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContactDetail {
    /// Contact id.
    pub id: i64,
    /// Contact display name.
    pub name: String,
    /// Every handle linked to the contact, with per-handle stats.
    pub handles: Vec<ContactHandleInfo>,
    /// 1:1 conversations the contact appears in.
    pub direct_conversations: u64,
    /// Group conversations the contact appears in.
    pub group_conversations: u64,
    /// Messages across all of the contact's conversations.
    pub total_messages: u64,
    /// When the contact’s address-book shape last changed (`datetime('now')`).
    pub last_modified: String,
    /// Group names on this contact (A–Z).
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Body for `POST /v1/export/contacts/summaries`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ContactSummariesBody {
    /// Contact ids to summarize; an empty list covers every contact.
    #[serde(default)]
    pub ids: Vec<i64>,
}

/// Contact-level first/last seen and message counts for the selection table.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContactSelectionSummary {
    /// Contact id.
    pub id: i64,
    /// Contact display name.
    pub name: String,
    /// Date of the contact's first message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// Date of the contact's last message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    /// 1:1 conversations with the contact.
    pub individual_conversations: u64,
    /// Group conversations with the contact.
    pub group_conversations: u64,
    /// Messages in 1:1 conversations with the contact.
    pub individual_message_count: u64,
    /// Messages in group conversations with the contact.
    pub group_message_count: u64,
}

/// Response for `POST /v1/export/contacts/summaries`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContactSummariesPage {
    /// One summary per requested contact.
    pub contacts: Vec<ContactSelectionSummary>,
}

/// A contact is linked to a conversation when one of its handles is either
/// the conversation's chat handle or a participant handle in it.
///
/// `contact_id_expr` is the SQL expression for the contact id (`$N` or `ct.id`).
pub(crate) fn involves_contact_expr(contact_id_expr: &str) -> String {
    format!(
        "EXISTS (
       SELECT 1 FROM contact_handles ch
       WHERE ch.account_id = c.account_id
         AND ch.contact_id = {contact_id_expr}
         AND (
           ch.handle_id = c.chat_handle_id
           OR EXISTS (
             SELECT 1 FROM participants p
             WHERE p.conversation_id = c.id AND p.handle_id = ch.handle_id
           )
         )
     )"
    )
}

/// Expects two bind parameters: `account_id` ($1), `contact_id` ($2).
/// Alias `c` = conversations.
pub(crate) fn involves_contact_sql() -> String {
    involves_contact_expr("$2")
}

/// Conversation `c` is not in `trashed_conversations`.
pub(crate) const NOT_TRASHED_CONVERSATION_SQL: &str = "NOT EXISTS (
               SELECT 1 FROM trashed_conversations tc
               WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
             )";

/// Conversation `c`'s chat handle is not in `trashed_handles`.
pub(crate) const NOT_TRASHED_CHAT_HANDLE_SQL: &str = "NOT EXISTS (
               SELECT 1 FROM trashed_handles th
               WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id
             )";

/// Contact `ct` is not in `trashed_contacts`.
pub(crate) const NOT_TRASHED_CONTACT_SQL: &str = "NOT EXISTS (
               SELECT 1 FROM trashed_contacts tct
               WHERE tct.account_id = ct.account_id AND tct.contact_id = ct.id
             )";

/// Correlated: conversation `c` involves contact row `ct` (no bind params).
fn involves_ct_sql() -> String {
    involves_contact_expr("ct.id")
}

/// Contact has at least one non-duplicate message in an involved conversation.
fn contact_has_messages_sql() -> String {
    format!(
        "EXISTS (
           SELECT 1
           FROM conversations c
           JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
           WHERE c.account_id = ct.account_id
             AND {involves}
         )",
        involves = involves_ct_sql()
    )
}

/// Comparison for a first-/last-contact date bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateBoundOp {
    /// Calendar day on or after (`>=` or bare `first-contact:`).
    OnOrAfter,
    /// Strictly before that calendar day (`<`).
    Before,
    /// Calendar day on or before (bare `last-contact:` back-compat).
    OnOrBefore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DateBound {
    op: DateBoundOp,
    ymd: String,
}

fn date_bound_cmp(op: DateBoundOp) -> &'static str {
    match op {
        DateBoundOp::OnOrAfter => ">=",
        DateBoundOp::Before => "<",
        DateBoundOp::OnOrBefore => "<=",
    }
}

fn involved_message_date_agg(involves: &str, min_or_max: &str) -> String {
    format!(
        "date((
           SELECT {min_or_max}(m.timestamp)
           FROM conversations c
           JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
           WHERE c.account_id = ct.account_id
             AND {involves}
         ))"
    )
}

fn push_contact_date_bounds(
    where_parts: &mut Vec<String>,
    params: &mut Vec<Bind>,
    bounds: &[DateBound],
    agg: &str,
) {
    for bound in bounds {
        let n = params.len() + 1;
        where_parts.push(format!("{agg} {} date(${n})", date_bound_cmp(bound.op)));
        params.push(Bind::Text(bound.ymd.clone()));
    }
}

#[derive(Debug, Default)]
struct ContactListFilters {
    handle: Option<String>,
    text: String,
    /// Bounds on earliest message day (AND’d).
    first_contact: Vec<DateBound>,
    /// Bounds on latest message day (AND’d).
    last_contact: Vec<DateBound>,
    /// `Some(true)` = has messages; `Some(false)` = never messaged.
    has_messages: Option<bool>,
    no_name: bool,
    /// Contacts with no rows in `contact_handles`.
    no_handle: bool,
    /// Lowercased service ids (`imessage`, `sms`, `mms`, `whatsapp`); OR match.
    /// UI may send `service:phone` (Text message), which expands to imessage/sms/mms.
    services: Vec<String>,
    /// Contacts that belong to this group (case-insensitive).
    group: Option<String>,
    /// Contacts with no group memberships (`group:none` / `has:no-group`).
    no_group: bool,
}

fn normalize_ymd(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.len() >= 10 && t.as_bytes().get(4) == Some(&b'-') && t.as_bytes().get(7) == Some(&b'-') {
        let ymd = &t[..10];
        if ymd.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
            return Some(ymd.to_string());
        }
    }
    None
}

/// Parse `>=YYYY-MM-DD`, `<YYYY-MM-DD`, or bare `YYYY-MM-DD`.
/// Bare dates use `bare` (OnOrAfter for first-contact, OnOrBefore for last-contact).
fn parse_date_bound_value(raw: &str, bare: DateBoundOp) -> Option<DateBound> {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix(">=") {
        return normalize_ymd(rest).map(|ymd| DateBound {
            op: DateBoundOp::OnOrAfter,
            ymd,
        });
    }
    // Prefer `<` over bare; do not treat `<=` as Before.
    if let Some(rest) = t.strip_prefix('<') {
        if rest.starts_with('=') {
            return None;
        }
        return normalize_ymd(rest).map(|ymd| DateBound {
            op: DateBoundOp::Before,
            ymd,
        });
    }
    normalize_ymd(t).map(|ymd| DateBound { op: bare, ymd })
}

fn expand_service_token(value: &str) -> Vec<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        // UI "Text message" / drawer phone bucket: any non-WhatsApp messaging service.
        "phone" | "text" | "text-message" | "textmessage" => {
            vec!["imessage".into(), "sms".into(), "mms".into()]
        }
        "sms" | "mms" | "sms/mms" | "sms-mms" => vec!["sms".into(), "mms".into()],
        "imessage" => vec!["imessage".into()],
        "whatsapp" => vec!["whatsapp".into()],
        other if !other.is_empty() => vec![other.to_string()],
        _ => Vec::new(),
    }
}

/// Pull `prefix:"quoted value"` or `prefix:bare` from `q`. Returns (value, remainder).
fn take_prefixed_quoted_or_bare(q: &str, prefix: &str) -> (Option<String>, String) {
    let q = q.trim();
    if q.is_empty() {
        return (None, String::new());
    }
    let lower = q.to_ascii_lowercase();
    let quoted = format!("{prefix}\"");
    if let Some(start) = lower.find(&quoted) {
        let after = start + quoted.len();
        if let Some(rel_end) = q[after..].find('"') {
            let end = after + rel_end;
            let value = q[after..end].to_string();
            let mut rest = String::new();
            rest.push_str(&q[..start]);
            rest.push_str(&q[end + 1..]);
            return (
                Some(value).filter(|s| !s.is_empty()),
                rest.split_whitespace().collect::<Vec<_>>().join(" "),
            );
        }
    }
    if let Some(start) = lower.find(prefix) {
        let after = start + prefix.len();
        if q.get(after..after + 1) != Some("\"") {
            let end = q[after..]
                .find(char::is_whitespace)
                .map(|i| after + i)
                .unwrap_or(q.len());
            if end > after {
                let value = q[after..end].trim_matches('"').to_string();
                let mut rest = String::new();
                rest.push_str(&q[..start]);
                rest.push_str(&q[end..]);
                return (
                    Some(value).filter(|s| !s.is_empty()),
                    rest.split_whitespace().collect::<Vec<_>>().join(" "),
                );
            }
        }
    }
    (None, q.to_string())
}

fn apply_group_token(out: &mut ContactListFilters, raw: &str) {
    let value = raw.trim();
    if value.is_empty() {
        return;
    }
    let lower = value.to_ascii_lowercase();
    if lower == "none" || lower == "no-group" || lower == "no-label" {
        out.no_group = true;
        out.group = None;
        return;
    }
    out.no_group = false;
    out.group = Some(value.to_string());
}

/// Parse `q` into structured list filters (handle, free text, advanced tokens).
fn parse_contact_list_filters(q: &str) -> ContactListFilters {
    let (handle, rest) = parse_contact_list_query(q);
    let (group_raw, rest) = take_prefixed_quoted_or_bare(&rest, "group:");
    let (label_raw, rest) = if group_raw.is_none() {
        take_prefixed_quoted_or_bare(&rest, "label:")
    } else {
        (None, rest)
    };
    let (within, rest) = if group_raw.is_none() && label_raw.is_none() {
        take_prefixed_quoted_or_bare(&rest, "within:")
    } else {
        (None, rest)
    };
    let mut out = ContactListFilters {
        handle,
        ..Default::default()
    };
    if let Some(ref raw) = group_raw.or(label_raw).or(within) {
        apply_group_token(&mut out, raw);
    }
    let mut text_parts = Vec::new();
    for tok in rest.split_whitespace() {
        let lower = tok.to_ascii_lowercase();
        if lower == "search:contacts" {
            continue;
        }
        if let Some(rest) = lower.strip_prefix("first-contact:") {
            if let Some(b) = parse_date_bound_value(rest, DateBoundOp::OnOrAfter) {
                out.first_contact.push(b);
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix("last-contact:") {
            if let Some(b) = parse_date_bound_value(rest, DateBoundOp::OnOrBefore) {
                out.last_contact.push(b);
            }
            continue;
        }
        if lower == "has:messages" {
            out.has_messages = Some(true);
            continue;
        }
        if lower == "has:no-messages" {
            out.has_messages = Some(false);
            continue;
        }
        if lower == "has:no-name" {
            out.no_name = true;
            continue;
        }
        if lower == "has:no-handle" {
            out.no_handle = true;
            continue;
        }
        if lower == "has:no-label" || lower == "has:no-group" {
            out.no_group = true;
            out.group = None;
            continue;
        }
        if let Some(rest) = lower.strip_prefix("service:") {
            for s in expand_service_token(rest) {
                if !out.services.iter().any(|x| x == &s) {
                    out.services.push(s);
                }
            }
            continue;
        }
        // Legacy / unsupported tokens — ignore.
        if lower.starts_with("message-count:")
            || lower.starts_with("group-count:")
            || lower.starts_with("handle:")
        {
            continue;
        }
        text_parts.push(tok);
    }
    out.text = text_parts.join(" ");
    out
}

/// Case-insensitive group-name equality for one engine (`COLLATE NOCASE` is
/// invalid Postgres SQL; Postgres uses `lower()`).
fn group_name_eq_sql(engine: DbEngine, placeholder: usize) -> String {
    match engine {
        DbEngine::Sqlite => format!("cl.name = ${placeholder} COLLATE NOCASE"),
        DbEngine::Postgres => format!("lower(cl.name) = lower(${placeholder})"),
    }
}

/// Flat list of contacts: id, display name, handle count, and handle values (paged).
///
/// `q` matches preferred name or any linked handle (raw/normalized), case-insensitive.
/// `handle:<raw>` restricts to contacts that have that handle substring.
/// Advanced tokens: `first-contact:` / `last-contact:` (optional `>=` / `<` prefix;
/// bare first = on or after, bare last = on or before; repeated tokens AND),
/// `has:messages`, `has:no-messages`, `has:no-name`, `has:no-handle`, `has:no-group`,
/// `group:` / `label:` / `within:` (one group, or `group:none`), `service:` (OR across services).
///
/// # Errors
///
/// Returns a bad-request error for an invalid query, or an internal error when
/// a database statement fails.
pub async fn list_contacts(
    conn: &mut AnyConnection,
    account_id: &str,
    q: &str,
    limit: usize,
    offset: usize,
) -> Result<ContactListPage, ExportQueryError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    if offset > MAX_LIST_OFFSET {
        return Err(ExportQueryError::bad(format!(
            "offset exceeds maximum of {MAX_LIST_OFFSET}"
        )));
    }

    crate::search_query::validate_list_search_query(q)?;
    let filters = parse_contact_list_filters(q);
    let engine = engine_of(conn);
    let involves = involves_ct_sql();
    let has_messages_sql = contact_has_messages_sql();

    let mut where_parts = vec![
        "ct.account_id = $1".to_string(),
        NOT_TRASHED_CONTACT_SQL.to_string(),
    ];
    let mut params: Vec<Bind> = vec![Bind::Text(account_id.to_string())];

    if let Some(ref handle) = filters.handle {
        let n = params.len() + 1;
        where_parts.push(format!(
            "EXISTS (
               SELECT 1 FROM contact_handles ch
               JOIN handles h ON h.id = ch.handle_id
               WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                 AND (h.raw {} OR coalesce(h.normalized, '') {})
             )",
            like_ci_numbered(engine, n),
            like_ci_numbered(engine, n + 1),
        ));
        let like = format!("%{handle}%");
        params.push(Bind::Text(like.clone()));
        params.push(Bind::Text(like));
    }

    if !filters.text.is_empty() {
        let n = params.len() + 1;
        where_parts.push(format!(
            "(COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)') {}
              OR EXISTS (
                SELECT 1 FROM contact_handles ch
                JOIN handles h ON h.id = ch.handle_id
                WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                  AND (h.raw {} OR coalesce(h.normalized, '') {})
              ))",
            like_ci_numbered(engine, n),
            like_ci_numbered(engine, n + 1),
            like_ci_numbered(engine, n + 2),
        ));
        let like = format!("%{}%", filters.text);
        params.push(Bind::Text(like.clone()));
        params.push(Bind::Text(like.clone()));
        params.push(Bind::Text(like));
    }

    match filters.has_messages {
        Some(true) => where_parts.push(has_messages_sql.clone()),
        Some(false) => where_parts.push(format!("NOT {has_messages_sql}")),
        None => {}
    }

    if filters.no_name {
        where_parts.push(
            "(NULLIF(trim(ct.preferred_name), '') IS NULL
              OR EXISTS (
                SELECT 1 FROM contact_handles ch
                JOIN handles h ON h.id = ch.handle_id
                WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                  AND (
                    lower(trim(ct.preferred_name)) = lower(trim(h.raw))
                    OR (
                      h.normalized IS NOT NULL
                      AND trim(h.normalized) != ''
                      AND lower(trim(ct.preferred_name)) = lower(trim(h.normalized))
                    )
                  )
              ))"
            .into(),
        );
    }

    if filters.no_handle {
        where_parts.push(
            "NOT EXISTS (
               SELECT 1 FROM contact_handles ch
               WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
             )"
            .into(),
        );
    }

    if let Some(ref label) = filters.group {
        let n = params.len() + 1;
        where_parts.push(format!(
            "EXISTS (
               SELECT 1 FROM contact_group_members clm
               JOIN contact_groups cl ON cl.id = clm.group_id
               WHERE clm.contact_id = ct.id
                 AND cl.account_id = ct.account_id
                 AND {}
             )",
            group_name_eq_sql(engine, n),
        ));
        params.push(Bind::Text(label.clone()));
    } else if filters.no_group {
        where_parts.push(
            "NOT EXISTS (
               SELECT 1 FROM contact_group_members clm
               JOIN contact_groups cl ON cl.id = clm.group_id
               WHERE clm.contact_id = ct.id
                 AND cl.account_id = ct.account_id
             )"
            .into(),
        );
    }

    if !filters.services.is_empty() {
        let n = params.len() + 1;
        let placeholders = in_placeholders(n, filters.services.len());
        where_parts.push(format!(
            "EXISTS (
               SELECT 1 FROM conversations c
               JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
               WHERE c.account_id = ct.account_id
                 AND lower(m.service) IN ({placeholders})
                 AND {involves}
             )"
        ));
        for s in &filters.services {
            params.push(Bind::Text(s.clone()));
        }
    }

    push_contact_date_bounds(
        &mut where_parts,
        &mut params,
        &filters.first_contact,
        &involved_message_date_agg(&involves, "MIN"),
    );
    push_contact_date_bounds(
        &mut where_parts,
        &mut params,
        &filters.last_contact,
        &involved_message_date_agg(&involves, "MAX"),
    );

    let where_sql = where_parts.join(" AND ");
    let count_sql = format!("SELECT COUNT(*) FROM contacts ct WHERE {where_sql}");
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    for p in &params {
        match p {
            Bind::Text(v) => count_q = count_q.bind(v.clone()),
        }
    }
    let total: i64 = count_q.fetch_one(&mut *conn).await?;
    let total = total.max(0) as u64;

    let order_by = match engine {
        DbEngine::Sqlite => "ORDER BY name COLLATE NOCASE, ct.id",
        DbEngine::Postgres => "ORDER BY lower(name), ct.id",
    };
    let page_n = params.len() + 1;
    let sql = format!(
        "SELECT ct.id,
                COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)') AS name,
                (SELECT COUNT(*)
                 FROM contact_handles ch
                 WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id) AS handle_count,
                (SELECT {handles_agg}
                 FROM (
                   SELECT DISTINCT h.normalized AS val
                   FROM contact_handles ch
                   JOIN handles h ON h.id = ch.handle_id
                   WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                     AND h.normalized IS NOT NULL AND trim(h.normalized) != ''
                   UNION
                   SELECT DISTINCT h.raw AS val
                   FROM contact_handles ch
                   JOIN handles h ON h.id = ch.handle_id
                   WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id
                     AND h.raw IS NOT NULL AND trim(h.raw) != ''
                 )) AS handles,
                ct.last_modified,
                (SELECT {groups_agg}
                 FROM contact_group_members clm
                 JOIN contact_groups cl ON cl.id = clm.group_id
                 WHERE clm.contact_id = ct.id AND cl.account_id = ct.account_id) AS groups
         FROM contacts ct
         WHERE {where_sql}
         {order_by}
         LIMIT ${page_n} OFFSET ${page_n_plus}",
        handles_agg = group_concat_unit_separator(engine, "val"),
        groups_agg = group_concat_unit_separator(engine, "cl.name"),
        page_n = page_n,
        page_n_plus = page_n + 1
    );
    let mut page_q = sqlx::query_as::<_, ContactRow>(&sql);
    for p in &params {
        match p {
            Bind::Text(v) => page_q = page_q.bind(v.clone()),
        }
    }
    let rows: Vec<ContactRow> = page_q
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&mut *conn)
        .await?;

    let contacts = rows
        .into_iter()
        .map(
            |(id, name, handle_count, handles_blob, last_modified, groups_blob)| {
                let handles = handles_blob
                    .map(|s| {
                        s.split('\u{1f}')
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut groups = groups_blob
                    .map(|s| {
                        s.split('\u{1f}')
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                groups.sort_by_key(|a| a.to_ascii_lowercase());
                ContactSummary {
                    id,
                    name,
                    handle_count: handle_count.max(0) as u64,
                    handles,
                    last_modified,
                    groups,
                }
            },
        )
        .collect();

    Ok(ContactListPage {
        contacts,
        total,
        limit,
        offset,
    })
}

type ContactRow = (i64, String, i64, Option<String>, String, Option<String>);

/// Parse `q` into optional handle filter + free-text remainder.
fn parse_contact_list_query(q: &str) -> (Option<String>, String) {
    let q = q.trim();
    if q.is_empty() {
        return (None, String::new());
    }
    let lower = q.to_ascii_lowercase();
    if let Some(start) = lower.find("handle:\"") {
        let after = start + "handle:\"".len();
        if let Some(rel_end) = q[after..].find('"') {
            let end = after + rel_end;
            let handle = q[after..end].to_string();
            let mut rest = String::new();
            rest.push_str(&q[..start]);
            rest.push_str(&q[end + 1..]);
            return (
                Some(handle).filter(|s| !s.is_empty()),
                rest.split_whitespace().collect::<Vec<_>>().join(" "),
            );
        }
    }
    if let Some(start) = lower.find("handle:") {
        let after = start + "handle:".len();
        if q.get(after..after + 1) != Some("\"") {
            let end = q[after..]
                .find(char::is_whitespace)
                .map(|i| after + i)
                .unwrap_or(q.len());
            if end > after {
                let handle = q[after..end].trim_matches('"').to_string();
                let mut rest = String::new();
                rest.push_str(&q[..start]);
                rest.push_str(&q[end..]);
                return (
                    Some(handle).filter(|s| !s.is_empty()),
                    rest.split_whitespace().collect::<Vec<_>>().join(" "),
                );
            }
        }
    }
    (None, q.to_string())
}

/// Full contact view: per-handle service + date range + direct message count,
/// plus conversation and total-message stats across all the contact's handles.
///
/// # Errors
///
/// Returns an internal error when a database statement fails.
pub async fn get_contact_detail(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
) -> Result<Option<ContactDetail>, ExportQueryError> {
    let name_and_modified: Option<(String, String)> = sqlx::query_as(&format!(
        "SELECT COALESCE(NULLIF(trim(preferred_name), ''), '(unknown)'),
                last_modified
         FROM contacts ct
         WHERE ct.id = $1 AND ct.account_id = $2
           AND {not_trashed}",
        not_trashed = NOT_TRASHED_CONTACT_SQL,
    ))
    .bind(contact_id)
    .bind(account_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((name, last_modified)) = name_and_modified else {
        return Ok(None);
    };

    // One row per handle. Date range and message counts cover direct + group
    // conversations that include the handle (excluding trashed conversations).
    let rows: Vec<ContactHandleRow> = sqlx::query_as(&format!(
        "SELECT h.raw,
                    NULLIF(trim(h.service), '') AS service,
                    NULLIF(trim(ch.name_alias), '') AS name_alias,
                    MIN(m.timestamp) AS first_ts,
                    MAX(m.timestamp) AS last_ts,
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'individual' THEN c.id END),
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'group' THEN c.id END),
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'individual' THEN m.id END),
                    COUNT(DISTINCT CASE WHEN c.conversation_type = 'group' THEN m.id END)
             FROM contact_handles ch
             JOIN handles h ON h.id = ch.handle_id
             LEFT JOIN conversations c ON c.account_id = ch.account_id
               AND (c.chat_handle_id = ch.handle_id
                    OR EXISTS (
                      SELECT 1 FROM participants p
                      WHERE p.conversation_id = c.id AND p.handle_id = ch.handle_id
                    ))
               AND {not_trashed_conversation}
               AND {not_trashed_handle}
             LEFT JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
             WHERE ch.account_id = $1 AND ch.contact_id = $2
             GROUP BY ch.handle_id, h.raw, h.service, ch.name_alias
             ORDER BY h.raw",
        not_trashed_conversation = NOT_TRASHED_CONVERSATION_SQL,
        not_trashed_handle = NOT_TRASHED_CHAT_HANDLE_SQL,
    ))
    .bind(account_id)
    .bind(contact_id)
    .fetch_all(&mut *conn)
    .await?;
    let handles = rows
        .into_iter()
        .map(
            |(
                handle,
                service,
                name_alias,
                start_date,
                end_date,
                individual_conversations,
                group_conversations,
                individual_message_count,
                group_message_count,
            )| ContactHandleInfo {
                handle,
                service,
                name_alias,
                start_date,
                end_date,
                individual_conversations: individual_conversations.max(0) as u64,
                group_conversations: group_conversations.max(0) as u64,
                individual_message_count: individual_message_count.max(0) as u64,
                group_message_count: group_message_count.max(0) as u64,
            },
        )
        .collect();

    // Conversation + message stats across handles of this contact only.
    // Do not GROUP BY the entire account messages table — that dominated drawer latency.
    let (direct, groups, total): (i64, i64, i64) = sqlx::query_as(&format!(
        "WITH involved AS (
               SELECT c.id, c.conversation_type
               FROM conversations c
               WHERE c.account_id = $1
                 AND {involves_contact_sql}
                 AND {not_trashed_conversation}
                 AND {not_trashed_handle}
             )
             SELECT
               (SELECT COUNT(*) FROM involved WHERE conversation_type = 'individual'),
               (SELECT COUNT(*) FROM involved WHERE conversation_type = 'group'),
               (SELECT COUNT(*) FROM messages m
                WHERE m.duplicate_of IS NULL
                  AND m.conversation_id IN (SELECT id FROM involved))",
        involves_contact_sql = involves_contact_sql(),
        not_trashed_conversation = NOT_TRASHED_CONVERSATION_SQL,
        not_trashed_handle = NOT_TRASHED_CHAT_HANDLE_SQL,
    ))
    .bind(account_id)
    .bind(contact_id)
    .fetch_one(&mut *conn)
    .await?;

    let contact_groups =
        crate::contact_groups_api::groups_for_contact(conn, account_id, contact_id).await?;

    Ok(Some(ContactDetail {
        id: contact_id,
        name,
        handles,
        direct_conversations: direct.max(0) as u64,
        group_conversations: groups.max(0) as u64,
        total_messages: total.max(0) as u64,
        last_modified,
        groups: contact_groups,
    }))
}

type ContactHandleRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
    i64,
);

/// First/last seen and message counts for many contacts in one grouped query.
///
/// Unknown, trashed, and duplicate ids are skipped. At most [`MAX_LIST_LIMIT`]
/// ids are read so the `IN` list stays under SQLite's variable cap.
///
/// # Errors
///
/// Returns an internal error when a database statement fails.
pub async fn get_contact_summaries(
    conn: &mut AnyConnection,
    account_id: &str,
    ids: &[i64],
) -> Result<Vec<ContactSelectionSummary>, ExportQueryError> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids.iter().copied() {
        if id <= 0 || !seen.insert(id) {
            continue;
        }
        unique.push(id);
        if unique.len() == MAX_LIST_LIMIT {
            break;
        }
    }
    if unique.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = in_placeholders(2, unique.len());
    let involves = involves_contact_expr("selected.id");
    let sql = format!(
        "WITH selected AS (
            SELECT ct.id,
                   ct.account_id,
                   COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)') AS name
            FROM contacts ct
            WHERE ct.account_id = $1
              AND ct.id IN ({placeholders})
              AND {not_trashed}
         ),
         involved AS (
            SELECT selected.id AS contact_id, c.id AS conversation_id, c.conversation_type
            FROM selected
            JOIN conversations c ON c.account_id = selected.account_id
              AND {involves}
              AND {not_trashed_conversation}
              AND {not_trashed_handle}
         )
         SELECT
            s.id,
            s.name,
            MIN(m.timestamp) AS start_date,
            MAX(m.timestamp) AS end_date,
            COUNT(DISTINCT CASE WHEN i.conversation_type = 'individual' THEN i.conversation_id END),
            COUNT(DISTINCT CASE WHEN i.conversation_type = 'group' THEN i.conversation_id END),
            COUNT(DISTINCT CASE WHEN i.conversation_type = 'individual' THEN m.id END),
            COUNT(DISTINCT CASE WHEN i.conversation_type = 'group' THEN m.id END)
         FROM selected s
         LEFT JOIN involved i ON i.contact_id = s.id
         LEFT JOIN messages m ON m.conversation_id = i.conversation_id
           AND m.duplicate_of IS NULL
         GROUP BY s.id, s.name",
        not_trashed = NOT_TRASHED_CONTACT_SQL,
        not_trashed_conversation = NOT_TRASHED_CONVERSATION_SQL,
        not_trashed_handle = NOT_TRASHED_CHAT_HANDLE_SQL,
    );

    let mut q = sqlx::query_as::<_, ContactSelectionRow>(&sql);
    q = q.bind(account_id);
    for id in &unique {
        q = q.bind(*id);
    }
    let rows: Vec<ContactSelectionRow> = q.fetch_all(&mut *conn).await?;

    let mut by_id = HashMap::new();
    for (
        id,
        name,
        start_date,
        end_date,
        individual_conversations,
        group_conversations,
        individual_message_count,
        group_message_count,
    ) in rows
    {
        by_id.insert(
            id,
            ContactSelectionSummary {
                id,
                name,
                start_date,
                end_date,
                individual_conversations: individual_conversations.max(0) as u64,
                group_conversations: group_conversations.max(0) as u64,
                individual_message_count: individual_message_count.max(0) as u64,
                group_message_count: group_message_count.max(0) as u64,
            },
        );
    }
    Ok(unique
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect())
}

type ContactSelectionRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
    i64,
);

fn infer_handle_type(raw: &str, service: Option<&str>) -> HandleType {
    let svc = service
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match svc.as_str() {
        "phone" | "sms" | "imessage" | "whatsapp" => HandleType::Phone,
        "email" => HandleType::Email,
        "" => infer_handle_type_from_shape(raw),
        _ => HandleType::Other,
    }
}

async fn find_contact_handle_id(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
    raw: &str,
    service: Option<&str>,
) -> AnyResult<Option<i64>> {
    let needle = raw.trim();
    if needle.is_empty() {
        return Ok(None);
    }
    let mut sql = String::from(
        "SELECT ch.handle_id
         FROM contact_handles ch
         JOIN handles h ON h.id = ch.handle_id
         WHERE ch.account_id = $1 AND ch.contact_id = $2
           AND (h.raw = $3 OR h.normalized = $3)",
    );
    let id = if let Some(svc) = service.map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(" AND h.service = $4 LIMIT 1");
        let platform = message_ir::HandleService::parse(svc);
        sqlx::query_scalar::<_, i64>(&sql)
            .bind(account_id)
            .bind(contact_id)
            .bind(needle)
            .bind(platform.as_str())
            .fetch_optional(&mut *conn)
            .await?
    } else {
        sql.push_str(
            " ORDER BY CASE h.service WHEN 'phone' THEN 0 WHEN 'whatsapp' THEN 1 ELSE 2 END
             LIMIT 1",
        );
        sqlx::query_scalar::<_, i64>(&sql)
            .bind(account_id)
            .bind(contact_id)
            .bind(needle)
            .fetch_optional(&mut *conn)
            .await?
    };
    Ok(id)
}

async fn ensure_handle_row(
    conn: &mut AnyConnection,
    account_id: &str,
    raw: &str,
    service: Option<&str>,
) -> AnyResult<i64> {
    let handle_type = infer_handle_type(raw, service);
    // Contact-owned handles must not be linked as account owner identities.
    let (id, _) = crate::db::handles::upsert_handle_row(
        conn,
        account_id,
        raw.trim(),
        handle_type,
        service.map(|s| s.trim()).filter(|s| !s.is_empty()),
    )
    .await?;
    Ok(id)
}

async fn contact_exists(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
) -> AnyResult<bool> {
    let found: Option<i64> = sqlx::query_scalar(&format!(
        "SELECT ct.id
         FROM contacts ct
         WHERE ct.id = $1 AND ct.account_id = $2
           AND {not_trashed}",
        not_trashed = NOT_TRASHED_CONTACT_SQL,
    ))
    .bind(contact_id)
    .bind(account_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(found.is_some())
}

/// Apply a contact mutation. Returns false when the contact is missing.
///
/// # Errors
///
/// Returns an error when the mutation is invalid or a database write fails.
pub async fn mutate_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
    body: &ContactMutationBody,
) -> AnyResult<bool> {
    if !contact_exists(conn, account_id, contact_id).await? {
        return Ok(false);
    }

    let set_count = [
        body.name.is_some(),
        body.add_handle.is_some(),
        body.update_handle.is_some(),
        body.remove_handle.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count();
    if set_count != 1 {
        bail!("exactly one of name, add_handle, update_handle, remove_handle is required");
    }

    if let Some(name) = body.name.as_ref() {
        let name = name.trim();
        if name.is_empty() {
            bail!("name must not be empty");
        }
        sqlx::query("UPDATE contacts SET preferred_name = $1 WHERE id = $2 AND account_id = $3")
            .bind(name)
            .bind(contact_id)
            .bind(account_id)
            .execute(&mut *conn)
            .await?;
        return touch_ok(conn, account_id, contact_id).await;
    }

    if let Some(add) = body.add_handle.as_ref() {
        let raw = add.handle.trim();
        if raw.is_empty() {
            bail!("handle must not be empty");
        }
        let handle_id = ensure_handle_row(conn, account_id, raw, add.service.as_deref()).await?;
        // One contact per handle (PK on contact_handles.handle_id + account).
        if require_handle_available(conn, account_id, handle_id, contact_id)
            .await?
            .is_some()
        {
            // Already linked — no address-book change.
            return Ok(true);
        }
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(account_id)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await?;
        return touch_ok(conn, account_id, contact_id).await;
    }

    if let Some(upd) = body.update_handle.as_ref() {
        let prev = upd.previous_handle.trim();
        let next = upd.handle.trim();
        if prev.is_empty() || next.is_empty() {
            bail!("previous_handle and handle must not be empty");
        }
        let Some(old_id) =
            find_contact_handle_id(conn, account_id, contact_id, prev, upd.service.as_deref())
                .await?
        else {
            bail!("previous handle not found on contact");
        };
        let new_id = ensure_handle_row(conn, account_id, next, upd.service.as_deref()).await?;
        if old_id == new_id {
            if let Some(svc) = upd
                .service
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                sqlx::query("UPDATE handles SET service = $1 WHERE id = $2")
                    .bind(svc)
                    .bind(new_id)
                    .execute(&mut *conn)
                    .await?;
                return touch_ok(conn, account_id, contact_id).await;
            }
            return Ok(true);
        }
        if require_handle_available(conn, account_id, new_id, contact_id)
            .await?
            .is_some()
        {
            // Already on this contact — drop the previous link.
            sqlx::query(
                "DELETE FROM contact_handles
                 WHERE account_id = $1 AND contact_id = $2 AND handle_id = $3",
            )
            .bind(account_id)
            .bind(contact_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await?;
            return touch_ok(conn, account_id, contact_id).await;
        }
        sqlx::query(
            "UPDATE contact_handles SET handle_id = $1
             WHERE account_id = $2 AND contact_id = $3 AND handle_id = $4",
        )
        .bind(new_id)
        .bind(account_id)
        .bind(contact_id)
        .bind(old_id)
        .execute(&mut *conn)
        .await?;
        return touch_ok(conn, account_id, contact_id).await;
    }

    if let Some(rem) = body.remove_handle.as_ref() {
        let raw = rem.handle.trim();
        if raw.is_empty() {
            bail!("handle must not be empty");
        }
        let Some(handle_id) =
            find_contact_handle_id(conn, account_id, contact_id, raw, rem.service.as_deref())
                .await?
        else {
            bail!("handle not found on contact");
        };
        sqlx::query(
            "DELETE FROM contact_handles
             WHERE account_id = $1 AND contact_id = $2 AND handle_id = $3",
        )
        .bind(account_id)
        .bind(contact_id)
        .bind(handle_id)
        .execute(&mut *conn)
        .await?;
        return touch_ok(conn, account_id, contact_id).await;
    }

    Ok(true)
}

async fn require_handle_available(
    conn: &mut AnyConnection,
    account_id: &str,
    handle_id: i64,
    contact_id: i64,
) -> AnyResult<Option<i64>> {
    let existing = contact_id_for_handle(conn, account_id, handle_id).await?;
    if let Some(other) = existing
        && other != contact_id
    {
        bail!("handle already linked to another contact");
    }
    Ok(existing)
}

async fn touch_ok(conn: &mut AnyConnection, account_id: &str, contact_id: i64) -> AnyResult<bool> {
    contacts::touch_contact(conn, account_id, contact_id).await?;
    Ok(true)
}

/// Page through the account's contacts (id, name, handles, groups).
#[utoipa::path(
    get,
    path = "/v1/export/contacts",
    tag = "Contacts",
    security(("bearer" = [])),
    params(
        ("q" = Option<String>, Query, description = "Contact search; empty lists all"),
        ("limit" = Option<usize>, Query, description = "Page size"),
        ("offset" = Option<usize>, Query, description = "Page offset")
    ),
    responses(
        (status = 200, body = ContactListPage),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contacts_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<crate::server::ListPageQuery>,
) -> Result<Json<ContactListPage>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let q = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let page = list_contacts(&mut conn, &auth.account_id, &q, limit, offset).await?;
    Ok(Json(page))
}

/// First/last message dates and counts for a list of contact ids.
#[utoipa::path(
    post,
    path = "/v1/export/contacts/summaries",
    tag = "Contacts",
    security(("bearer" = [])),
    request_body = ContactSummariesBody,
    responses(
        (status = 200, body = ContactSummariesPage),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_summaries_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ContactSummariesBody>,
) -> Result<Json<ContactSummariesPage>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    if body.ids.len() > MAX_LIST_LIMIT {
        return Err(ApiError::BadRequest(format!(
            "at most {} contact ids",
            MAX_LIST_LIMIT
        )));
    }
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let page = get_contact_summaries(&mut conn, &auth.account_id, &body.ids)
        .await
        .map(|contacts| ContactSummariesPage { contacts })?;
    Ok(Json(page))
}

/// Full contact view: per-handle services, message stats, and group
/// memberships.
#[utoipa::path(
    get,
    path = "/v1/export/contacts/{id}",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact id")),
    responses(
        (status = 200, body = ContactDetail),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_detail_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(contact_id): AxumPath<i64>,
) -> Result<Json<ContactDetail>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let detail = get_contact_detail(&mut conn, &auth.account_id, contact_id).await?;
    detail
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("contact not found".into()))
}

/// Rename a contact or change its linked handles.
#[utoipa::path(
    post,
    path = "/v1/export/contacts/{id}",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact id")),
    request_body = ContactMutationBody,
    responses(
        (status = 200, body = ContactDetail),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_mutate_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(contact_id): AxumPath<i64>,
    Json(body): Json<ContactMutationBody>,
) -> Result<Json<ContactDetail>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_full_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    match mutate_contact(&mut conn, &auth.account_id, contact_id, &body).await {
        Ok(false) => Err(ApiError::NotFound("contact not found".into())),
        Err(e) => Err(ApiError::BadRequest(e.to_string())),
        Ok(true) => get_contact_detail(&mut conn, &auth.account_id, contact_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::Internal("contact missing after mutate".into()))
            .map(Json),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::{account_profile, engine, schema};

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir, String) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000c1".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir, account)
    }

    #[tokio::test]
    async fn list_contacts_uses_preferred_name_and_handle_ids() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Pat') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let handle_id = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550100",
            HandleType::Phone,
        )
        .await
        .unwrap();
        // link_account_handle puts it on account_handles; also link as contact handle.
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = list_contacts(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.contacts.len(), 1);
        assert_eq!(page.contacts[0].name, "Pat");
        assert_eq!(page.contacts[0].handle_count, 1);
        assert!(
            page.contacts[0]
                .handles
                .iter()
                .any(|h| h.contains("5555550100") || h.contains("+15555550100")),
            "handles={:?}",
            page.contacts[0].handles
        );
    }

    #[tokio::test]
    async fn list_contacts_filters_and_paginates() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        for (name, phone) in [
            ("Pat", "+15555550100"),
            ("Sam", "+15555550200"),
            ("Alex", "+15555550300"),
        ] {
            let contact_id: i64 = sqlx::query_scalar(
                "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
            )
            .bind(&account)
            .bind(name)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            let handle_id =
                account_profile::link_account_handle(&mut conn, &account, phone, HandleType::Phone)
                    .await
                    .unwrap();
            sqlx::query(
                "INSERT INTO contact_handles (account_id, handle_id, contact_id)
                 VALUES ($1, $2, $3)",
            )
            .bind(&account)
            .bind(handle_id)
            .bind(contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        }

        let by_name = list_contacts(&mut conn, &account, "sam", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(by_name.total, 1);
        assert_eq!(by_name.contacts[0].name, "Sam");

        let by_handle = list_contacts(
            &mut conn,
            &account,
            "handle:5555550200",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(by_handle.total, 1);
        assert_eq!(by_handle.contacts[0].name, "Sam");

        let page0 = list_contacts(&mut conn, &account, "", 2, 0).await.unwrap();
        assert_eq!(page0.total, 3);
        assert_eq!(page0.limit, 2);
        assert_eq!(page0.offset, 0);
        assert_eq!(page0.contacts.len(), 2);
        let page1 = list_contacts(&mut conn, &account, "", 2, 2).await.unwrap();
        assert_eq!(page1.total, 3);
        assert_eq!(page1.offset, 2);
        assert_eq!(page1.contacts.len(), 1);

        let clamped = list_contacts(&mut conn, &account, "", MAX_LIST_LIMIT + 50, 0)
            .await
            .unwrap();
        assert_eq!(clamped.limit, MAX_LIST_LIMIT);
        assert_eq!(clamped.total, 3);
    }

    #[tokio::test]
    async fn get_contact_detail_counts_direct_group_and_messages() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let peer = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550200",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(peer)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        // Direct conversation with 2 messages.
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (1, $1, $2, 'individual', 'd.jsonl')",
        )
        .bind(&account)
        .bind(peer)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (1, $1, 'Sam')",
        )
        .bind(peer)
        .execute(&mut *conn)
        .await
        .unwrap();
        for (body, ts) in [
            ("hi", "2024-06-01T12:00:00Z"),
            ("there", "2024-06-01T13:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO messages (
                    conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
                 ) VALUES (1, $1, 'imessage', $2, 0, 0, $3)",
            )
            .bind(&account)
            .bind(ts)
            .bind(body)
            .execute(&mut *conn)
            .await
            .unwrap();
        }

        // Group conversation that includes Sam, with 1 message.
        let group_chat = account_profile::link_account_handle(
            &mut conn,
            &account,
            "chat-sam-group",
            HandleType::Other,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, group_title, source_file
             ) VALUES (2, $1, $2, 'group', 'Sam Group', 'g.jsonl')",
        )
        .bind(&account)
        .bind(group_chat)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (2, $1, 'Sam')",
        )
        .bind(peer)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (2, $1, 'imessage', '2024-07-01T12:00:00Z', 0, 0, 'group hi')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        // Unrelated conversation should not be counted.
        let other = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550999",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (9, $1, $2, 'individual', 'other.jsonl')",
        )
        .bind(&account)
        .bind(other)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (9, $1, 'imessage', '2024-08-01T12:00:00Z', 0, 0, 'nope')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let detail = get_contact_detail(&mut conn, &account, contact_id)
            .await
            .unwrap()
            .expect("contact exists");
        assert_eq!(detail.name, "Sam");
        assert_eq!(detail.direct_conversations, 1);
        assert_eq!(detail.group_conversations, 1);
        assert_eq!(detail.total_messages, 3);
        assert_eq!(detail.handles.len(), 1);
        assert!(
            detail.handles[0].handle.contains("5555550200")
                || detail.handles[0].handle.contains("+15555550200"),
            "handle={:?}",
            detail.handles[0].handle
        );
        assert_eq!(detail.handles[0].individual_conversations, 1);
        assert_eq!(detail.handles[0].group_conversations, 1);
        assert_eq!(detail.handles[0].individual_message_count, 2);
        assert_eq!(detail.handles[0].group_message_count, 1);
    }

    #[tokio::test]
    async fn get_contact_summaries_counts_two_contacts_in_one_query() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        let sam_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let sam_handle = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550200",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(sam_handle)
        .bind(sam_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (1, $1, $2, 'individual', 'd.jsonl')",
        )
        .bind(&account)
        .bind(sam_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (1, $1, 'Sam')",
        )
        .bind(sam_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
        for (body, ts) in [
            ("hi", "2024-06-01T12:00:00Z"),
            ("there", "2024-06-01T13:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO messages (
                    conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
                 ) VALUES (1, $1, 'imessage', $2, 0, 0, $3)",
            )
            .bind(&account)
            .bind(ts)
            .bind(body)
            .execute(&mut *conn)
            .await
            .unwrap();
        }
        let group_chat = account_profile::link_account_handle(
            &mut conn,
            &account,
            "chat-sam-group",
            HandleType::Other,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, group_title, source_file
             ) VALUES (2, $1, $2, 'group', 'Sam Group', 'g.jsonl')",
        )
        .bind(&account)
        .bind(group_chat)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (2, $1, 'Sam')",
        )
        .bind(sam_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (2, $1, 'imessage', '2024-07-01T12:00:00Z', 0, 0, 'group hi')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let pat_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Pat') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let pat_handle = account_profile::link_account_handle(
            &mut conn,
            &account,
            "+15555550100",
            HandleType::Phone,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(&account)
        .bind(pat_handle)
        .bind(pat_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (3, $1, $2, 'individual', 'pat.jsonl')",
        )
        .bind(&account)
        .bind(pat_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES (3, $1, 'Pat')",
        )
        .bind(pat_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (3, $1, 'imessage', '2024-05-01T09:00:00Z', 0, 0, 'hey')",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let summaries = get_contact_summaries(&mut conn, &account, &[sam_id, pat_id, 99_999])
            .await
            .unwrap();
        assert_eq!(summaries.len(), 2);

        assert_eq!(summaries[0].id, sam_id);
        assert_eq!(summaries[0].name, "Sam");
        assert_eq!(summaries[0].individual_conversations, 1);
        assert_eq!(summaries[0].group_conversations, 1);
        assert_eq!(summaries[0].individual_message_count, 2);
        assert_eq!(summaries[0].group_message_count, 1);
        assert_eq!(
            summaries[0].start_date.as_deref(),
            Some("2024-06-01T12:00:00Z")
        );
        assert_eq!(
            summaries[0].end_date.as_deref(),
            Some("2024-07-01T12:00:00Z")
        );

        assert_eq!(summaries[1].id, pat_id);
        assert_eq!(summaries[1].name, "Pat");
        assert_eq!(summaries[1].individual_conversations, 1);
        assert_eq!(summaries[1].group_conversations, 0);
        assert_eq!(summaries[1].individual_message_count, 1);
        assert_eq!(summaries[1].group_message_count, 0);
        assert_eq!(
            summaries[1].start_date.as_deref(),
            Some("2024-05-01T09:00:00Z")
        );
        assert_eq!(
            summaries[1].end_date.as_deref(),
            Some("2024-05-01T09:00:00Z")
        );
    }

    #[tokio::test]
    async fn mutate_contact_add_update_remove_handle_and_rename() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: None,
                    add_handle: Some(ContactHandlePayload {
                        handle: "+15555550200".into(),
                        service: Some("phone".into()),
                    }),
                    update_handle: None,
                    remove_handle: None,
                },
            )
            .await
            .unwrap()
        );

        let detail = get_contact_detail(&mut conn, &account, contact_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.handles.len(), 1);
        assert!(detail.handles[0].handle.contains("5555550200"));

        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: Some("Samantha".into()),
                    add_handle: None,
                    update_handle: None,
                    remove_handle: None,
                },
            )
            .await
            .unwrap()
        );
        let renamed = get_contact_detail(&mut conn, &account, contact_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(renamed.name, "Samantha");

        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: None,
                    add_handle: None,
                    update_handle: Some(ContactUpdateHandlePayload {
                        previous_handle: detail.handles[0].handle.clone(),
                        handle: "sam@example.com".into(),
                        service: Some("email".into()),
                    }),
                    remove_handle: None,
                },
            )
            .await
            .unwrap()
        );
        let updated = get_contact_detail(&mut conn, &account, contact_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.handles.len(), 1);
        assert_eq!(updated.handles[0].handle, "sam@example.com");

        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: None,
                    add_handle: None,
                    update_handle: None,
                    remove_handle: Some(ContactRemoveHandlePayload {
                        handle: "sam@example.com".into(),
                        service: Some("phone".into()),
                    }),
                },
            )
            .await
            .unwrap()
        );
        let empty = get_contact_detail(&mut conn, &account, contact_id)
            .await
            .unwrap()
            .unwrap();
        assert!(empty.handles.is_empty());
    }

    #[tokio::test]
    async fn mutate_contact_rejects_trashed_contact() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let contact_id =
            insert_contact_with_handle(&mut conn, &account, "Trashed", "+15555550100").await;
        sqlx::query("INSERT INTO trashed_contacts (account_id, contact_id) VALUES ($1, $2)")
            .bind(&account)
            .bind(contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let changed = mutate_contact(
            &mut conn,
            &account,
            contact_id,
            &ContactMutationBody {
                name: Some("Changed".into()),
                add_handle: None,
                update_handle: None,
                remove_handle: None,
            },
        )
        .await
        .unwrap();

        assert!(!changed);
        let name: String = sqlx::query_scalar(
            "SELECT preferred_name FROM contacts WHERE id = $1 AND account_id = $2",
        )
        .bind(contact_id)
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(name, "Trashed");
    }

    async fn contact_last_modified(
        conn: &mut AnyConnection,
        account: &str,
        contact_id: i64,
    ) -> String {
        sqlx::query_scalar("SELECT last_modified FROM contacts WHERE id = $1 AND account_id = $2")
            .bind(contact_id)
            .bind(account)
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    }

    async fn set_contact_last_modified(
        conn: &mut AnyConnection,
        account: &str,
        contact_id: i64,
        value: &str,
    ) {
        sqlx::query("UPDATE contacts SET last_modified = $1 WHERE id = $2 AND account_id = $3")
            .bind(value)
            .bind(contact_id)
            .bind(account)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn mutate_contact_bumps_last_modified_on_shape_changes() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Sam') RETURNING id",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let detail = get_contact_detail(&mut conn, &account, contact_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!detail.last_modified.is_empty());
        let page = list_contacts(&mut conn, &account, "", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(page.contacts[0].last_modified, detail.last_modified);

        const OLD: &str = "2000-01-01 00:00:00";
        set_contact_last_modified(&mut conn, &account, contact_id, OLD).await;
        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: Some("Samantha".into()),
                    add_handle: None,
                    update_handle: None,
                    remove_handle: None,
                },
            )
            .await
            .unwrap()
        );
        let after_rename = contact_last_modified(&mut conn, &account, contact_id).await;
        assert_ne!(after_rename, OLD);

        set_contact_last_modified(&mut conn, &account, contact_id, OLD).await;
        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: None,
                    add_handle: Some(ContactHandlePayload {
                        handle: "+15555550200".into(),
                        service: Some("phone".into()),
                    }),
                    update_handle: None,
                    remove_handle: None,
                },
            )
            .await
            .unwrap()
        );
        let after_add = contact_last_modified(&mut conn, &account, contact_id).await;
        assert_ne!(after_add, OLD);

        // Re-adding the same handle is a no-op and must not bump.
        set_contact_last_modified(&mut conn, &account, contact_id, OLD).await;
        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: None,
                    add_handle: Some(ContactHandlePayload {
                        handle: "+15555550200".into(),
                        service: Some("phone".into()),
                    }),
                    update_handle: None,
                    remove_handle: None,
                },
            )
            .await
            .unwrap()
        );
        assert_eq!(
            contact_last_modified(&mut conn, &account, contact_id).await,
            OLD
        );

        set_contact_last_modified(&mut conn, &account, contact_id, OLD).await;
        assert!(
            mutate_contact(
                &mut conn,
                &account,
                contact_id,
                &ContactMutationBody {
                    name: None,
                    add_handle: None,
                    update_handle: None,
                    remove_handle: Some(ContactRemoveHandlePayload {
                        handle: "+15555550200".into(),
                        service: Some("phone".into()),
                    }),
                },
            )
            .await
            .unwrap()
        );
        assert_ne!(
            contact_last_modified(&mut conn, &account, contact_id).await,
            OLD
        );
    }

    async fn insert_contact_with_handle(
        conn: &mut AnyConnection,
        account: &str,
        name: &str,
        phone: &str,
    ) -> i64 {
        // Schema requires preferred_name NOT NULL; empty string = no display name.
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(account)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let handle_id =
            account_profile::link_account_handle(conn, account, phone, HandleType::Phone)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(account)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        contact_id
    }

    async fn insert_direct_conversation(
        conn: &mut AnyConnection,
        account: &str,
        conversation_id: i64,
        phone: &str,
        service: &str,
        timestamps: &[&str],
    ) {
        let handle_id: i64 = match sqlx::query_scalar::<_, i64>(
            "SELECT id FROM handles WHERE account_id = $1 AND (raw = $2 OR normalized = $2) LIMIT 1",
        )
        .bind(account)
        .bind(phone)
        .fetch_optional(&mut *conn)
        .await
        .unwrap()
        {
            Some(id) => id,
            None => {
                account_profile::link_account_handle(conn, account, phone, HandleType::Phone)
                    .await
                    .unwrap()
            }
        };
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES ($1, $2, $3, 'individual', 't.jsonl')",
        )
        .bind(conversation_id)
        .bind(account)
        .bind(handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES ($1, $2, NULL)",
        )
        .bind(conversation_id)
        .bind(handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        for (i, ts) in timestamps.iter().enumerate() {
            sqlx::query(
                "INSERT INTO messages (
                    conversation_id, account_id, source, service, timestamp, is_from_me, sort_order, body
                 ) VALUES ($1, $2, $3, $3, $4, 0, $5, 'hi')",
            )
            .bind(conversation_id)
            .bind(account)
            .bind(service)
            .bind(ts)
            .bind(i as i64)
            .execute(&mut *conn)
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn list_contacts_filters_has_messages_and_never_messaged() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        insert_contact_with_handle(&mut conn, &account, "Messaged", "+15555550100").await;
        insert_contact_with_handle(&mut conn, &account, "Silent", "+15555550200").await;
        insert_direct_conversation(
            &mut conn,
            &account,
            1,
            "+15555550100",
            "imessage",
            &["2024-06-01T12:00:00Z"],
        )
        .await;

        let with_msg = list_contacts(
            &mut conn,
            &account,
            "has:messages search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(with_msg.total, 1);
        assert_eq!(with_msg.contacts[0].name, "Messaged");

        let never = list_contacts(
            &mut conn,
            &account,
            "has:no-messages search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(never.total, 1);
        assert_eq!(never.contacts[0].name, "Silent");
    }

    #[tokio::test]
    async fn list_contacts_filters_no_preferred_name() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        insert_contact_with_handle(&mut conn, &account, "Pat", "+15555550100").await;
        insert_contact_with_handle(&mut conn, &account, "", "+15555550200").await;
        insert_contact_with_handle(&mut conn, &account, "+15555550300", "+15555550300").await;

        let page = list_contacts(
            &mut conn,
            &account,
            "has:no-name search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(page.total, 2);
        let names: Vec<_> = page.contacts.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"(unknown)"));
        assert!(names.iter().any(|n| n.contains("5555550300")));
    }

    #[tokio::test]
    async fn list_contacts_filters_no_handle() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        insert_contact_with_handle(&mut conn, &account, "WithHandle", "+15555550100").await;
        sqlx::query("INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2)")
            .bind(&account)
            .bind("Orphan")
            .execute(&mut *conn)
            .await
            .unwrap();

        let page = list_contacts(
            &mut conn,
            &account,
            "has:no-handle search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.contacts[0].name, "Orphan");
        assert_eq!(page.contacts[0].handle_count, 0);
    }

    #[tokio::test]
    async fn list_contacts_filters_service_or() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        insert_contact_with_handle(&mut conn, &account, "IMsg", "+15555550100").await;
        insert_contact_with_handle(&mut conn, &account, "Sms", "+15555550200").await;
        insert_contact_with_handle(&mut conn, &account, "Wa", "+15555550300").await;
        insert_direct_conversation(
            &mut conn,
            &account,
            1,
            "+15555550100",
            "iMessage",
            &["2024-06-01T12:00:00Z"],
        )
        .await;
        insert_direct_conversation(
            &mut conn,
            &account,
            2,
            "+15555550200",
            "sms",
            &["2024-06-01T12:00:00Z"],
        )
        .await;
        insert_direct_conversation(
            &mut conn,
            &account,
            3,
            "+15555550300",
            "whatsapp",
            &["2024-06-01T12:00:00Z"],
        )
        .await;

        let page = list_contacts(
            &mut conn,
            &account,
            "service:phone search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(page.total, 2);
        let names: Vec<_> = page.contacts.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"IMsg"));
        assert!(names.contains(&"Sms"));
    }

    #[tokio::test]
    async fn list_contacts_filters_first_and_last_contact_dates() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        insert_contact_with_handle(&mut conn, &account, "Early", "+15555550100").await;
        insert_contact_with_handle(&mut conn, &account, "Late", "+15555550200").await;
        insert_direct_conversation(
            &mut conn,
            &account,
            1,
            "+15555550100",
            "imessage",
            &["2020-01-15T12:00:00Z", "2020-02-01T12:00:00Z"],
        )
        .await;
        insert_direct_conversation(
            &mut conn,
            &account,
            2,
            "+15555550200",
            "imessage",
            &["2024-06-01T12:00:00Z", "2024-08-01T12:00:00Z"],
        )
        .await;

        // Bare first-contact = on or after (back-compat).
        let first = list_contacts(
            &mut conn,
            &account,
            "first-contact:2024-01-01 search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(first.total, 1);
        assert_eq!(first.contacts[0].name, "Late");

        // Prefixed >= matches bare first semantics.
        let first_ge = list_contacts(
            &mut conn,
            &account,
            "first-contact:>=2024-01-01 search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(first_ge.total, 1);
        assert_eq!(first_ge.contacts[0].name, "Late");

        // Bare last-contact = on or before (back-compat).
        let last = list_contacts(
            &mut conn,
            &account,
            "last-contact:2020-12-31 search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(last.total, 1);
        assert_eq!(last.contacts[0].name, "Early");

        // Before: earliest message strictly before 2024-01-01 → Early only.
        let first_before = list_contacts(
            &mut conn,
            &account,
            "first-contact:<2024-01-01 search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(first_before.total, 1);
        assert_eq!(first_before.contacts[0].name, "Early");

        // Between on first message: >=2024-01-01 and <2025-01-01 → Late.
        let between = list_contacts(
            &mut conn,
            &account,
            "first-contact:>=2024-01-01 first-contact:<2025-01-01 search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(between.total, 1);
        assert_eq!(between.contacts[0].name, "Late");

        // Last message on or after mid-2024 → Late (MAX 2024-08-01).
        let last_ge = list_contacts(
            &mut conn,
            &account,
            "last-contact:>=2024-07-01 search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(last_ge.total, 1);
        assert_eq!(last_ge.contacts[0].name, "Late");

        // Last message before 2024-01-01 → Early.
        let last_before = list_contacts(
            &mut conn,
            &account,
            "last-contact:<2024-01-01 search:contacts",
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(last_before.total, 1);
        assert_eq!(last_before.contacts[0].name, "Early");
    }

    #[test]
    fn parse_date_bound_value_prefixes() {
        assert_eq!(
            parse_date_bound_value(">=2024-01-15", DateBoundOp::OnOrAfter),
            Some(DateBound {
                op: DateBoundOp::OnOrAfter,
                ymd: "2024-01-15".into(),
            })
        );
        assert_eq!(
            parse_date_bound_value("<2024-01-15", DateBoundOp::OnOrBefore),
            Some(DateBound {
                op: DateBoundOp::Before,
                ymd: "2024-01-15".into(),
            })
        );
        assert_eq!(
            parse_date_bound_value("2024-01-15", DateBoundOp::OnOrBefore),
            Some(DateBound {
                op: DateBoundOp::OnOrBefore,
                ymd: "2024-01-15".into(),
            })
        );
        assert!(parse_date_bound_value("<=2024-01-15", DateBoundOp::OnOrAfter).is_none());
    }

    #[tokio::test]
    async fn list_contacts_filters_by_group_and_no_group() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let family = insert_contact_with_handle(&mut conn, &account, "Ada", "+15555550100").await;
        insert_contact_with_handle(&mut conn, &account, "Ben", "+15555550200").await;
        crate::contact_groups_api::set_contacts_group_membership(
            &mut conn,
            &account,
            &[family],
            "Family",
            true,
        )
        .await
        .unwrap();

        let grouped = list_contacts(&mut conn, &account, "group:Family", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(grouped.total, 1);
        assert_eq!(grouped.contacts[0].name, "Ada");
        assert_eq!(grouped.contacts[0].groups, vec!["Family".to_string()]);

        let quoted = list_contacts(
            &mut conn,
            &account,
            r#"group:"Family""#,
            DEFAULT_LIST_LIMIT,
            0,
        )
        .await
        .unwrap();
        assert_eq!(quoted.total, 1);

        let none = list_contacts(&mut conn, &account, "group:none", DEFAULT_LIST_LIMIT, 0)
            .await
            .unwrap();
        assert_eq!(none.total, 1);
        assert_eq!(none.contacts[0].name, "Ben");
        assert!(none.contacts[0].groups.is_empty());
    }
}
