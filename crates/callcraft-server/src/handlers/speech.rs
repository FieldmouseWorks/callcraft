use std::time::Instant;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use base64::Engine;

use crate::AppState;
use crate::auth::{TokenQuery, auth_guard, error_response};
use crate::handlers::sessions::{produce_turn, record_latency};
use crate::providers;
use crate::speech;

pub(crate) async fn voice_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let caller_mode = if state.caller.lock().expect("caller lock").is_enabled() {
        "llm"
    } else {
        "deterministic"
    };
    let (stt, tts) = {
        let speech = state.speech.lock().expect("speech lock");
        (speech.stt.is_some(), speech.tts.is_some())
    };
    Json(json!({
        "ok": true,
        "stt": stt,
        "tts": tts,
        "caller": caller_mode,
    }))
    .into_response()
}

pub(crate) async fn voice_turn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: Bytes,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let speech = state.speech.lock().expect("speech lock").clone();
    let Some(stt_config) = speech.stt.clone() else {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "speech-to-text is not configured",
        );
    };
    if body.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "empty audio body");
    }
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/webm")
        .to_string();
    if !mime.starts_with("audio/") {
        return error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "audio content type required",
        );
    }

    {
        let sessions = state.sessions.lock().expect("sessions lock");
        let Some(stored) = sessions.get(&id) else {
            return error_response(StatusCode::NOT_FOUND, "session not found");
        };
        if stored.finished {
            return error_response(StatusCode::CONFLICT, "session is finished");
        }
    }

    let stt_started = Instant::now();
    let stt_result = speech::transcribe(&state.http, &stt_config, &mime, &body).await;
    record_latency(&state, &id, "stt", stt_started.elapsed().as_millis() as u64);
    let transcript = match stt_result {
        Ok(text) => text,
        Err(error) => return error_response(StatusCode::BAD_GATEWAY, error.message()),
    };
    let transcript = transcript.trim().to_string();
    if transcript.is_empty() {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, "no speech detected");
    }

    let turn_started = Instant::now();
    let outcome = match produce_turn(&state, &id, &transcript).await {
        Ok(outcome) => outcome,
        Err(response) => return *response,
    };
    record_latency(
        &state,
        &id,
        "turn",
        turn_started.elapsed().as_millis() as u64,
    );

    let mut payload = json!({
        "ok": true,
        "transcript": transcript,
        "caller": outcome.caller.clone(),
        "caller_source": outcome.source,
        "caller_error": outcome.caller_error,
    });

    if let Some(tts_config) = speech.tts.clone() {
        let tts_started = Instant::now();
        let tts_result = speech::synthesize(&state.http, &tts_config, &outcome.caller).await;
        record_latency(&state, &id, "tts", tts_started.elapsed().as_millis() as u64);
        match tts_result {
            Ok((audio, audio_mime)) => {
                payload["audio_base64"] =
                    json!(base64::engine::general_purpose::STANDARD.encode(audio));
                payload["audio_mime"] = json!(audio_mime);
            }
            Err(error) => {
                payload["audio_error"] = json!(error.message());
            }
        }
    }

    Json(payload).into_response()
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpeechPayload {
    kind: speech::SpeechKind,
    provider_id: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    voice: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpeechResetPayload {
    kind: speech::SpeechKind,
}

fn provided_speech_key(payload: &SpeechPayload) -> Option<String> {
    payload
        .api_key
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn speech_key(
    state: &AppState,
    kind: speech::SpeechKind,
    provider_id: &str,
    provided: Option<String>,
    requested_base_url: Option<&str>,
) -> Option<String> {
    let namespace = match kind {
        speech::SpeechKind::Stt => "stt",
        speech::SpeechKind::Tts => "tts",
    };
    let requested = requested_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let current_key = {
        let speech = state.speech.lock().expect("speech lock");
        match kind {
            speech::SpeechKind::Stt => speech
                .stt
                .as_ref()
                .filter(|config| config.provider_id.as_deref() == Some(provider_id))
                .filter(|config| {
                    requested.is_none_or(|base| providers::same_base(&config.base_url, base))
                })
                .and_then(|config| config.api_key.clone()),
            speech::SpeechKind::Tts => speech
                .tts
                .as_ref()
                .filter(|config| config.provider_id.as_deref() == Some(provider_id))
                .filter(|config| {
                    requested.is_none_or(|base| providers::same_base(&config.base_url, base))
                })
                .and_then(|config| config.api_key.clone()),
        }
    };
    if let Some(key) = provided.or(current_key) {
        return Some(key);
    }

    let stored = state.settings.load_speech()?;
    let stored_matches = match kind {
        speech::SpeechKind::Stt => stored
            .stt
            .as_ref()
            .filter(|config| config.provider_id == provider_id)
            .is_some_and(|config| {
                requested.is_none_or(|base| providers::same_base(&config.base_url, base))
            }),
        speech::SpeechKind::Tts => stored
            .tts
            .as_ref()
            .filter(|config| config.provider_id == provider_id)
            .is_some_and(|config| {
                requested.is_none_or(|base| providers::same_base(&config.base_url, base))
            }),
    };
    if !stored_matches {
        return None;
    }
    state.settings.load_key_ns(namespace, provider_id)
}

fn active_speech(state: &AppState) -> serde_json::Value {
    let speech = state.speech.lock().expect("speech lock");
    let stt = speech.stt.as_ref().map(|config| {
        json!({
            "provider_id": config.provider_id,
            "base_url": config.base_url,
            "model": config.model,
            "language": config.language,
            "api_key_set": config.api_key.as_deref().map(|key| !key.trim().is_empty()).unwrap_or(false),
        })
    });
    let tts = speech.tts.as_ref().map(|config| {
        json!({
            "provider_id": config.provider_id,
            "base_url": config.base_url,
            "model": config.model,
            "voice": config.voice,
            "api_key_set": config.api_key.as_deref().map(|key| !key.trim().is_empty()).unwrap_or(false),
        })
    });
    json!({"stt": stt, "tts": tts})
}

pub(crate) async fn speech_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    Json(json!({
        "ok": true,
        "stt": speech::STT_PROVIDERS,
        "tts": speech::TTS_PROVIDERS,
        "active": active_speech(&state),
        "keyring": state.settings.keyring_enabled(),
    }))
    .into_response()
}

pub(crate) async fn speech_test(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<SpeechPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let provided = provided_speech_key(&payload);
    let api_key = speech_key(
        &state,
        payload.kind,
        &payload.provider_id,
        provided,
        payload.base_url.as_deref(),
    );
    let started = Instant::now();

    match payload.kind {
        speech::SpeechKind::Stt => {
            let config = match speech::stt_from_request(
                &payload.provider_id,
                payload.base_url.clone(),
                payload.model.clone(),
                payload.language.clone(),
                api_key,
            ) {
                Ok(config) => config,
                Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
            };
            match speech::transcribe(
                &state.http,
                &config,
                speech::STT_TEST_MIME,
                speech::STT_TEST_AUDIO,
            )
            .await
            {
                Ok(transcript) => Json(json!({
                    "ok": true,
                    "kind": "stt",
                    "provider_id": config.provider_id,
                    "model": config.model,
                    "latency_ms": started.elapsed().as_millis(),
                    "transcript": transcript,
                }))
                .into_response(),
                Err(error) => Json(json!({
                    "ok": false,
                    "kind": "stt",
                    "error": error.message(),
                    "latency_ms": started.elapsed().as_millis(),
                }))
                .into_response(),
            }
        }
        speech::SpeechKind::Tts => {
            let config = match speech::tts_from_request(
                &payload.provider_id,
                payload.base_url.clone(),
                payload.model.clone(),
                payload.voice.clone(),
                api_key,
            ) {
                Ok(config) => config,
                Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
            };
            match speech::synthesize(&state.http, &config, "Callcraft voice connection test.").await
            {
                Ok((audio, mime)) => Json(json!({
                    "ok": true,
                    "kind": "tts",
                    "provider_id": config.provider_id,
                    "model": config.model,
                    "voice": config.voice,
                    "latency_ms": started.elapsed().as_millis(),
                    "bytes": audio.len(),
                    "audio_mime": mime,
                    "audio_base64": base64::engine::general_purpose::STANDARD.encode(audio),
                }))
                .into_response(),
                Err(error) => Json(json!({
                    "ok": false,
                    "kind": "tts",
                    "error": error.message(),
                    "latency_ms": started.elapsed().as_millis(),
                }))
                .into_response(),
            }
        }
    }
}

pub(crate) async fn speech_configure(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<SpeechPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let provided = provided_speech_key(&payload);
    let api_key = speech_key(
        &state,
        payload.kind,
        &payload.provider_id,
        provided.clone(),
        payload.base_url.as_deref(),
    );

    enum NewConfig {
        Stt(speech::SttConfig),
        Tts(speech::TtsConfig),
    }

    let new_config = match payload.kind {
        speech::SpeechKind::Stt => match speech::stt_from_request(
            &payload.provider_id,
            payload.base_url.clone(),
            payload.model.clone(),
            payload.language.clone(),
            api_key,
        ) {
            Ok(config) => NewConfig::Stt(config),
            Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
        },
        speech::SpeechKind::Tts => match speech::tts_from_request(
            &payload.provider_id,
            payload.base_url.clone(),
            payload.model.clone(),
            payload.voice.clone(),
            api_key,
        ) {
            Ok(config) => NewConfig::Tts(config),
            Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
        },
    };

    let mut stored = state.settings.load_speech().unwrap_or_default();
    let namespace = match payload.kind {
        speech::SpeechKind::Stt => "stt",
        speech::SpeechKind::Tts => "tts",
    };
    if let Some(key) = provided.as_deref()
        && let Err(error) = state
            .settings
            .save_key_ns(namespace, &payload.provider_id, key)
    {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }

    match &new_config {
        NewConfig::Stt(config) => {
            stored.stt = Some(providers::StoredStt {
                provider_id: payload.provider_id.clone(),
                base_url: config.base_url.clone(),
                model: config.model.clone(),
                language: config.language.clone().unwrap_or_default(),
            });
        }
        NewConfig::Tts(config) => {
            stored.tts = Some(providers::StoredTts {
                provider_id: payload.provider_id.clone(),
                base_url: config.base_url.clone(),
                model: config.model.clone(),
                voice: config.voice.clone().unwrap_or_default(),
            });
        }
    }

    if let Err(error) = state.settings.save_speech(&stored) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }

    let mut speech = state.speech.lock().expect("speech lock");
    match new_config {
        NewConfig::Stt(config) => speech.stt = Some(config),
        NewConfig::Tts(config) => speech.tts = Some(config),
    }
    drop(speech);

    Json(json!({
        "ok": true,
        "active": active_speech(&state),
    }))
    .into_response()
}

pub(crate) async fn speech_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<SpeechResetPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let mut stored = state.settings.load_speech().unwrap_or_default();
    match payload.kind {
        speech::SpeechKind::Stt => {
            stored.stt = None;
            state.speech.lock().expect("speech lock").stt = None;
        }
        speech::SpeechKind::Tts => {
            stored.tts = None;
            state.speech.lock().expect("speech lock").tts = None;
        }
    }
    let result = if stored.stt.is_none() && stored.tts.is_none() {
        state.settings.clear_speech()
    } else {
        state.settings.save_speech(&stored)
    };
    if let Err(error) = result {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    Json(json!({
        "ok": true,
        "active": active_speech(&state),
    }))
    .into_response()
}
