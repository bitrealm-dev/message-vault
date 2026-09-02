//! Message Tags over HTTP: one handler per route, each a call into
//! [`crate::named_set_api`] with [`tag_spec`].

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::named_membership::tag_spec;
use crate::named_set_api::{
    self, MemberIdList, MembersChanged, MembersPatch, NamedSet, NamedSetBody, NamedSetList,
};
use crate::server::{ApiError, AppState, ErrorBody, FullAccess};

/// The account's Message Tags, A–Z.
#[utoipa::path(
    get,
    path = "/v1/message-tags",
    tag = "Message tags",
    security(("bearer" = [])),
    responses(
        (status = 200, body = NamedSetList),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody)
    )
)]
pub(crate) async fn message_tags_list(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
) -> Result<Json<NamedSetList>, ApiError> {
    named_set_api::list(tag_spec(), &state, &auth.account_id).await
}

/// Create a Message Tag.
#[utoipa::path(
    post,
    path = "/v1/message-tags",
    tag = "Message tags",
    security(("bearer" = [])),
    request_body = NamedSetBody,
    responses(
        (status = 200, body = NamedSet),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody)
    )
)]
pub(crate) async fn message_tags_create(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Json(body): Json<NamedSetBody>,
) -> Result<Json<NamedSet>, ApiError> {
    named_set_api::create(tag_spec(), &state, &auth.account_id, body).await
}

/// Rename a Message Tag.
#[utoipa::path(
    patch,
    path = "/v1/message-tags/{id}",
    tag = "Message tags",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Message Tag id")),
    request_body = NamedSetBody,
    responses(
        (status = 200, body = NamedSet),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)
    )
)]
pub(crate) async fn message_tags_update(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<NamedSetBody>,
) -> Result<Json<NamedSet>, ApiError> {
    named_set_api::update(tag_spec(), &state, &auth.account_id, id, body).await
}

/// Delete a Message Tag and its memberships.
#[utoipa::path(
    delete,
    path = "/v1/message-tags/{id}",
    tag = "Message tags",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Message Tag id")),
    responses(
        (status = 204),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn message_tags_delete(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    named_set_api::delete(tag_spec(), &state, &auth.account_id, id).await
}

/// Conversation ids in one Message Tag.
#[utoipa::path(
    get,
    path = "/v1/message-tags/{id}/members",
    tag = "Message tags",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Message Tag id")),
    responses(
        (status = 200, body = MemberIdList),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn message_tag_members_list(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<Json<MemberIdList>, ApiError> {
    named_set_api::members_list(tag_spec(), &state, &auth.account_id, id).await
}

/// Put conversations in and take conversations out of one Message Tag.
#[utoipa::path(
    patch,
    path = "/v1/message-tags/{id}/members",
    tag = "Message tags",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Message Tag id")),
    request_body = MembersPatch,
    responses(
        (status = 200, body = MembersChanged),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn message_tag_members_update(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<MembersPatch>,
) -> Result<Json<MembersChanged>, ApiError> {
    named_set_api::members_update(tag_spec(), &state, &auth.account_id, id, body).await
}
