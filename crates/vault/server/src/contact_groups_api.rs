//! Contact Groups over HTTP: one handler per route, each a call into
//! [`crate::named_set_api`] with [`group_spec`].

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::named_membership::group_spec;
use crate::named_set_api::{
    self, MemberIdList, MembersChanged, MembersPatch, NamedSet, NamedSetBody, NamedSetList,
};
use crate::server::{ApiError, AppState, ErrorBody, FullAccess};

/// The account's Contact Groups, A–Z.
#[utoipa::path(
    get,
    path = "/v1/contact-groups",
    tag = "Contacts",
    security(("bearer" = [])),
    responses(
        (status = 200, body = NamedSetList),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody)
    )
)]
pub(crate) async fn contact_groups_list(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
) -> Result<Json<NamedSetList>, ApiError> {
    named_set_api::list(group_spec(), &state, &auth.account_id).await
}

/// Create a Contact Group.
#[utoipa::path(
    post,
    path = "/v1/contact-groups",
    tag = "Contacts",
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
pub(crate) async fn contact_groups_create(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Json(body): Json<NamedSetBody>,
) -> Result<Json<NamedSet>, ApiError> {
    named_set_api::create(group_spec(), &state, &auth.account_id, body).await
}

/// Rename a Contact Group.
#[utoipa::path(
    patch,
    path = "/v1/contact-groups/{id}",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
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
pub(crate) async fn contact_groups_update(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<NamedSetBody>,
) -> Result<Json<NamedSet>, ApiError> {
    named_set_api::update(group_spec(), &state, &auth.account_id, id, body).await
}

/// Delete a Contact Group and its memberships.
#[utoipa::path(
    delete,
    path = "/v1/contact-groups/{id}",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
    responses(
        (status = 204),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn contact_groups_delete(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    named_set_api::delete(group_spec(), &state, &auth.account_id, id).await
}

/// Contact ids in one Contact Group.
#[utoipa::path(
    get,
    path = "/v1/contact-groups/{id}/members",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
    responses(
        (status = 200, body = MemberIdList),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn contact_group_members_list(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<Json<MemberIdList>, ApiError> {
    named_set_api::members_list(group_spec(), &state, &auth.account_id, id).await
}

/// Put contacts in and take contacts out of one Contact Group.
#[utoipa::path(
    patch,
    path = "/v1/contact-groups/{id}/members",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
    request_body = MembersPatch,
    responses(
        (status = 200, body = MembersChanged),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn contact_group_members_update(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<MembersPatch>,
) -> Result<Json<MembersChanged>, ApiError> {
    named_set_api::members_update(group_spec(), &state, &auth.account_id, id, body).await
}
