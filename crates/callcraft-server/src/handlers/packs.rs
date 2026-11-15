use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::AppState;
use crate::auth::{TokenQuery, auth_guard, error_response};

pub(crate) async fn list_packs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let view = state.packs.list();
    Json(json!({
        "ok": true,
        "packs": view.packs,
        "default_pack_id": view.default_pack_id,
        "warning": view.warning,
    }))
    .into_response()
}

pub(crate) async fn save_default_scenario(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: String,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match state.packs.save_default(&body) {
        Ok(result) => Json(json!({"ok": true, "pack_id": result.pack_id, "scenario": result.scenario, "warnings": result.warnings})).into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub(crate) async fn save_scenario(
    State(state): State<AppState>,
    Path(pack_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: String,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match state.packs.save(&pack_id, &body) {
        Ok(result) => Json(json!({"ok": true, "pack_id": result.pack_id, "scenario": result.scenario, "warnings": result.warnings})).into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub(crate) async fn open_scenario(
    State(state): State<AppState>,
    Path((pack_id, scenario_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match state.packs.open(&pack_id, &scenario_id) {
        Ok((yaml, validated)) => Json(json!({
            "ok": true,
            "pack_id": pack_id,
            "yaml": yaml,
            "scenario": validated.scenario,
            "warnings": validated.warnings,
        }))
        .into_response(),
        Err(error) => error_response(StatusCode::NOT_FOUND, &error),
    }
}
