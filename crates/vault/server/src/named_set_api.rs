//! One HTTP surface for Contact Groups and Message Tags.
//!
//! Both are a named set the account owns plus a membership of contact or
//! conversation ids. The request and response types and the six operations
//! live here once, over [`MembershipSpec`]; the `named_set_routes!` macro
//! stamps out both collections' twelve route handlers from them, one
//! `#[utoipa::path]` function per route, because utoipa needs a concrete
//! function with literal strings to describe each route and cannot see
//! through a generic or a `concat!`. The two invocations below name every
//! path, so both collections' routes stay greppable here.

use crate::extract::Json;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::named_membership::{self, MembershipSpec};
use crate::server::{ApiError, AppState, ErrorBody, FullAccess};

/// One Contact Group or Message Tag: its id and name.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct NamedSet {
    pub(crate) id: i64,
    pub(crate) name: String,
}

/// The account's sets of one kind, A–Z.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct NamedSetList {
    pub(crate) items: Vec<NamedSet>,
}

/// A name to create, or the new name for an existing set.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct NamedSetBody {
    pub(crate) name: String,
}

/// Member ids of one set, ascending.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MemberIdList {
    pub(crate) items: Vec<i64>,
}

/// Members to put in and take out of one set, in one request.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MembersPatch {
    #[serde(default)]
    pub(crate) add: Vec<i64>,
    #[serde(default)]
    pub(crate) remove: Vec<i64>,
}

/// How many memberships a patch created and how many it removed.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MembersChanged {
    pub(crate) added: u64,
    pub(crate) removed: u64,
}

/// The account's sets of this kind, A–Z. Never fails on its own; errors here
/// come from the database connection (500).
pub(crate) async fn list(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
) -> Result<Json<NamedSetList>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let items = named_membership::list_sets(spec, &mut conn, account_id)
        .await?
        .into_iter()
        .map(|(id, name)| NamedSet { id, name })
        .collect();
    Ok(Json(NamedSetList { items }))
}

/// Create a set and answer its id and trimmed name. A blank or over-long
/// name, or a reserved name, answers 400; a name already taken (ignoring
/// case) answers 409.
pub(crate) async fn create(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    body: NamedSetBody,
) -> Result<Json<NamedSet>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let (id, name) = named_membership::create_set(spec, &mut conn, account_id, &body.name).await?;
    Ok(Json(NamedSet { id, name }))
}

/// Rename a set by id, answering its id and the new name. An unknown or
/// another account's id answers 404; a blank, over-long, or reserved name
/// answers 400; another set's name (ignoring case) answers 409.
pub(crate) async fn update(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
    body: NamedSetBody,
) -> Result<Json<NamedSet>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let name = named_membership::rename_set(spec, &mut conn, account_id, id, &body.name).await?;
    Ok(Json(NamedSet { id, name }))
}

/// Delete a set by id and its memberships, answering 204. An unknown or
/// another account's id answers 404.
pub(crate) async fn delete(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    named_membership::delete_set(spec, &mut conn, account_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Member ids of one set, ascending. An unknown or another account's id
/// answers 404.
pub(crate) async fn members_list(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
) -> Result<Json<MemberIdList>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let items = named_membership::list_member_ids_of(spec, &mut conn, account_id, id).await?;
    Ok(Json(MemberIdList { items }))
}

/// Add and remove members of one set in one call, answering how many
/// changed. An unknown or another account's set id, or an unknown member id
/// in `add` or `remove`, answers 404; an empty patch answers 400.
pub(crate) async fn members_update(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
    body: MembersPatch,
) -> Result<Json<MembersChanged>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let (added, removed) =
        named_membership::patch_members(spec, &mut conn, account_id, id, &body.add, &body.remove)
            .await?;
    Ok(Json(MembersChanged { added, removed }))
}

/// One collection's six HTTP handlers.
///
/// Contact Groups and Message Tags are the same six operations over
/// [`MembershipSpec`]; what differs is the paths, the tag, the noun in the
/// prose, and which spec function to call. utoipa needs a concrete function
/// per route with literal strings in its attribute — it cannot describe a
/// generic, and it cannot see through `concat!` — so the handlers are stamped
/// out here rather than written twice.
///
/// The function names and doc comments are load-bearing: `operationId` comes
/// from the name and `summary` from the first line of the doc, so both appear
/// in `docs/src/assets/openapi.json`.
///
/// Adding a third collection is this macro invoked a third time, plus a
/// `MembershipSpec` for it in `named_membership.rs` and six
/// `.routes(routes!(..))` lines in `openapi.rs` — `utoipa_axum` needs each
/// route named there and that cannot be folded in here.
///
/// The formatting is pinned because rustfmt reindents an attribute body
/// inside a macro arm to three times the depth of the code around it.
#[rustfmt::skip]
macro_rules! named_set_routes {
    (
        spec: $spec:path,
        tag: $tag:literal,
        id_description: $id_description:literal,
        root_path: $root_path:literal,
        id_path: $id_path:literal,
        members_path: $members_path:literal,
        list: $list_fn:ident, $list_doc:literal,
        create: $create_fn:ident, $create_doc:literal,
        update: $update_fn:ident, $update_doc:literal,
        delete: $delete_fn:ident, $delete_doc:literal,
        members_list: $members_list_fn:ident, $members_list_doc:literal,
        members_update: $members_update_fn:ident, $members_update_doc:literal,
    ) => {
        #[doc = $list_doc]
        #[utoipa::path(
            get,
            path = $root_path,
            tag = $tag,
            security(("bearer" = [])),
            responses(
                (status = 200, body = NamedSetList),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody)
            )
        )]
        pub(crate) async fn $list_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
        ) -> Result<Json<NamedSetList>, ApiError> {
            list($spec(), &state, &auth.account_id).await
        }

        #[doc = $create_doc]
        #[utoipa::path(
            post,
            path = $root_path,
            tag = $tag,
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
        pub(crate) async fn $create_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            Json(body): Json<NamedSetBody>,
        ) -> Result<Json<NamedSet>, ApiError> {
            create($spec(), &state, &auth.account_id, body).await
        }

        #[doc = $update_doc]
        #[utoipa::path(
            patch,
            path = $id_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
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
        pub(crate) async fn $update_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
            Json(body): Json<NamedSetBody>,
        ) -> Result<Json<NamedSet>, ApiError> {
            update($spec(), &state, &auth.account_id, id, body).await
        }

        #[doc = $delete_doc]
        #[utoipa::path(
            delete,
            path = $id_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
            responses(
                (status = 204),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 404, body = ErrorBody)
            )
        )]
        pub(crate) async fn $delete_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
        ) -> Result<StatusCode, ApiError> {
            delete($spec(), &state, &auth.account_id, id).await
        }

        #[doc = $members_list_doc]
        #[utoipa::path(
            get,
            path = $members_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
            responses(
                (status = 200, body = MemberIdList),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 404, body = ErrorBody)
            )
        )]
        pub(crate) async fn $members_list_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
        ) -> Result<Json<MemberIdList>, ApiError> {
            members_list($spec(), &state, &auth.account_id, id).await
        }

        #[doc = $members_update_doc]
        #[utoipa::path(
            patch,
            path = $members_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
            request_body = MembersPatch,
            responses(
                (status = 200, body = MembersChanged),
                (status = 400, body = ErrorBody),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 404, body = ErrorBody)
            )
        )]
        pub(crate) async fn $members_update_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
            Json(body): Json<MembersPatch>,
        ) -> Result<Json<MembersChanged>, ApiError> {
            members_update($spec(), &state, &auth.account_id, id, body).await
        }
    };
}

named_set_routes! {
    spec: crate::named_membership::group_spec,
    tag: "Contacts",
    id_description: "Contact Group id",
    root_path: "/v1/contact-groups",
    id_path: "/v1/contact-groups/{id}",
    members_path: "/v1/contact-groups/{id}/members",
    list: contact_groups_list, "The account's Contact Groups, A–Z.",
    create: contact_groups_create, "Create a Contact Group.",
    update: contact_groups_update, "Rename a Contact Group.",
    delete: contact_groups_delete, "Delete a Contact Group and its memberships.",
    members_list: contact_group_members_list, "Contact ids in one Contact Group.",
    members_update: contact_group_members_update,
        "Put contacts in and take contacts out of one Contact Group.",
}

named_set_routes! {
    spec: crate::named_membership::tag_spec,
    tag: "Message tags",
    id_description: "Message Tag id",
    root_path: "/v1/message-tags",
    id_path: "/v1/message-tags/{id}",
    members_path: "/v1/message-tags/{id}/members",
    list: message_tags_list, "The account's Message Tags, A–Z.",
    create: message_tags_create, "Create a Message Tag.",
    update: message_tags_update, "Rename a Message Tag.",
    delete: message_tags_delete, "Delete a Message Tag and its memberships.",
    members_list: message_tag_members_list, "Conversation ids in one Message Tag.",
    members_update: message_tag_members_update,
        "Put conversations in and take conversations out of one Message Tag.",
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use crate::server::AppState;
    use crate::test_support::{
        RegisteredAccount, delete_status, get_json, get_status, patch_json, patch_status,
        post_json, post_status, register_via_api, test_vault,
    };

    /// Which collection a case runs against. Every case runs for both.
    #[derive(Clone, Copy)]
    enum Kind {
        Groups,
        Tags,
    }

    impl Kind {
        fn base(self) -> &'static str {
            match self {
                Kind::Groups => "/v1/contact-groups",
                Kind::Tags => "/v1/message-tags",
            }
        }

        /// Insert one row a set of this kind can hold, answering its id.
        async fn member(self, state: &AppState, account_id: &str) -> i64 {
            let mut conn = state.db.acquire().await.unwrap();
            match self {
                Kind::Groups => sqlx::query_scalar(
                    "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Ada') RETURNING id",
                )
                .bind(account_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
                Kind::Tags => {
                    let handle_id: i64 = sqlx::query_scalar(
                        "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
                         VALUES ($1, $2, $2, 'phone', 'phone') RETURNING id",
                    )
                    .bind(account_id)
                    .bind(format!("+1555{}", rand_suffix()))
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap();
                    sqlx::query_scalar(
                        "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
                         VALUES ($1, $2, 'individual', 'seed.jsonl') RETURNING id",
                    )
                    .bind(account_id)
                    .bind(handle_id)
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap()
                }
            }
        }
    }

    /// Distinct handle text per inserted conversation.
    fn rand_suffix() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    async fn alice(state: &AppState) -> RegisteredAccount {
        register_via_api(state, "alice", "hunter2hunter2").await
    }

    async fn create(state: &AppState, kind: Kind, token: &str, name: &str) -> i64 {
        let set: Value = post_json(state, kind.base(), token, json!({ "name": name })).await;
        set["id"].as_i64().unwrap()
    }

    async fn names(state: &AppState, kind: Kind, token: &str) -> Vec<String> {
        let list: Value = get_json(state, kind.base(), token).await;
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_string())
            .collect()
    }

    async fn member_ids(state: &AppState, kind: Kind, token: &str, id: i64) -> Vec<i64> {
        let list: Value = get_json(state, &format!("{}/{id}/members", kind.base()), token).await;
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn create_list_update_and_delete_a_set() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;

            let created: Value = post_json(
                state,
                kind.base(),
                &user.token,
                json!({ "name": " Family " }),
            )
            .await;
            assert_eq!(created["name"], "Family");
            let id = created["id"].as_i64().unwrap();
            assert!(created.get("ok").is_none());

            create(state, kind, &user.token, "Work").await;
            assert_eq!(
                names(state, kind, &user.token).await,
                vec!["Family", "Work"]
            );

            let updated: Value = patch_json(
                state,
                &format!("{}/{id}", kind.base()),
                &user.token,
                json!({ "name": "Fam" }),
            )
            .await;
            assert_eq!(updated, json!({ "id": id, "name": "Fam" }));

            let case_only: Value = patch_json(
                state,
                &format!("{}/{id}", kind.base()),
                &user.token,
                json!({ "name": "fam" }),
            )
            .await;
            assert_eq!(case_only["name"], "fam");
            assert_eq!(names(state, kind, &user.token).await, vec!["fam", "Work"]);

            assert_eq!(
                delete_status(state, &format!("{}/{id}", kind.base()), &user.token).await,
                StatusCode::NO_CONTENT
            );
            assert_eq!(names(state, kind, &user.token).await, vec!["Work"]);
            assert_eq!(
                delete_status(state, &format!("{}/{id}", kind.base()), &user.token).await,
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn create_and_update_refuse_duplicate_empty_and_reserved_names() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            create(state, kind, &user.token, "Family").await;
            let work = create(state, kind, &user.token, "Work").await;

            assert_eq!(
                post_status(state, kind.base(), &user.token, json!({ "name": "family" })).await,
                StatusCode::CONFLICT
            );
            assert_eq!(
                post_status(state, kind.base(), &user.token, json!({ "name": "Trash" })).await,
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                post_status(state, kind.base(), &user.token, json!({ "name": "  " })).await,
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                patch_status(
                    state,
                    &format!("{}/{work}", kind.base()),
                    &user.token,
                    json!({ "name": "FAMILY" })
                )
                .await,
                StatusCode::CONFLICT
            );
            assert_eq!(
                patch_status(
                    state,
                    &format!("{}/{work}", kind.base()),
                    &user.token,
                    json!({ "name": "" })
                )
                .await,
                StatusCode::BAD_REQUEST
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_id_answers_404_on_every_route() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let base = kind.base();
            assert_eq!(
                patch_status(
                    state,
                    &format!("{base}/999"),
                    &user.token,
                    json!({ "name": "X" })
                )
                .await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                delete_status(state, &format!("{base}/999"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                get_status(state, &format!("{base}/999/members"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                patch_status(
                    state,
                    &format!("{base}/999/members"),
                    &user.token,
                    json!({ "add": [1] })
                )
                .await,
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn members_patch_adds_and_removes_in_one_call() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let a = kind.member(state, &user.account_id).await;
            let b = kind.member(state, &user.account_id).await;
            let id = create(state, kind, &user.token, "Family").await;
            let members = format!("{}/{id}/members", kind.base());

            let changed: Value =
                patch_json(state, &members, &user.token, json!({ "add": [a, b] })).await;
            assert_eq!(changed, json!({ "added": 2, "removed": 0 }));
            assert_eq!(member_ids(state, kind, &user.token, id).await, vec![a, b]);

            let changed: Value = patch_json(
                state,
                &members,
                &user.token,
                json!({ "add": [a], "remove": [b] }),
            )
            .await;
            assert_eq!(changed, json!({ "added": 0, "removed": 1 }));
            assert_eq!(member_ids(state, kind, &user.token, id).await, vec![a]);

            assert_eq!(
                patch_status(state, &members, &user.token, json!({})).await,
                StatusCode::BAD_REQUEST
            );
        }
    }

    #[tokio::test]
    async fn members_patch_with_a_foreign_member_writes_nothing() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let a = kind.member(state, &user.account_id).await;
            let id = create(state, kind, &user.token, "Family").await;
            let members = format!("{}/{id}/members", kind.base());
            assert_eq!(
                patch_status(state, &members, &user.token, json!({ "add": [a, 999999] })).await,
                StatusCode::NOT_FOUND
            );
            assert!(member_ids(state, kind, &user.token, id).await.is_empty());
        }
    }

    #[tokio::test]
    async fn another_accounts_set_is_not_visible() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let bob = register_via_api(state, "bob", "hunter2hunter2").await;
            let id = create(state, kind, &bob.token, "Holiday").await;
            let base = kind.base();

            assert!(names(state, kind, &user.token).await.is_empty());
            assert_eq!(
                patch_status(
                    state,
                    &format!("{base}/{id}"),
                    &user.token,
                    json!({ "name": "X" })
                )
                .await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                delete_status(state, &format!("{base}/{id}"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                get_status(state, &format!("{base}/{id}/members"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                patch_status(
                    state,
                    &format!("{base}/{id}/members"),
                    &user.token,
                    json!({ "add": [1] })
                )
                .await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(names(state, kind, &bob.token).await, vec!["Holiday"]);
        }
    }
}
