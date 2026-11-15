use std::time::Instant;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::auth::{TokenQuery, auth_guard, error_response};
use crate::caller_llm;
use crate::providers;
use crate::{AppState, CallerConfig, EXAMPLE_SCENARIO};

#[derive(Debug, Deserialize)]
pub(crate) struct ProviderPayload {
    provider_id: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
}

fn provided_api_key(payload: &ProviderPayload) -> Option<String> {
    payload
        .api_key
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn stored_api_key(
    state: &AppState,
    provider_id: &str,
    requested_base_url: Option<&str>,
) -> Option<String> {
    let requested = requested_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let current_key = {
        let caller = state.caller.lock().expect("caller lock");
        caller
            .provider_id
            .as_deref()
            .filter(|id| *id == provider_id)
            .filter(|_| requested.is_none_or(|base| providers::same_base(&caller.base_url, base)))
            .and_then(|_| caller.api_key.clone())
    };
    if current_key.is_some() {
        return current_key;
    }

    let stored = state.settings.load()?;
    if stored.provider_id != provider_id {
        return None;
    }
    if let Some(base) = requested
        && !providers::same_base(&stored.base_url, base)
    {
        return None;
    }
    state.settings.load_key(provider_id)
}

fn active_provider(state: &AppState) -> serde_json::Value {
    let caller = state.caller.lock().expect("caller lock");
    json!({
        "provider_id": caller.provider_id,
        "base_url": caller.base_url,
        "model": caller.model,
        "api_key_set": caller
            .api_key
            .as_deref()
            .map(|key| !key.trim().is_empty())
            .unwrap_or(false),
        "enabled": caller.is_enabled(),
    })
}

pub(crate) async fn providers_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    Json(json!({
        "ok": true,
        "providers": providers::PROVIDERS,
        "active": active_provider(&state),
        "keyring": state.settings.keyring_enabled(),
    }))
    .into_response()
}

pub(crate) async fn provider_test(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<ProviderPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let api_key = provided_api_key(&payload)
        .or_else(|| stored_api_key(&state, &payload.provider_id, payload.base_url.as_deref()));
    let config = match providers::config_from_request(
        &payload.provider_id,
        payload.base_url.clone(),
        payload.model.clone(),
        api_key,
    ) {
        Ok(config) => config,
        Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
    };
    let scenario = match callcraft_core::validate_yaml(EXAMPLE_SCENARIO, false) {
        Ok(validated) => validated.scenario,
        Err(error) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("embedded scenario: {error}"),
            );
        }
    };

    let started = Instant::now();
    match caller_llm::decide(
        &state.http,
        &config,
        &scenario,
        &[],
        &[],
        &[],
        0.0,
        "Where are you located?",
    )
    .await
    {
        Ok(decision) => match caller_llm::render(
            &decision,
            &scenario,
            &[],
            "Where are you located?",
            &[],
            0.0,
            true,
        ) {
            Ok((caller, fact_ids)) => Json(json!({
                "ok": true,
                "provider_id": config.provider_id,
                "model": config.model,
                "latency_ms": started.elapsed().as_millis(),
                "fact_ids": fact_ids,
                "style": decision.style,
                "caller": caller,
            }))
            .into_response(),
            Err(error) => Json(json!({
                "ok": false,
                "error": error.message(),
                "latency_ms": started.elapsed().as_millis(),
            }))
            .into_response(),
        },
        Err(error) => Json(json!({
            "ok": false,
            "error": error.message(),
            "latency_ms": started.elapsed().as_millis(),
        }))
        .into_response(),
    }
}

pub(crate) async fn provider_configure(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<ProviderPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }

    if payload.provider_id == "deterministic" {
        if let Err(error) = state.settings.clear() {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
        }
        *state.caller.lock().expect("caller lock") = CallerConfig::default();
        return Json(json!({
            "ok": true,
            "active": active_provider(&state),
        }))
        .into_response();
    }

    let provided_key = provided_api_key(&payload);
    let api_key = provided_key
        .clone()
        .or_else(|| stored_api_key(&state, &payload.provider_id, payload.base_url.as_deref()));
    let config = match providers::config_from_request(
        &payload.provider_id,
        payload.base_url.clone(),
        payload.model.clone(),
        api_key,
    ) {
        Ok(config) => config,
        Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
    };

    if let Some(key) = provided_key.as_deref()
        && let Err(error) = state.settings.save_key(&payload.provider_id, key)
    {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    let stored = providers::StoredConfig {
        provider_id: payload.provider_id.clone(),
        base_url: config.base_url.clone(),
        model: config.model.clone(),
    };
    if let Err(error) = state.settings.save(&stored) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }

    *state.caller.lock().expect("caller lock") = config;
    {
        let mut sessions = state.sessions.lock().expect("sessions lock");
        for stored in sessions.values_mut() {
            stored.caller_failures = 0;
            stored.caller_disabled = false;
        }
    }
    Json(json!({
        "ok": true,
        "active": active_provider(&state),
        "keyring": state.settings.keyring_enabled(),
    }))
    .into_response()
}

pub(crate) async fn provider_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    if let Err(error) = state.settings.clear() {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    *state.caller.lock().expect("caller lock") = CallerConfig::default();
    Json(json!({
        "ok": true,
        "active": active_provider(&state),
    }))
    .into_response()
}
