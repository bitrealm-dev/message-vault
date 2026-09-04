//! Contact list/detail used by `GET /v1/contacts`,
//! `GET /v1/contacts/{id}` and `PATCH /v1/contacts/{id}`, `POST /v1/contacts/summaries`,
//! and `POST /v1/contacts/match`.

use std::collections::{HashMap, HashSet};

use crate::extract::{Json, Path as AxumPath, Query};
use anyhow::{Result as AnyResult, bail};
use axum::extract::State;
use axum::http::StatusCode;
use message_ir::HandleType;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;

use crate::db::contacts::{self, contact_id_for_handle};
use crate::db::dialect::{engine_of, group_concat_unit_separator, order_by_name_ci};
use crate::db::handles::{infer_handle_type_from_shape, normalize_handle};
use crate::db::sql::{SqlParam, bind_args, in_placeholders, renumber_placeholders};
use crate::db::trash::{restore_contact, trash_contact};
use crate::paging::{
    DEFAULT_LIST_LIMIT, MAX_CONTACT_SUMMARY_IDS, MAX_LIST_OFFSET, Page, PageQuery, page_params,
};
use crate::server::{ApiError, AppState, FullAccess};

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

/// Body for `PATCH /v1/contacts/{id}`. Exactly one mutation field should be set.
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

/// Body for `POST /v1/contacts/summaries`.
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

/// Response for `POST /v1/contacts/summaries`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContactSummariesPage {
    /// One summary per requested contact.
    pub items: Vec<ContactSelectionSummary>,
}

/// A contact is linked to a conversation when one of its handles is either
/// the conversation's chat handle or a participant handle in it.
///
/// `contact_id_expr` is the SQL expression for the contact id (`$N` or `ct.id`).
fn involves_contact_expr(contact_id_expr: &str) -> String {
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
fn involves_contact_sql() -> String {
    involves_contact_expr("$2")
}

/// Conversation `c` is not in `trashed_conversations`.
const NOT_TRASHED_CONVERSATION_SQL: &str = "NOT EXISTS (
               SELECT 1 FROM trashed_conversations tc
               WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id
             )";

/// Contact `ct` is not in `trashed_contacts`.
const NOT_TRASHED_CONTACT_SQL: &str = "NOT EXISTS (
               SELECT 1 FROM trashed_contacts tct
               WHERE tct.account_id = ct.account_id AND tct.contact_id = ct.id
             )";

/// One page of the contact list for `q`, a query in the search language.
///
/// # Errors
///
/// `BadRequest` for a query the language refuses; `Internal` when a
/// statement fails.
pub async fn list_contacts(
    conn: &mut AnyConnection,
    account_id: &str,
    q: &str,
    limit: usize,
    offset: usize,
    today: chrono::NaiveDate,
) -> Result<Page<ContactSummary>, ApiError> {
    let engine = engine_of(conn);
    let filter = crate::search::compile(crate::search::CompileRequest {
        list: crate::search::ListKind::Contacts,
        query: q,
        account_id,
        engine,
        today,
    })?;
    let where_sql = filter.where_sql();

    let count_sql = renumber_placeholders(&format!(
        "SELECT COUNT(*) FROM contacts ct WHERE {where_sql}"
    ));
    let total: i64 = sqlx::query_scalar_with(&count_sql, bind_args(filter.params()))
        .fetch_one(&mut *conn)
        .await?;
    let total = total.max(0) as u64;

    let order_by = format!("{}, ct.id", order_by_name_ci(engine, "name"));
    let sql = renumber_placeholders(&format!(
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
         LIMIT ? OFFSET ?",
        handles_agg = group_concat_unit_separator(engine, "val"),
        groups_agg = group_concat_unit_separator(engine, "cl.name"),
    ));
    let mut params = filter.params().to_vec();
    params.push(SqlParam::Int(limit as i64));
    params.push(SqlParam::Int(offset as i64));
    let rows: Vec<ContactRow> = sqlx::query_as_with(&sql, bind_args(&params))
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

    Ok(Page {
        items: contacts,
        total,
        limit,
        offset,
    })
}

type ContactRow = (i64, String, i64, Option<String>, String, Option<String>);

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
) -> Result<Option<ContactDetail>, ApiError> {
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
             LEFT JOIN messages m ON m.conversation_id = c.id AND m.duplicate_of IS NULL
             WHERE ch.account_id = $1 AND ch.contact_id = $2
             GROUP BY ch.handle_id, h.raw, h.service
             ORDER BY h.raw",
        not_trashed_conversation = NOT_TRASHED_CONVERSATION_SQL,
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
                start_date,
                end_date,
                individual_conversations,
                group_conversations,
                individual_message_count,
                group_message_count,
            )| ContactHandleInfo {
                handle,
                service,
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
             )
             SELECT
               (SELECT COUNT(*) FROM involved WHERE conversation_type = 'individual'),
               (SELECT COUNT(*) FROM involved WHERE conversation_type = 'group'),
               (SELECT COUNT(*) FROM messages m
                WHERE m.duplicate_of IS NULL
                  AND m.conversation_id IN (SELECT id FROM involved))",
        involves_contact_sql = involves_contact_sql(),
        not_trashed_conversation = NOT_TRASHED_CONVERSATION_SQL,
    ))
    .bind(account_id)
    .bind(contact_id)
    .fetch_one(&mut *conn)
    .await?;

    let contact_groups = crate::named_membership::names_for_item(
        crate::named_membership::group_spec(),
        conn,
        account_id,
        contact_id,
    )
    .await?;

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
    i64,
    i64,
    i64,
    i64,
);

/// First/last seen and message counts for many contacts in one grouped query.
///
/// Unknown, trashed, and duplicate ids are skipped. At most
/// [`MAX_CONTACT_SUMMARY_IDS`] ids are read so the `IN` list stays under
/// SQLite's variable cap.
///
/// # Errors
///
/// Returns an internal error when a database statement fails.
pub async fn get_contact_summaries(
    conn: &mut AnyConnection,
    account_id: &str,
    ids: &[i64],
) -> Result<Vec<ContactSelectionSummary>, ApiError> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids.iter().copied() {
        if id <= 0 || !seen.insert(id) {
            continue;
        }
        unique.push(id);
        if unique.len() == MAX_CONTACT_SUMMARY_IDS {
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

/// Most identifiers one request to `POST /v1/contacts/match` may ask about.
///
/// A staged folder can reference thousands of participants; the client
/// batches. The query runs as a single statement with no chunking, so the
/// cap keeps its bind count bounded (501 identifiers would mean 502 binds);
/// raising it needs `SQLITE_IN_CHUNK`-style chunking first, not just a
/// bigger number.
pub(crate) const MAX_MATCH_IDENTIFIERS: usize = 500;

/// Body for `POST /v1/contacts/match`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ContactMatchBody {
    /// Raw identifiers — phone numbers, emails — as they appear in an export.
    identifiers: Vec<String>,
}

/// Response for `POST /v1/contacts/match`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ContactMatchResponse {
    /// The subset this account has no contact for: trimmed, in first-seen
    /// order, blanks dropped and duplicates (by normalized form) collapsed
    /// to their first spelling.
    unknown: Vec<String>,
}

/// Which of `identifiers` this account has no (non-trashed) contact for.
///
/// Matches on the same normalized form the import pipeline stores in
/// `handles.normalized` ([`normalize_handle`]), so an export spelling like
/// `+1 555 0100` is recognized against a vault contact stored as
/// `+15550100`. Blanks are dropped; duplicates are collapsed by *normalized*
/// form (two spellings of the same person must not both count as "new"),
/// keeping the first-seen raw (trimmed) spelling and first-seen order.
///
/// # Errors
///
/// Returns an error when a database statement fails.
async fn unknown_contact_identifiers(
    conn: &mut AnyConnection,
    account_id: &str,
    identifiers: &[String],
) -> AnyResult<Vec<String>> {
    let mut seen_normalized = HashSet::new();
    // (first-seen trimmed spelling, normalized form), one entry per distinct
    // normalized value.
    let mut unique: Vec<(String, String)> = Vec::new();
    for raw in identifiers {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Import prefers the handle type the source declared (SMS, email
        // header, ...); here there is no declared type, so this infers one
        // from the string's shape instead. The two can diverge: a
        // source-declared phone number whose digits don't look phone-shaped
        // (e.g. a short code) would infer as Other here and normalize
        // differently than the vault's stored (Phone-typed) form, reading as
        // "new" even though import would have linked it. Acceptable for a
        // best-effort gate count; not a source of silent data loss.
        let normalized = normalize_handle(trimmed, infer_handle_type_from_shape(trimmed)).0;
        if seen_normalized.insert(normalized.clone()) {
            unique.push((trimmed.to_string(), normalized));
        }
    }
    if unique.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = in_placeholders(2, unique.len());
    let sql = format!(
        "SELECT DISTINCT h.normalized
         FROM handles h
         JOIN contact_handles ch ON ch.account_id = h.account_id AND ch.handle_id = h.id
         JOIN contacts ct ON ct.account_id = ch.account_id AND ct.id = ch.contact_id
         WHERE h.account_id = $1
           AND h.normalized IN ({placeholders})
           AND {not_trashed}",
        not_trashed = NOT_TRASHED_CONTACT_SQL,
    );
    let mut q = sqlx::query_scalar::<_, String>(&sql).bind(account_id);
    for (_, normalized) in &unique {
        q = q.bind(normalized);
    }
    let known: HashSet<String> = q.fetch_all(&mut *conn).await?.into_iter().collect();

    Ok(unique
        .into_iter()
        .filter(|(_, norm)| !known.contains(norm))
        .map(|(raw, _)| raw)
        .collect())
}

/// Largest address book the load route accepts, in bytes.
///
/// A phone's contacts export is measured in tens of kilobytes; a few megabytes
/// is already far past any real address book, and the whole file is read into
/// memory before parsing.
pub(crate) const MAX_ADDRESS_BOOK_BYTES: usize = 8 * 1024 * 1024;

/// Body for `POST /v1/contacts/address-book`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct AddressBookBody {
    /// File name, used only to tell VCF from vCard CSV.
    filename: String,
    /// The file's text.
    content: String,
}

/// What loading an address book changed.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AddressBookLoadResponse {
    /// Contacts written from the file.
    pub contacts: u64,
    /// Phone identities linked to those contacts.
    pub phones: u64,
    /// Identities written with a review note (an ambiguous number).
    pub phones_needing_review: u64,
}

/// Load a VCF or vCard CSV address book into this account.
///
/// This is a standalone act against the vault, never part of an import run:
/// contacts are vault state, and a person may load them before or after
/// bringing messages in. Only the rows the address book owns are replaced, so
/// Contact Groups, names the person typed, and identities an import discovered
/// all survive.
#[utoipa::path(
    post,
    path = "/v1/contacts/address-book",
    tag = "Contacts",
    security(("bearer" = [])),
    request_body = AddressBookBody,
    responses(
        (status = 200, body = AddressBookLoadResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn address_book_load_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Json(body): Json<AddressBookBody>,
) -> Result<Json<AddressBookLoadResponse>, ApiError> {
    if body.content.len() > MAX_ADDRESS_BOOK_BYTES {
        return Err(ApiError::BadRequest(format!(
            "address book is larger than {} bytes",
            MAX_ADDRESS_BOOK_BYTES
        )));
    }
    if body.content.trim().is_empty() {
        return Err(ApiError::BadRequest("address book is empty".into()));
    }
    // The loader detects VCF versus vCard CSV from the path, so the upload is
    // written to a temp file under its own name rather than being sniffed twice.
    let dir =
        tempfile::tempdir().map_err(|e| ApiError::Internal(format!("create temp dir: {e}")))?;
    let name = sanitized_address_book_name(&body.filename);
    let path = dir.path().join(name);
    std::fs::write(&path, body.content.as_bytes())
        .map_err(|e| ApiError::Internal(format!("write address book: {e}")))?;

    let mut conn = state.db.acquire().await?;
    let stats = contacts::load_contacts_if_needed(&mut conn, Some(&path), true, &auth.account_id)
        .await
        .map_err(|e| ApiError::Internal(format!("load address book: {e}")))?;
    Ok(Json(AddressBookLoadResponse {
        contacts: stats.contacts,
        phones: stats.phones,
        phones_needing_review: stats.phones_needing_review,
    }))
}

/// A safe temp file name that keeps the extension the format detector reads.
///
/// The uploaded name never becomes a path: only its extension matters, and an
/// unrecognized one falls back to `.csv`, which is what the detector treats as
/// vCard CSV.
fn sanitized_address_book_name(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.ends_with(".vcf") || lower.ends_with(".vcard") {
        "address-book.vcf".to_string()
    } else {
        "address-book.csv".to_string()
    }
}

/// Report which identifiers this account has no vault contact for.
#[utoipa::path(
    post,
    path = "/v1/contacts/match",
    tag = "Contacts",
    security(("bearer" = [])),
    request_body = ContactMatchBody,
    responses(
        (status = 200, body = ContactMatchResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_match_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Json(body): Json<ContactMatchBody>,
) -> Result<Json<ContactMatchResponse>, ApiError> {
    if body.identifiers.len() > MAX_MATCH_IDENTIFIERS {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_MATCH_IDENTIFIERS} identifiers"
        )));
    }
    let mut conn = state.db.acquire().await?;
    let unknown =
        unknown_contact_identifiers(&mut conn, &auth.account_id, &body.identifiers).await?;
    Ok(Json(ContactMatchResponse { unknown }))
}

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
        // Typing a name in the drawer is the most deliberate naming act in the
        // product, so the row stops being the import's and becomes the
        // person's. That is what keeps a later address book from replacing
        // this name: an address book renames only `origin = 'import'` rows.
        sqlx::query(
            "UPDATE contacts SET preferred_name = $1, origin = 'user'
             WHERE id = $2 AND account_id = $3",
        )
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
        // The person attached this identity themselves, so a later address
        // book load leaves it alone.
        crate::db::contacts::link_handle_to_contact(
            conn,
            account_id,
            handle_id,
            contact_id,
            crate::db::contacts::Origin::User,
        )
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
    path = "/v1/contacts",
    tag = "Contacts",
    security(("bearer" = [])),
    params(
        ("q" = Option<String>, Query, description = "Contact search; empty lists all"),
        ("limit" = Option<usize>, Query, description = "Page size, default 40, max 500"),
        ("offset" = Option<usize>, Query, description = "Page offset, max 50000")
    ),
    responses(
        (status = 200, body = Page<ContactSummary>),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contacts_list_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<ContactSummary>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let q = query.q.unwrap_or_default();
    let page = page_params(
        query.limit,
        query.offset,
        DEFAULT_LIST_LIMIT,
        Some(MAX_LIST_OFFSET),
    )?;
    let result = list_contacts(
        &mut conn,
        &auth.account_id,
        &q,
        page.limit,
        page.offset,
        chrono::Local::now().date_naive(),
    )
    .await?;
    Ok(Json(result))
}

/// First/last message dates and counts for a list of contact ids.
#[utoipa::path(
    post,
    path = "/v1/contacts/summaries",
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
    FullAccess(auth): FullAccess,
    Json(body): Json<ContactSummariesBody>,
) -> Result<Json<ContactSummariesPage>, ApiError> {
    if body.ids.len() > MAX_CONTACT_SUMMARY_IDS {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_CONTACT_SUMMARY_IDS} contact ids"
        )));
    }
    let mut conn = state.db.acquire().await?;
    let page = get_contact_summaries(&mut conn, &auth.account_id, &body.ids)
        .await
        .map(|items| ContactSummariesPage { items })?;
    Ok(Json(page))
}

/// Full contact view: per-handle services, message stats, and group
/// memberships.
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}",
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
    FullAccess(auth): FullAccess,
    AxumPath(contact_id): AxumPath<i64>,
) -> Result<Json<ContactDetail>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let detail = get_contact_detail(&mut conn, &auth.account_id, contact_id).await?;
    detail
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("contact not found".into()))
}

/// Turn a contact edit's error into the HTTP failure a caller should see.
///
/// `mutate_contact` returns `anyhow` so that its validation messages ("handle
/// already linked to another contact") reach the person. A database error
/// is not something the person can fix by changing the request, so it is a
/// 500 with the cause on stderr rather than a 400 wearing sqlx's words.
fn classify_mutation_error(err: anyhow::Error) -> ApiError {
    if err.downcast_ref::<sqlx::Error>().is_some() {
        ApiError::Internal(format!("{err:#}"))
    } else {
        ApiError::BadRequest(err.to_string())
    }
}

/// Rename a contact or change its linked handles.
#[utoipa::path(
    patch,
    path = "/v1/contacts/{id}",
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
    FullAccess(auth): FullAccess,
    AxumPath(contact_id): AxumPath<i64>,
    Json(body): Json<ContactMutationBody>,
) -> Result<Json<ContactDetail>, ApiError> {
    let mut conn = state.db.acquire().await?;
    match mutate_contact(&mut conn, &auth.account_id, contact_id, &body).await {
        Ok(false) => Err(ApiError::NotFound("contact not found".into())),
        Err(e) => Err(classify_mutation_error(e)),
        Ok(true) => get_contact_detail(&mut conn, &auth.account_id, contact_id)
            .await?
            .ok_or_else(|| ApiError::Internal("contact missing after mutate".into()))
            .map(Json),
    }
}

/// Put a contact in the trash. Idempotent: trashing an already-trashed
/// contact still answers 204.
#[utoipa::path(
    post,
    path = "/v1/contacts/{id}/trash",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact id")),
    responses(
        (status = 204, description = "Trashed"),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_trash_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    AxumPath(contact_id): AxumPath<i64>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    if trash_contact(&mut conn, &auth.account_id, contact_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("contact not found".into()))
    }
}

/// Take a contact out of the trash. Idempotent: restoring a contact that
/// was not trashed still answers 204.
#[utoipa::path(
    post,
    path = "/v1/contacts/{id}/restore",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact id")),
    responses(
        (status = 204, description = "Restored"),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn contact_restore_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    AxumPath(contact_id): AxumPath<i64>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    if restore_contact(&mut conn, &auth.account_id, contact_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("contact not found".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::account_profile;
    use crate::test_support::{
        RegisteredAccount, TestVault, post_json, post_status, register_via_api, test_vault,
    };
    use axum::http::StatusCode;

    /// A vault, a signed-in account, and `handles` linked as contacts (one
    /// contact per phone, named `Contact 0`, `Contact 1`, ...).
    async fn contacts_fixture_with_handles(
        handles: &[&str],
    ) -> (TestVault, String, RegisteredAccount) {
        let vault = test_vault().await;
        let account = register_via_api(&vault.state, "alice", "hunter2hunter2").await;
        if !handles.is_empty() {
            let mut conn = vault.state.db.acquire().await.unwrap();
            for (i, handle) in handles.iter().enumerate() {
                insert_contact_with_handle(
                    &mut conn,
                    &account.account_id,
                    &format!("Contact {i}"),
                    handle,
                )
                .await;
            }
        }
        let token = account.token.clone();
        (vault, token, account)
    }

    /// A vault, a signed-in account, and one contact linked to `handle` that
    /// is then trashed.
    async fn contacts_fixture_with_trashed_handle(
        handle: &str,
    ) -> (TestVault, String, RegisteredAccount) {
        let vault = test_vault().await;
        let account = register_via_api(&vault.state, "alice", "hunter2hunter2").await;
        let mut conn = vault.state.db.acquire().await.unwrap();
        let contact_id =
            insert_contact_with_handle(&mut conn, &account.account_id, "Trashed", handle).await;
        sqlx::query("INSERT INTO trashed_contacts (account_id, contact_id) VALUES ($1, $2)")
            .bind(&account.account_id)
            .bind(contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let token = account.token.clone();
        (vault, token, account)
    }

    /// A second signed-in account in the same vault, with `handle` linked to
    /// one of its contacts. Used to prove `/v1/contacts/match` is scoped to
    /// the calling account rather than the whole vault database.
    async fn account_with_handle(vault: &TestVault, handle: &str) -> RegisteredAccount {
        let account = register_via_api(&vault.state, "bob", "hunter2hunter2").await;
        let mut conn = vault.state.db.acquire().await.unwrap();
        insert_contact_with_handle(&mut conn, &account.account_id, "Other", handle).await;
        account
    }

    #[tokio::test]
    async fn contact_match_reports_only_the_identifiers_the_vault_does_not_have() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let body = serde_json::json!({ "identifiers": ["+15550100", "+15550999"] });
        let response =
            post_json::<serde_json::Value>(&vault.state, "/v1/contacts/match", &token, body).await;
        assert_eq!(response["unknown"], serde_json::json!(["+15550999"]));
    }

    #[tokio::test]
    async fn contact_match_ignores_blank_identifiers_and_de_duplicates() {
        let (vault, token, _account) = contacts_fixture_with_handles(&[]).await;
        let body = serde_json::json!({ "identifiers": ["+15550999", "  ", "+15550999", ""] });
        let response =
            post_json::<serde_json::Value>(&vault.state, "/v1/contacts/match", &token, body).await;
        assert_eq!(response["unknown"], serde_json::json!(["+15550999"]));
    }

    #[tokio::test]
    async fn contact_match_collapses_duplicates_by_normalized_form() {
        // Two spellings of the same phone number must read as one new
        // person, not two — otherwise Gate 1's "N new to your vault" count
        // double-counts a single human written two ways.
        let (vault, token, _account) = contacts_fixture_with_handles(&[]).await;
        let body = serde_json::json!({ "identifiers": ["+1 (555) 010-0100", "+15550100100"] });
        let response =
            post_json::<serde_json::Value>(&vault.state, "/v1/contacts/match", &token, body).await;
        assert_eq!(
            response["unknown"],
            serde_json::json!(["+1 (555) 010-0100"]),
            "both spellings normalize to the same value, so only the \
             first-seen spelling should come back once"
        );
    }

    #[tokio::test]
    async fn contact_match_matches_a_differently_spelled_identifier_against_the_stored_normalized_value()
     {
        // Guards against a regression to matching on `h.raw`: the fixture
        // stores the E.164 form through the normal handle-linking path; the
        // request asks about a spaced-out spelling of the same number.
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let body = serde_json::json!({ "identifiers": ["+1 555 0100"] });
        let response =
            post_json::<serde_json::Value>(&vault.state, "/v1/contacts/match", &token, body).await;
        assert_eq!(
            response["unknown"],
            serde_json::json!([]),
            "the differently-spelled identifier normalizes to the stored value, so it is known"
        );
    }

    #[tokio::test]
    async fn contact_match_preserves_order_across_multiple_unknowns() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let body = serde_json::json!({ "identifiers": ["+15550100", "+15550200", "+15550300"] });
        let response =
            post_json::<serde_json::Value>(&vault.state, "/v1/contacts/match", &token, body).await;
        assert_eq!(
            response["unknown"],
            serde_json::json!(["+15550200", "+15550300"])
        );
    }

    #[tokio::test]
    async fn contact_match_does_not_count_a_trashed_contact_as_known() {
        // A trashed contact is not in the user's vault as far as every other
        // screen is concerned, and saying "you already have this person" about
        // someone they deleted would be a lie.
        let (vault, token, _account) = contacts_fixture_with_trashed_handle("+15550100").await;
        let body = serde_json::json!({ "identifiers": ["+15550100"] });
        let response =
            post_json::<serde_json::Value>(&vault.state, "/v1/contacts/match", &token, body).await;
        assert_eq!(response["unknown"], serde_json::json!(["+15550100"]));
    }

    #[tokio::test]
    async fn contact_match_is_scoped_to_the_calling_account() {
        let (vault, token, _mine) = contacts_fixture_with_handles(&[]).await;
        let _other = account_with_handle(&vault, "+15550100").await;
        let body = serde_json::json!({ "identifiers": ["+15550100"] });
        let response =
            post_json::<serde_json::Value>(&vault.state, "/v1/contacts/match", &token, body).await;
        assert_eq!(response["unknown"], serde_json::json!(["+15550100"]));
    }

    #[tokio::test]
    async fn contact_match_rejects_an_oversized_batch() {
        let (vault, token, _account) = contacts_fixture_with_handles(&[]).await;
        let identifiers: Vec<String> = (0..MAX_MATCH_IDENTIFIERS + 1)
            .map(|i| format!("+1555{i:06}"))
            .collect();
        let status = post_status(
            &vault.state,
            "/v1/contacts/match",
            &token,
            serde_json::json!({ "identifiers": identifiers }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_contacts_uses_preferred_name_and_handle_ids() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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

        let page = list_contacts(
            &mut conn,
            &account,
            "",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].name, "Pat");
        assert_eq!(page.items[0].handle_count, 1);
        assert!(
            page.items[0]
                .handles
                .iter()
                .any(|h| h.contains("5555550100") || h.contains("+15555550100")),
            "handles={:?}",
            page.items[0].handles
        );
    }

    #[tokio::test]
    async fn list_contacts_filters_and_paginates() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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

        let by_name = list_contacts(
            &mut conn,
            &account,
            "sam",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(by_name.total, 1);
        assert_eq!(by_name.items[0].name, "Sam");

        let by_handle = list_contacts(
            &mut conn,
            &account,
            "handle:5555550200",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(by_handle.total, 1);
        assert_eq!(by_handle.items[0].name, "Sam");

        let page0 = list_contacts(&mut conn, &account, "", 2, 0, crate::search::tests::today())
            .await
            .unwrap();
        assert_eq!(page0.total, 3);
        assert_eq!(page0.limit, 2);
        assert_eq!(page0.offset, 0);
        assert_eq!(page0.items.len(), 2);
        let page1 = list_contacts(&mut conn, &account, "", 2, 2, crate::search::tests::today())
            .await
            .unwrap();
        assert_eq!(page1.total, 3);
        assert_eq!(page1.offset, 2);
        assert_eq!(page1.items.len(), 1);
    }

    #[tokio::test]
    async fn get_contact_detail_counts_direct_group_and_messages() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;

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
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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
        let page = list_contacts(
            &mut conn,
            &account,
            "",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(page.items[0].last_modified, detail.last_modified);

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
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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
            "messages:>0",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(with_msg.total, 1);
        assert_eq!(with_msg.items[0].name, "Messaged");

        let never = list_contacts(
            &mut conn,
            &account,
            "messages:0",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(never.total, 1);
        assert_eq!(never.items[0].name, "Silent");
    }

    #[tokio::test]
    async fn list_contacts_filters_no_handle() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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
            "handle:none",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "Orphan");
        assert_eq!(page.items[0].handle_count, 0);
    }

    #[tokio::test]
    async fn list_contacts_filters_service_or() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
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
            "service:imessage,sms",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(page.total, 2);
        let names: Vec<_> = page.items.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"IMsg"));
        assert!(names.contains(&"Sms"));
    }

    #[test]
    fn address_book_upload_name_only_decides_the_format() {
        assert_eq!(
            sanitized_address_book_name("Contacts.vcf"),
            "address-book.vcf"
        );
        assert_eq!(
            sanitized_address_book_name("  contacts.VCARD "),
            "address-book.vcf"
        );
        assert_eq!(
            sanitized_address_book_name("export.csv"),
            "address-book.csv"
        );
        // A name that tries to escape the temp directory is never used as a path.
        assert_eq!(
            sanitized_address_book_name("../../etc/passwd"),
            "address-book.csv"
        );
    }

    #[tokio::test]
    async fn an_address_book_renames_a_contact_an_import_named() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let dir = vault.dir();
        let mut conn = vault.conn().await;

        // What an import leaves behind: a contact named by the backup, holding
        // the phone, marked as the import's.
        let discovered =
            insert_contact_with_handle(&mut conn, &account, "Bobby", "+15551234567").await;
        sqlx::query("UPDATE contacts SET origin = 'import' WHERE id = $1")
            .bind(discovered)
            .execute(&mut *conn)
            .await
            .unwrap();

        let book = dir.join("book.vcf");
        std::fs::write(
            &book,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Robert Smith\nN:Smith;Robert;;;\nTEL:+15551234567\nEND:VCARD\n",
        )
        .unwrap();
        contacts::load_contacts_if_needed(&mut conn, Some(&book), true, &account)
            .await
            .unwrap();

        let names: Vec<String> = sqlx::query_scalar(
            "SELECT preferred_name FROM contacts WHERE account_id = $1 ORDER BY preferred_name",
        )
        .bind(&account)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            names,
            vec!["Robert Smith".to_string()],
            "the book renames the imported contact instead of making a second one: {names:?}"
        );

        let name: String = sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
            .bind(discovered)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(name, "Robert Smith");

        // The identity stays the import's, so a later book that drops the card
        // does not take the person's messages' contact with it.
        let origin: String = sqlx::query_scalar("SELECT origin FROM contacts WHERE id = $1")
            .bind(discovered)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(origin, "import");
    }

    #[tokio::test]
    async fn a_nameless_card_does_not_blank_an_imported_name() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let dir = vault.dir();
        let mut conn = vault.conn().await;

        // An import already named this person; the book only lists their
        // number, nothing more.
        let discovered =
            insert_contact_with_handle(&mut conn, &account, "Bobby", "+15551234567").await;
        sqlx::query("UPDATE contacts SET origin = 'import' WHERE id = $1")
            .bind(discovered)
            .execute(&mut *conn)
            .await
            .unwrap();

        let book = dir.join("book.vcf");
        std::fs::write(
            &book,
            "BEGIN:VCARD\nVERSION:3.0\nTEL:+15551234567\nEND:VCARD\n",
        )
        .unwrap();
        contacts::load_contacts_if_needed(&mut conn, Some(&book), true, &account)
            .await
            .unwrap();

        // A card with no name has nothing to say about who this person is,
        // so it does not get to unname them.
        let name: String = sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
            .bind(discovered)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(name, "Bobby");

        let origin: String = sqlx::query_scalar("SELECT origin FROM contacts WHERE id = $1")
            .bind(discovered)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(origin, "import");
    }

    #[tokio::test]
    async fn an_address_book_does_not_rename_a_contact_the_person_typed() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let dir = vault.dir();
        let mut conn = vault.conn().await;

        // An import discovered this person and gave them the name that backup
        // used, holding the phone the book is about to load a card for.
        let hand_typed =
            insert_contact_with_handle(&mut conn, &account, "Bobby", "+15551234567").await;
        sqlx::query("UPDATE contacts SET origin = 'import' WHERE id = $1")
            .bind(hand_typed)
            .execute(&mut *conn)
            .await
            .unwrap();
        // The person is in a Contact Group they built by hand.
        crate::named_membership::set_membership(
            crate::named_membership::group_spec(),
            &mut conn,
            &account,
            &[hand_typed],
            "Family",
            true,
        )
        .await
        .unwrap();

        // Then the person renamed them in the drawer, the way a person does —
        // through the same route the web app calls. That, not raw SQL, is what
        // makes the row theirs.
        mutate_contact(
            &mut conn,
            &account,
            hand_typed,
            &ContactMutationBody {
                name: Some("My Friend Bob".to_string()),
                add_handle: None,
                update_handle: None,
                remove_handle: None,
            },
        )
        .await
        .unwrap();
        let origin: String = sqlx::query_scalar("SELECT origin FROM contacts WHERE id = $1")
            .bind(hand_typed)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(origin, "user", "naming someone makes the row the person's");

        let book = dir.join("book.vcf");
        std::fs::write(
            &book,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Robert Smith\nN:Smith;Robert;;;\nTEL:+15551234567\nEND:VCARD\n",
        )
        .unwrap();
        contacts::load_contacts_if_needed(&mut conn, Some(&book), true, &account)
            .await
            .unwrap();

        // The name the person typed survives untouched.
        let hand_typed_name: String =
            sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
                .bind(hand_typed)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(hand_typed_name, "My Friend Bob");

        // The card joins that person instead of standing a second contact
        // beside them. A second row would be the worse outcome: the phone is
        // already linked, so the new row would end up with no identity at all
        // and anything the card carried would land on it instead of on the
        // person.
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM contacts WHERE account_id = $1 ORDER BY id")
                .bind(&account)
                .fetch_all(&mut *conn)
                .await
                .unwrap();
        assert_eq!(
            ids,
            vec![hand_typed],
            "the card joins the person the vault already has: {ids:?}"
        );

        // They keep the identity that made them findable.
        let handles: Vec<String> = sqlx::query_scalar(
            "SELECT h.raw FROM contact_handles ch JOIN handles h ON h.id = ch.handle_id
             WHERE ch.account_id = $1 AND ch.contact_id = $2",
        )
        .bind(&account)
        .bind(hand_typed)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert_eq!(handles, vec!["+15551234567".to_string()]);

        // And the Contact Group still points at them, not at a stranded row.
        let members: Vec<i64> = sqlx::query_scalar(
            "SELECT gm.contact_id FROM contact_group_members gm
             JOIN contact_groups g ON g.id = gm.group_id
             WHERE g.account_id = $1 AND g.name = 'Family'",
        )
        .bind(&account)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert_eq!(members, vec![hand_typed]);
    }

    #[tokio::test]
    async fn loading_an_address_book_replaces_only_its_own_rows() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let dir = vault.dir();
        let mut conn = vault.conn().await;

        // An identity the vault learned from imported messages, and a Contact
        // Group the person built by hand.
        let discovered =
            insert_contact_with_handle(&mut conn, &account, "From Import", "+15555550999").await;
        sqlx::query("UPDATE contacts SET origin = 'import' WHERE id = $1")
            .bind(discovered)
            .execute(&mut *conn)
            .await
            .unwrap();
        crate::named_membership::set_membership(
            crate::named_membership::group_spec(),
            &mut conn,
            &account,
            &[discovered],
            "Family",
            true,
        )
        .await
        .unwrap();

        let book = dir.join("book.vcf");
        std::fs::write(
            &book,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\nN:Lovelace;Ada;;;\nTEL:+15551234567\nEND:VCARD\n",
        )
        .unwrap();
        contacts::load_contacts_if_needed(&mut conn, Some(&book), true, &account)
            .await
            .unwrap();

        // A second load of a book that dropped Ada removes her, because the
        // vault knows that row was the book's.
        let book2 = dir.join("book2.vcf");
        std::fs::write(
            &book2,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Grace Hopper\nN:Hopper;Grace;;;\nTEL:+15557654321\nEND:VCARD\n",
        )
        .unwrap();
        contacts::load_contacts_if_needed(&mut conn, Some(&book2), true, &account)
            .await
            .unwrap();

        let names: Vec<String> = sqlx::query_scalar(
            "SELECT preferred_name FROM contacts WHERE account_id = $1 ORDER BY preferred_name",
        )
        .bind(&account)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert!(
            names.contains(&"From Import".to_string()),
            "an import-discovered contact must survive a book reload: {names:?}"
        );
        assert!(
            names.contains(&"Grace Hopper".to_string()),
            "the new book's contact must be present: {names:?}"
        );
        assert!(
            !names.contains(&"Ada Lovelace".to_string()),
            "a contact the book dropped must go: {names:?}"
        );

        let groups: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM contact_groups WHERE account_id = $1 ORDER BY name",
        )
        .bind(&account)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            groups,
            vec!["Family".to_string()],
            "a Contact Group the person built must survive a book reload"
        );
    }

    #[tokio::test]
    async fn unknown_group_collects_contacts_missing_a_name_or_an_identity() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;

        // Knows who and how to reach them: not Unknown.
        insert_contact_with_handle(&mut conn, &account, "Ada", "+15555550100").await;
        // Has an identity, no preferred name: Unknown by the second clause.
        insert_contact_with_handle(&mut conn, &account, "", "+15555550200").await;
        // Has a name, no identity at all: Unknown by the first clause.
        crate::db::contacts::create_contact(
            &mut conn,
            &account,
            "Sarah",
            crate::db::contacts::Origin::Import,
        )
        .await
        .unwrap();

        let unknown = list_contacts(
            &mut conn,
            &account,
            "group:unknown",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(unknown.total, 2);
        let mut names: Vec<String> = unknown.items.iter().map(|c| c.name.clone()).collect();
        names.sort();
        // The list renders a nameless contact as "(unknown)".
        assert_eq!(names, vec!["(unknown)".to_string(), "Sarah".to_string()]);

        // Naming the nameless one takes it out of Unknown, because membership
        // is computed rather than stored.
        sqlx::query("UPDATE contacts SET preferred_name = 'Ben' WHERE account_id = $1 AND trim(preferred_name) = ''")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        let after = list_contacts(
            &mut conn,
            &account,
            "group:unknown",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(after.total, 1);
        assert_eq!(after.items[0].name, "Sarah");
    }

    #[tokio::test]
    async fn list_contacts_filters_by_group_and_no_group() {
        let vault = test_vault().await;
        let account = vault
            .account_with_id("00000000-0000-4000-8000-0000000000c1", "alice")
            .await;
        let mut conn = vault.conn().await;
        let family = insert_contact_with_handle(&mut conn, &account, "Ada", "+15555550100").await;
        insert_contact_with_handle(&mut conn, &account, "Ben", "+15555550200").await;
        crate::named_membership::set_membership(
            crate::named_membership::group_spec(),
            &mut conn,
            &account,
            &[family],
            "Family",
            true,
        )
        .await
        .unwrap();

        let grouped = list_contacts(
            &mut conn,
            &account,
            "group:Family",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(grouped.total, 1);
        assert_eq!(grouped.items[0].name, "Ada");
        assert_eq!(grouped.items[0].groups, vec!["Family".to_string()]);

        let quoted = list_contacts(
            &mut conn,
            &account,
            r#"group:"Family""#,
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(quoted.total, 1);

        let none = list_contacts(
            &mut conn,
            &account,
            "group:none",
            DEFAULT_LIST_LIMIT,
            0,
            crate::search::tests::today(),
        )
        .await
        .unwrap();
        assert_eq!(none.total, 1);
        assert_eq!(none.items[0].name, "Ben");
        assert!(none.items[0].groups.is_empty());
    }

    #[tokio::test]
    async fn contact_list_takes_the_search_language() {
        let (vault, token, account) =
            contacts_fixture_with_handles(&["+15550100", "+15550101"]).await;
        {
            let mut conn = vault.state.db.acquire().await.unwrap();
            let group_id: i64 = sqlx::query_scalar(
                "INSERT INTO contact_groups (account_id, name) VALUES ($1, 'Family') RETURNING id",
            )
            .bind(&account.account_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            let first: i64 =
                sqlx::query_scalar("SELECT MIN(id) FROM contacts WHERE account_id = $1")
                    .bind(&account.account_id)
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap();
            sqlx::query("INSERT INTO contact_group_members (contact_id, group_id) VALUES ($1, $2)")
                .bind(first)
                .bind(group_id)
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        let page: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts?q=group:Family", &token)
                .await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["name"], "Contact 0");
        let page: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts?q=group:none", &token).await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["name"], "Contact 1");
    }

    #[tokio::test]
    async fn contact_list_refuses_a_word_from_another_list() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let status =
            crate::test_support::get_status(&vault.state, "/v1/contacts?q=from:me", &token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn a_database_error_while_editing_a_contact_is_internal() {
        let err = anyhow::Error::from(sqlx::Error::PoolClosed).context("update contact");
        match super::classify_mutation_error(err) {
            crate::server::ApiError::Internal(_) => {}
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn a_validation_error_while_editing_a_contact_is_bad_request() {
        let err = anyhow::anyhow!("handle already linked to another contact");
        match super::classify_mutation_error(err) {
            crate::server::ApiError::BadRequest(m) => {
                assert_eq!(m, "handle already linked to another contact");
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_contact_list_is_a_page_and_summaries_are_items() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

        let page: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/contacts?limit=5", &user.token).await;
        assert_eq!(page["total"], 0);
        assert_eq!(page["limit"], 5);
        assert!(page["items"].is_array());
        assert!(page.get("contacts").is_none());

        let status =
            crate::test_support::get_status(&state, "/v1/contacts?limit=501", &user.token).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);

        let summaries: serde_json::Value = crate::test_support::post_json(
            &state,
            "/v1/contacts/summaries",
            &user.token,
            serde_json::json!({ "ids": [] }),
        )
        .await;
        assert!(summaries["items"].is_array());
        assert!(summaries.get("contacts").is_none());
    }

    async fn trashed_contact_row_count(conn: &mut AnyConnection, account_id: &str, id: i64) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM trashed_contacts WHERE account_id = $1 AND contact_id = $2",
        )
        .bind(account_id)
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn contact_trash_drops_it_from_the_list() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let status = crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{id}/trash"),
            &token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT);

        let list_after: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        assert_eq!(
            list_after["total"], 0,
            "a trashed contact must leave the contacts list"
        );
    }

    #[tokio::test]
    async fn contact_trash_twice_is_204_with_no_second_marker() {
        let (vault, token, account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();
        let path = format!("/v1/contacts/{id}/trash");

        for _ in 0..2 {
            let status = crate::test_support::post_status(
                &vault.state,
                &path,
                &token,
                serde_json::json!({}),
            )
            .await;
            assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
        }

        let mut conn = vault.state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_contact_row_count(&mut conn, &account.account_id, id).await,
            1,
            "trashing twice must not create a second marker row"
        );
    }

    #[tokio::test]
    async fn contact_restore_brings_it_back_to_the_list() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();
        crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{id}/trash"),
            &token,
            serde_json::json!({}),
        )
        .await;

        let status = crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{id}/restore"),
            &token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT);

        let list_after: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        assert_eq!(
            list_after["total"], 1,
            "a restored contact must come back to the contacts list"
        );
    }

    #[tokio::test]
    async fn contact_restore_twice_is_204_with_marker_gone() {
        let (vault, token, account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();
        crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{id}/trash"),
            &token,
            serde_json::json!({}),
        )
        .await;
        let path = format!("/v1/contacts/{id}/restore");

        for _ in 0..2 {
            let status = crate::test_support::post_status(
                &vault.state,
                &path,
                &token,
                serde_json::json!({}),
            )
            .await;
            assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
        }

        let mut conn = vault.state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_contact_row_count(&mut conn, &account.account_id, id).await,
            0,
            "restoring twice must leave no marker row"
        );
    }

    #[tokio::test]
    async fn contact_trash_404s_for_an_unknown_id() {
        let (vault, token, _account) = contacts_fixture_with_handles(&[]).await;

        let status = crate::test_support::post_status(
            &vault.state,
            "/v1/contacts/999999/trash",
            &token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn contact_restore_404s_for_an_unknown_id() {
        let (vault, token, _account) = contacts_fixture_with_handles(&[]).await;

        let status = crate::test_support::post_status(
            &vault.state,
            "/v1/contacts/999999/restore",
            &token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn contact_trash_404s_for_another_accounts_contact() {
        let (vault, alice_token, alice) = contacts_fixture_with_handles(&["+15550100"]).await;
        let alice_list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &alice_token).await;
        let alice_contact_id = alice_list["items"][0]["id"].as_i64().unwrap();

        let bob =
            crate::test_support::register_via_api(&vault.state, "bob", "hunter2hunter2").await;

        // Bob trashing Alice's contact id must 404, not 403 — a 403 would
        // confirm the id exists in someone else's vault.
        let status = crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{alice_contact_id}/trash"),
            &bob.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

        let mut conn = vault.state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_contact_row_count(&mut conn, &alice.account_id, alice_contact_id).await,
            0,
            "Bob's request must not trash Alice's contact"
        );
    }

    #[tokio::test]
    async fn contact_restore_404s_for_another_accounts_contact() {
        let (vault, alice_token, alice) = contacts_fixture_with_handles(&["+15550100"]).await;
        let alice_list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &alice_token).await;
        let alice_contact_id = alice_list["items"][0]["id"].as_i64().unwrap();
        let mut conn = vault.state.db.acquire().await.unwrap();
        sqlx::query("INSERT INTO trashed_contacts (account_id, contact_id) VALUES ($1, $2)")
            .bind(&alice.account_id)
            .bind(alice_contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        let bob =
            crate::test_support::register_via_api(&vault.state, "bob", "hunter2hunter2").await;

        let status = crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{alice_contact_id}/restore"),
            &bob.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

        let mut conn = vault.state.db.acquire().await.unwrap();
        assert_eq!(
            trashed_contact_row_count(&mut conn, &alice.account_id, alice_contact_id).await,
            1,
            "Bob's request must not restore Alice's contact"
        );
    }

    #[tokio::test]
    async fn contact_trash_requires_auth() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let status = crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{id}/trash"),
            "not-a-token",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn contact_restore_requires_auth() {
        let (vault, token, _account) = contacts_fixture_with_handles(&["+15550100"]).await;
        let list: serde_json::Value =
            crate::test_support::get_json(&vault.state, "/v1/contacts", &token).await;
        let id = list["items"][0]["id"].as_i64().unwrap();

        let status = crate::test_support::post_status(
            &vault.state,
            &format!("/v1/contacts/{id}/restore"),
            "not-a-token",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    }

    /// The conversations list refuses an offset past `MAX_LIST_OFFSET`
    /// (conversations_api.rs). The contacts list shares `page_params` and must
    /// answer the same way over HTTP.
    #[tokio::test]
    async fn the_contacts_route_refuses_an_offset_past_the_ceiling() {
        let vault = crate::test_support::test_vault().await;
        let user =
            crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;

        let (status, text) =
            crate::test_support::get_raw(&vault.state, "/v1/contacts?offset=50001", &user.token)
                .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{text}");
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(body["error"].is_string(), "{body}");

        let ok =
            crate::test_support::get_status(&vault.state, "/v1/contacts?offset=50000", &user.token)
                .await;
        assert_eq!(
            ok,
            axum::http::StatusCode::OK,
            "the ceiling itself is allowed"
        );
    }
}
