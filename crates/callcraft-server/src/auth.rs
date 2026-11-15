use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::AppState;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct TokenQuery {}

pub(crate) fn error_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({"ok": false, "error": message})),
    )
        .into_response()
}

fn authorized(headers: &HeaderMap, _query: &TokenQuery, state: &AppState) -> bool {
    let host_ok = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(host_allowed)
        .unwrap_or(false);
    let header_token = headers
        .get("x-callcraft-token")
        .and_then(|value| value.to_str().ok());
    host_ok && header_token == Some(state.token.as_str())
}

fn host_allowed(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.');
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else if host.matches(':').count() == 1 {
        host.split(':').next().unwrap_or(host)
    } else {
        host
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

pub(crate) fn auth_guard(
    state: &AppState,
    headers: &HeaderMap,
    query: &TokenQuery,
) -> Option<Response> {
    if authorized(headers, query, state) {
        None
    } else {
        Some(error_response(StatusCode::UNAUTHORIZED, "unauthorized"))
    }
}
