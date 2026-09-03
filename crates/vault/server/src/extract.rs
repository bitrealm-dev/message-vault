//! Axum's `Query`, `Path`, and `Json`, answering in the vault's own error body.
//!
//! Axum's extractors reject a bad request with a plain-text body. Every other
//! failure on this interface is `{"error": "<sentence>"}` with the status, so
//! these three wrappers turn each rejection into an [`ApiError::BadRequest`]
//! carrying Axum's sentence. Handlers use these names in place of Axum's.

use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::server::ApiError;

/// Axum's `Query`, rejecting as `{error}`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Query<T>(pub T);

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Query(value)),
            Err(rejection) => Err(ApiError::BadRequest(rejection.body_text())),
        }
    }
}

/// Axum's `Path`, rejecting as `{error}`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Path<T>(pub T);

impl<T, S> FromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Path(value)),
            Err(rejection) => Err(ApiError::BadRequest(rejection.body_text())),
        }
    }
}

/// Axum's `Json`, rejecting as `{error}` and answering as JSON.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(Json(value)),
            // Axum already picked the right status (413 over the body limit,
            // 415 for a missing/wrong Content-Type, 400 for malformed JSON);
            // keep it rather than flattening everything to 400.
            Err(rejection) => Err(ApiError::Status(rejection.status(), rejection.body_text())),
        }
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support::{register_via_api, test_vault};

    /// GET a path and return the status and the parsed JSON body.
    async fn get(
        state: &crate::server::AppState,
        path: &str,
        token: &str,
    ) -> (StatusCode, serde_json::Value) {
        let (status, text) = crate::test_support::get_raw(state, path, token).await;
        let body = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("{path} answered non-JSON: {text}"));
        (status, body)
    }

    #[tokio::test]
    async fn a_query_parameter_of_the_wrong_type_is_a_json_400() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, body) = get(&state, "/v1/conversations?limit=ten", &user.token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("limit"), "{body}");
        assert!(body.get("ok").is_none());
    }

    #[tokio::test]
    async fn a_path_id_that_is_not_a_number_is_a_json_400() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, body) = get(&state, "/v1/conversations/abc/sources", &user.token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].is_string(), "{body}");
    }

    #[tokio::test]
    async fn a_json_body_missing_a_field_is_a_json_422() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, text) = crate::test_support::post_raw(
            &state,
            "/v1/saved-searches",
            &user.token,
            "application/json",
            r#"{"name": "only a name"}"#,
        )
        .await;
        // Well-formed JSON that fails to deserialize into the target type is
        // Axum's `JsonDataError`, which carries `422` — a different rejection
        // from malformed JSON syntax (`400`). Since `Status` now keeps
        // whatever status Axum picked, this answers 422, not 400.
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(body["error"].as_str().unwrap().contains("query"), "{body}");
    }

    #[tokio::test]
    async fn a_json_body_with_the_wrong_content_type_is_a_json_415() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, text) = crate::test_support::post_raw(
            &state,
            "/v1/saved-searches",
            &user.token,
            "text/plain",
            r#"{"name": "only a name", "query": "hi"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let body: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|_| panic!("non-JSON body: {text}"));
        assert!(body["error"].is_string(), "{body}");
    }

    #[tokio::test]
    async fn a_json_body_over_the_auth_router_body_limit_is_a_json_413() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        // The auth router caps request bodies at 32 KiB (server.rs,
        // `limited_auth_router`); pad well past it with a valid JSON string.
        let padding = "a".repeat(64 * 1024);
        let body =
            serde_json::json!({ "username": padding, "password": "hunter2hunter2" }).to_string();
        let (status, text) = crate::test_support::post_raw(
            &state,
            "/v1/auth/login",
            "unused-token",
            "application/json",
            body,
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        let body: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|_| panic!("non-JSON body: {text}"));
        assert!(body["error"].is_string(), "{body}");
    }

    #[tokio::test]
    async fn an_unknown_api_path_is_a_json_404_and_a_wrong_method_a_json_405() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, body) = get(&state, "/v1/no-such-thing", &user.token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "no route at /v1/no-such-thing");

        let (status, text) =
            crate::test_support::delete_raw(&state, "/v1/conversations", &user.token).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["error"], "DELETE is not allowed at /v1/conversations");
    }

    #[tokio::test]
    async fn bare_v1_and_v1_slash_are_a_json_404() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        for path in ["/v1", "/v1/"] {
            let (status, text) = crate::test_support::get_raw(&state, path, "unused-token").await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            let body: serde_json::Value = serde_json::from_str(&text)
                .unwrap_or_else(|_| panic!("{path} answered non-JSON: {text}"));
            assert!(body["error"].is_string(), "{path}: {body}");
        }
    }
}
