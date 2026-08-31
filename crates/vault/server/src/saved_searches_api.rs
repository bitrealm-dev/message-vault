//! Saved searches stored in `saved_searches`.
//!
//! Rows are addressed by id in the path rather than by name in the body, which
//! is how contact groups and message tags work. A saved search carries a name
//! and a query that are edited together, so name-addressing would use the
//! changing field as the key.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};

use crate::db::saved_searches::{self, SavedSearch, SavedSearchKind};
use crate::server::{ApiError, AppState, FullAccess};

/// A saved search's name and query.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct SavedSearchBody {
    name: String,
    query: String,
}

/// The account's saved searches, A–Z.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct SavedSearchesListResponse {
    #[serde(rename = "savedSearches")]
    saved_searches: Vec<SavedSearch>,
}

/// The affected saved search plus the updated list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct SavedSearchResponse {
    #[serde(rename = "savedSearch")]
    saved_search: SavedSearch,
    #[serde(rename = "savedSearches")]
    saved_searches: Vec<SavedSearch>,
}

/// The updated list after deletion.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct SavedSearchDeleteResponse {
    ok: bool,
    #[serde(rename = "savedSearches")]
    saved_searches: Vec<SavedSearch>,
}

/// List the account's saved searches, A–Z.
#[utoipa::path(
    get,
    path = "/v1/saved-searches",
    tag = "Saved searches",
    security(("bearer" = [])),
    responses(
        (status = 200, body = SavedSearchesListResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_list_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
) -> Result<Json<SavedSearchesListResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let saved_searches = saved_searches::list(&mut conn, &auth.account_id).await?;
    Ok(Json(SavedSearchesListResponse { saved_searches }))
}

/// Create a saved search and return it with the updated list.
#[utoipa::path(
    post,
    path = "/v1/saved-searches",
    tag = "Saved searches",
    security(("bearer" = [])),
    request_body = SavedSearchBody,
    responses(
        (status = 200, body = SavedSearchResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_create_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Json(body): Json<SavedSearchBody>,
) -> Result<Json<SavedSearchResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let saved_search = saved_searches::create(
        &mut conn,
        &auth.account_id,
        &body.name,
        &body.query,
        SavedSearchKind::Manual,
    )
    .await?;
    let saved_searches = saved_searches::list(&mut conn, &auth.account_id).await?;
    Ok(Json(SavedSearchResponse {
        saved_search,
        saved_searches,
    }))
}

/// Replace a saved search's name and query, and return the updated list.
#[utoipa::path(
    patch,
    path = "/v1/saved-searches/{id}",
    tag = "Saved searches",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Saved search id")),
    request_body = SavedSearchBody,
    responses(
        (status = 200, body = SavedSearchResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_update_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<SavedSearchBody>,
) -> Result<Json<SavedSearchResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let saved_search =
        saved_searches::update(&mut conn, &auth.account_id, id, &body.name, &body.query).await?;
    let saved_searches = saved_searches::list(&mut conn, &auth.account_id).await?;
    Ok(Json(SavedSearchResponse {
        saved_search,
        saved_searches,
    }))
}

/// Delete a saved search and return the updated list.
///
/// Deleting an import-created saved search removes the shortcut only. The
/// `vault_imports` row it pointed at is the account's permanent record of that
/// run and is never touched here.
#[utoipa::path(
    delete,
    path = "/v1/saved-searches/{id}",
    tag = "Saved searches",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Saved search id")),
    responses(
        (status = 200, body = SavedSearchDeleteResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_delete_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<Json<SavedSearchDeleteResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    saved_searches::delete(&mut conn, &auth.account_id, id).await?;
    let saved_searches = saved_searches::list(&mut conn, &auth.account_id).await?;
    Ok(Json(SavedSearchDeleteResponse {
        ok: true,
        saved_searches,
    }))
}
