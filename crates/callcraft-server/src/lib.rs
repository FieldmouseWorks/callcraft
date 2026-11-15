//! Loopback-only HTTP API and static UI for Callcraft.
//!
//! The Tauri shell starts this server in-process and points its webview at it,
//! so the desktop app and the browser (`callcraft serve`) share one API, one origin
//! model and one test surface.

mod caller_llm;

pub use caller_llm::CallerConfig;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

use base64::Engine;
use reqwest::multipart;

use callcraft_core::report;
use callcraft_core::scenario::Scenario;
use callcraft_core::session::{Session, Speaker, Utterance};
use callcraft_core::{CallerEngine, report::Report};

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const AUTHOR_JS: &str = include_str!("../../../web/author.js");
const STYLES_CSS: &str = include_str!("../../../web/styles.css");
const EXAMPLE_SCENARIO: &str = include_str!("../../../examples/chest-pain-001.scenario.yaml");

const SESSION_TTL: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_SESSIONS: usize = 64;

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
    sessions: Arc<Mutex<HashMap<String, StoredSession>>>,
    voice: VoiceConfig,
    caller: CallerConfig,
    http: reqwest::Client,
}

struct StoredSession {
    scenario: Scenario,
    session: Session,
    engine: CallerEngine,
    started: Instant,
    finished: bool,
}

/// Out-of-process voice adapters. Providers are commodity HTTP endpoints; the
/// core never embeds a speech or model runtime. Empty URLs disable a direction.
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    pub stt_url: Option<String>,
    pub stt_model: String,
    pub stt_language: Option<String>,
    pub tts_url: Option<String>,
    pub tts_model: String,
    pub tts_voice: String,
    pub adapter_token: Option<String>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            stt_url: None,
            stt_model: "whisper-1".to_string(),
            stt_language: Some("en".to_string()),
            tts_url: None,
            tts_model: "tts-1".to_string(),
            tts_voice: "default".to_string(),
            adapter_token: None,
        }
    }
}

impl VoiceConfig {
    pub fn from_env() -> Self {
        Self {
            stt_url: env_url("CALLCRAFT_STT_URL"),
            stt_model: env_string("CALLCRAFT_STT_MODEL").unwrap_or_else(|| "whisper-1".to_string()),
            stt_language: env_string("CALLCRAFT_STT_LANGUAGE").or_else(|| Some("en".to_string())),
            tts_url: env_url("CALLCRAFT_TTS_URL"),
            tts_model: env_string("CALLCRAFT_TTS_MODEL").unwrap_or_else(|| "tts-1".to_string()),
            tts_voice: env_string("CALLCRAFT_TTS_VOICE").unwrap_or_else(|| "default".to_string()),
            adapter_token: env_string("CALLCRAFT_ADAPTER_TOKEN"),
        }
    }
}

fn env_url(key: &str) -> Option<String> {
    env_string(key)
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Start the loopback server on a background thread.
///
/// Port 0 asks the OS for an ephemeral port. Returns the actual port and a
/// join handle; the caller decides how long to keep it alive.
pub fn start(port: u16, token: String) -> anyhow::Result<(u16, std::thread::JoinHandle<()>)> {
    start_with_adapters(
        port,
        token,
        VoiceConfig::from_env(),
        CallerConfig::from_env(),
    )
}

/// Start with explicit voice adapters and a deterministic caller.
pub fn start_with_config(
    port: u16,
    token: String,
    voice: VoiceConfig,
) -> anyhow::Result<(u16, std::thread::JoinHandle<()>)> {
    start_with_adapters(port, token, voice, CallerConfig::default())
}

/// Start with explicit voice and caller adapter configuration.
pub fn start_with_adapters(
    port: u16,
    token: String,
    voice: VoiceConfig,
    caller: CallerConfig,
) -> anyhow::Result<(u16, std::thread::JoinHandle<()>)> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    let actual_port = listener.local_addr()?.port();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| anyhow::anyhow!("http client: {error}"))?;
    let state = AppState {
        token: Arc::new(token),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        voice,
        caller,
        http,
    };

    let handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("listener");
            axum::serve(listener, app(state)).await.expect("server");
        });
    });

    Ok((actual_port, handle))
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/author.js", get(author_js))
        .route("/styles.css", get(styles))
        .route("/api/health", get(health))
        .route("/api/example", get(example))
        .route("/api/validate", post(validate))
        .route("/api/voice/status", get(voice_status))
        .route("/api/scenario/parse", post(parse_scenario))
        .route("/api/scenario/validate", post(validate_scenario_json))
        .route("/api/scenario/render", post(render_scenario))
        .route("/api/sessions", post(create_session))
        .route(
            "/api/sessions/{id}",
            get(get_session).delete(delete_session),
        )
        .route("/api/sessions/{id}/turns", post(turn))
        .route("/api/sessions/{id}/events", post(event))
        .route("/api/sessions/{id}/finish", post(finish))
        .route("/api/sessions/{id}/voice-turn", post(voice_turn))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
        .into_response()
}

async fn author_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        AUTHOR_JS,
    )
        .into_response()
}

async fn styles() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLES_CSS,
    )
        .into_response()
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"ok": true, "service": "callcraft-server"}))
}

async fn example() -> Json<serde_json::Value> {
    Json(json!({"ok": true, "yaml": EXAMPLE_SCENARIO}))
}

#[derive(Debug, Deserialize, Default)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TurnPayload {
    text: String,
}

#[derive(Debug, Deserialize)]
struct EventPayload {
    name: String,
}

#[derive(Debug, Serialize)]
struct ScenarioSummary {
    id: String,
    title: String,
    disclaimer: String,
}

#[derive(Debug, Serialize)]
struct SessionStateView {
    ok: bool,
    scenario: ScenarioSummary,
    transcript: Vec<Utterance>,
    events: Vec<String>,
    finished: bool,
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

fn authorized(headers: &HeaderMap, query: &TokenQuery, state: &AppState) -> bool {
    let host_ok = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(|host| host.starts_with("127.0.0.1") || host.starts_with("localhost"))
        .unwrap_or(false);
    let header_token = headers
        .get("x-callcraft-token")
        .and_then(|value| value.to_str().ok());
    let token_ok = header_token == Some(state.token.as_str())
        || query.token.as_deref() == Some(state.token.as_str());
    host_ok && token_ok
}

fn auth_guard(state: &AppState, headers: &HeaderMap, query: &TokenQuery) -> Option<Response> {
    if authorized(headers, query, state) {
        None
    } else {
        Some(error_response(StatusCode::UNAUTHORIZED, "unauthorized"))
    }
}

fn summary(scenario: &Scenario) -> ScenarioSummary {
    ScenarioSummary {
        id: scenario.id.clone(),
        title: scenario.title.clone(),
        disclaimer: scenario.safety.disclaimer.clone(),
    }
}

fn collect_events(scenario: &Scenario) -> Vec<String> {
    let mut events = BTreeSet::new();
    for item in &scenario.rubric.items {
        if let Some(anchors) = &item.anchors {
            for anchor in anchors.all() {
                if let Some(event) = anchor.event {
                    events.insert(event);
                }
            }
        }
    }
    events.into_iter().collect()
}

fn prune(sessions: &mut HashMap<String, StoredSession>) {
    sessions.retain(|_, stored| stored.started.elapsed() < SESSION_TTL);
    while sessions.len() >= MAX_SESSIONS {
        let oldest = sessions
            .iter()
            .min_by_key(|(_, stored)| stored.started)
            .map(|(id, _)| id.clone());
        match oldest {
            Some(id) => {
                sessions.remove(&id);
            }
            None => break,
        }
    }
}

async fn validate(
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

async fn parse_scenario(
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

async fn validate_scenario_json(
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

async fn render_scenario(
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

async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: String,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }

    let validated = match callcraft_core::validate_yaml(&body, false) {
        Ok(validated) => validated,
        Err(error) => return error_response(StatusCode::BAD_REQUEST, &error.to_string()),
    };

    let scenario = validated.scenario;
    let events = collect_events(&scenario);
    let session = Session::new(&scenario, "api", now_iso());
    let engine = CallerEngine::new(&scenario);
    let session_id = uuid::Uuid::new_v4().to_string();

    {
        let mut sessions = state.sessions.lock().expect("sessions lock");
        prune(&mut sessions);
        sessions.insert(
            session_id.clone(),
            StoredSession {
                scenario: scenario.clone(),
                session,
                engine,
                started: Instant::now(),
                finished: false,
            },
        );
    }

    Json(json!({
        "ok": true,
        "session_id": session_id,
        "scenario": summary(&scenario),
        "events": events,
        "warnings": validated.warnings,
    }))
    .into_response()
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }

    let sessions = state.sessions.lock().expect("sessions lock");
    let Some(stored) = sessions.get(&id) else {
        return error_response(StatusCode::NOT_FOUND, "session not found");
    };

    Json(SessionStateView {
        ok: true,
        scenario: summary(&stored.scenario),
        transcript: stored.session.transcript().cloned().collect(),
        events: stored.session.events().map(str::to_string).collect(),
        finished: stored.finished,
    })
    .into_response()
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }

    let mut sessions = state.sessions.lock().expect("sessions lock");
    sessions.remove(&id);
    Json(json!({"ok": true})).into_response()
}

struct TurnOutcome {
    caller: String,
    source: &'static str,
    caller_error: Option<String>,
    transcript: Vec<Utterance>,
    events: Vec<String>,
}

fn speaker_label(speaker: Speaker) -> &'static str {
    match speaker {
        Speaker::Trainee => "Trainee",
        Speaker::Caller => "Caller",
    }
}

/// Resolve one trainee utterance, optionally through the LLM decision adapter,
/// then apply it under a short lock. The model never writes caller text; it
/// selects fact ids, and rendering happens here from the fact base.
async fn produce_turn(state: &AppState, id: &str, text: &str) -> Result<TurnOutcome, Response> {
    let (scenario, transcript, events, disclosed, elapsed_s) = {
        let sessions = state.sessions.lock().expect("sessions lock");
        let Some(stored) = sessions.get(id) else {
            return Err(error_response(StatusCode::NOT_FOUND, "session not found"));
        };
        if stored.finished {
            return Err(error_response(StatusCode::CONFLICT, "session is finished"));
        }
        let transcript = stored
            .session
            .transcript()
            .map(|utterance| {
                (
                    speaker_label(utterance.speaker).to_string(),
                    utterance.text.clone(),
                )
            })
            .collect::<Vec<_>>();
        let events = stored
            .session
            .events()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let disclosed = stored
            .engine
            .disclosed_fact_ids()
            .map(str::to_string)
            .collect::<Vec<_>>();
        (
            stored.scenario.clone(),
            transcript,
            events,
            disclosed,
            stored.started.elapsed().as_secs_f64(),
        )
    };

    let mut llm_reply: Option<(String, Vec<String>)> = None;
    let mut caller_error: Option<String> = None;
    let mut drift = false;

    if state.caller.url.is_some() {
        match caller_llm::decide(
            &state.http,
            &state.caller,
            &scenario,
            &transcript,
            &events,
            &disclosed,
            elapsed_s,
            text,
        )
        .await
        {
            Ok(decision) => match caller_llm::render(&decision, &scenario, &events) {
                Ok((reply, fact_ids)) => llm_reply = Some((reply, fact_ids)),
                Err(error) => {
                    drift = error.is_invalid();
                    caller_error = Some(error.message().to_string());
                }
            },
            Err(error) => {
                drift = error.is_invalid();
                caller_error = Some(error.message().to_string());
            }
        }
    }

    let mut sessions = state.sessions.lock().expect("sessions lock");
    let Some(stored) = sessions.get_mut(id) else {
        return Err(error_response(StatusCode::NOT_FOUND, "session not found"));
    };
    if stored.finished {
        return Err(error_response(StatusCode::CONFLICT, "session is finished"));
    }
    if drift {
        stored.session.drift_rejections += 1;
    }

    let t_ms = stored.started.elapsed().as_millis() as u64;
    stored
        .session
        .add_utterance(Speaker::Trainee, text.to_string(), t_ms, Vec::new());
    let (caller, source, fact_ids) = match llm_reply {
        Some((reply, fact_ids)) => {
            for fact_id in &fact_ids {
                stored.engine.mark_disclosed(fact_id);
            }
            (reply, "llm", fact_ids)
        }
        None => {
            let reply = stored.engine.respond(text, t_ms as f64 / 1000.0);
            (reply.text, "deterministic", reply.disclosed)
        }
    };
    stored.session.add_utterance(
        Speaker::Caller,
        caller.clone(),
        t_ms.saturating_add(1),
        fact_ids,
    );

    let transcript = stored.session.transcript().cloned().collect();
    let events = stored.session.events().map(str::to_string).collect();
    Ok(TurnOutcome {
        caller,
        source,
        caller_error,
        transcript,
        events,
    })
}

async fn turn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<TurnPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let text = payload.text.trim();
    if text.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "text is empty");
    }

    let outcome = match produce_turn(&state, &id, text).await {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    Json(json!({
        "ok": true,
        "caller": outcome.caller,
        "caller_source": outcome.source,
        "caller_error": outcome.caller_error,
        "transcript": outcome.transcript,
        "events": outcome.events,
    }))
    .into_response()
}

async fn event(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<EventPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let name = payload.name.trim();
    if name.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "event name is empty");
    }

    let mut sessions = state.sessions.lock().expect("sessions lock");
    let Some(stored) = sessions.get_mut(&id) else {
        return error_response(StatusCode::NOT_FOUND, "session not found");
    };
    if stored.finished {
        return error_response(StatusCode::CONFLICT, "session is finished");
    }

    stored.session.add_event(name.to_string());
    stored.engine.note_event(name);
    let events: Vec<&str> = stored.session.events().collect();
    Json(json!({"ok": true, "events": events})).into_response()
}

async fn finish(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }

    let mut sessions = state.sessions.lock().expect("sessions lock");
    let Some(stored) = sessions.get_mut(&id) else {
        return error_response(StatusCode::NOT_FOUND, "session not found");
    };

    stored.finished = true;
    let evaluation = callcraft_core::scoring::evaluate(&stored.scenario, &stored.session);
    let log = callcraft_core::to_jsonl(&stored.session, &stored.scenario);
    let report: Report = report::build(&stored.scenario, &stored.session, evaluation, &log);

    Json(json!({"ok": true, "report": report})).into_response()
}

async fn voice_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    Json(json!({
        "ok": true,
        "stt": state.voice.stt_url.is_some(),
        "tts": state.voice.tts_url.is_some(),
        "caller": if state.caller.url.is_some() { "llm" } else { "deterministic" },
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct SttResponse {
    #[serde(default)]
    text: String,
}

async fn voice_turn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    body: Bytes,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let Some(stt_url) = state.voice.stt_url.clone() else {
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

    let transcript = match transcribe(&state.http, &state.voice, &stt_url, &mime, &body).await {
        Ok(text) => text,
        Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
    };
    let transcript = transcript.trim().to_string();
    if transcript.is_empty() {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, "no speech detected");
    }

    let outcome = match produce_turn(&state, &id, &transcript).await {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };

    let mut payload = json!({
        "ok": true,
        "transcript": transcript,
        "caller": outcome.caller.clone(),
        "caller_source": outcome.source,
        "caller_error": outcome.caller_error,
    });

    if let Some(tts_url) = state.voice.tts_url.clone() {
        match synthesize(&state.http, &state.voice, &tts_url, &outcome.caller).await {
            Ok((audio, audio_mime)) => {
                payload["audio_base64"] =
                    json!(base64::engine::general_purpose::STANDARD.encode(audio));
                payload["audio_mime"] = json!(audio_mime);
            }
            Err(error) => {
                payload["audio_error"] = json!(error);
            }
        }
    }

    Json(payload).into_response()
}

async fn transcribe(
    client: &reqwest::Client,
    config: &VoiceConfig,
    base_url: &str,
    mime: &str,
    audio: &Bytes,
) -> Result<String, String> {
    let part = multipart::Part::bytes(audio.to_vec())
        .file_name("turn.webm")
        .mime_str(mime)
        .map_err(|error| format!("audio part: {error}"))?;
    let mut form = multipart::Form::new()
        .part("file", part)
        .text("model", config.stt_model.clone());
    if let Some(language) = &config.stt_language {
        form = form.text("language", language.clone());
    }

    let mut request = client
        .post(format!("{base_url}/v1/audio/transcriptions"))
        .multipart(form);
    if let Some(token) = &config.adapter_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("speech-to-text request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "speech-to-text returned {status}: {}",
            snippet(&body)
        ));
    }
    let payload: SttResponse = response
        .json()
        .await
        .map_err(|error| format!("speech-to-text response: {error}"))?;
    Ok(payload.text)
}

async fn synthesize(
    client: &reqwest::Client,
    config: &VoiceConfig,
    base_url: &str,
    text: &str,
) -> Result<(Vec<u8>, String), String> {
    let mut request = client
        .post(format!("{base_url}/v1/audio/speech"))
        .json(&json!({
            "model": config.tts_model.clone(),
            "input": text,
            "voice": config.tts_voice.clone(),
            "response_format": "wav",
        }));
    if let Some(token) = &config.adapter_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("text-to-speech request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "text-to-speech returned {status}: {}",
            snippet(&body)
        ));
    }
    let mime = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/wav")
        .to_string();
    let audio = response
        .bytes()
        .await
        .map_err(|error| format!("text-to-speech body: {error}"))?
        .to_vec();
    Ok((audio, mime))
}

fn snippet(text: &str) -> String {
    text.chars().take(200).collect()
}

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
