use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::AppState;
use crate::auth::{TokenQuery, auth_guard, error_response};

pub(crate) async fn validate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: String,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }

    match callcraft_core::validate_yaml(&body, false) {
        Ok(validated) => Json(json!({
            "ok": true,
            "id": validated.scenario.id,
            "title": validated.scenario.title,
            "facts": validated.scenario.facts.len(),
            "rubric_items": validated.scenario.rubric.items.len(),
            "warnings": validated.warnings,
        }))
        .into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error.to_string()),
    }
}

pub(crate) async fn parse_scenario(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: String,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match callcraft_core::validate_yaml(&body, false) {
        Ok(validated) => Json(json!({
            "ok": true,
            "scenario": validated.scenario,
            "warnings": validated.warnings,
        }))
        .into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error.to_string()),
    }
}

pub(crate) async fn validate_scenario_json(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(value): Json<serde_json::Value>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match callcraft_core::validate_value(&value, false) {
        Ok(validated) => Json(json!({
            "ok": true,
            "warnings": validated.warnings,
            "scenario": validated.scenario,
        }))
        .into_response(),
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error.to_string()),
    }
}

pub(crate) async fn render_scenario(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(value): Json<serde_json::Value>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    match callcraft_core::validate_value(&value, false) {
        Ok(validated) => match callcraft_core::render_yaml(&validated.scenario) {
            Ok(yaml) => Json(json!({
                "ok": true,
                "yaml": yaml,
                "warnings": validated.warnings,
            }))
            .into_response(),
            Err(error) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("render failed: {error}"),
            ),
        },
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error.to_string()),
    }
}
