use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::packs::{ImportMode, ImportReport};

#[derive(Debug, Deserialize)]
pub(crate) struct ImportQuery {
    mode: Option<String>,
}
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

pub(crate) async fn export_pack(
    State(state): State<AppState>,
    Path(pack_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match state.packs.export(&pack_id) {
        Ok((bytes, filename)) => {
            let mut response = Response::new(Body::from(bytes));
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/zip"),
            );
            if let Ok(value) =
                HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            {
                headers.insert(header::CONTENT_DISPOSITION, value);
            }
            response
        }
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub(crate) async fn inspect_import(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: Bytes,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match state.packs.inspect_bytes(&body) {
        Ok(report) => Json(json!({"ok": true, "dry_run": true, "report": report})).into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub(crate) async fn import_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Query(import): Query<ImportQuery>,
    body: Bytes,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let Some(mode) = import.mode.as_deref() else {
        return error_response(StatusCode::BAD_REQUEST, "mode=install|replace is required");
    };
    let mode = match ImportMode::parse(mode) {
        Ok(mode) => mode,
        Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
    };
    match state.packs.import_bytes(&body, mode) {
        Ok(report) => json_import(&report, false).into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub(crate) async fn rollback_pack(
    State(state): State<AppState>,
    Path(pack_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match state.packs.rollback(&pack_id) {
        Ok(report) => json_import(&report, true).into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

fn json_import(report: &ImportReport, rollback: bool) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "dry_run": false,
        "rollback": rollback,
        "report": report,
    }))
}
