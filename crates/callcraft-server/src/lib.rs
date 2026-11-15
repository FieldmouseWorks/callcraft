//! Loopback-only HTTP API and static UI for Callcraft.
//!
//! The Tauri shell starts this server in-process and points its webview at it,
//! so the desktop app and the browser (`callcraft serve`) share one API, one origin
//! model and one test surface.

mod caller_llm;
pub mod providers;
pub mod speech;

pub use providers::CallerConfig;

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

use callcraft_core::report;
use callcraft_core::scenario::Scenario;
use callcraft_core::session::{Session, Speaker, Utterance};
use callcraft_core::{CallerEngine, report::Report};

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const AUTHOR_JS: &str = include_str!("../../../web/author.js");
const SETTINGS_JS: &str = include_str!("../../../web/settings.js");
const STYLES_CSS: &str = include_str!("../../../web/styles.css");
const EXAMPLE_SCENARIO: &str = include_str!("../../../examples/chest-pain-001.scenario.yaml");

const SESSION_TTL: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_SESSIONS: usize = 64;

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
    sessions: Arc<Mutex<HashMap<String, StoredSession>>>,
    speech: Arc<Mutex<speech::SpeechConfig>>,
    caller: Arc<Mutex<CallerConfig>>,
    settings: providers::SettingsStore,
    http: reqwest::Client,
}

struct StoredSession {
    scenario: Scenario,
    session: Session,
    engine: CallerEngine,
    started: Instant,
    finished: bool,
}

/// Start the loopback server on a background thread.
///
/// Port 0 asks the OS for an ephemeral port. Returns the actual port and a
/// join handle; the caller decides how long to keep it alive.
pub fn start(port: u16, token: String) -> anyhow::Result<(u16, std::thread::JoinHandle<()>)> {
    let settings = providers::SettingsStore::from_env();
    let caller = CallerConfig::load(&settings);
    let speech = speech::SpeechConfig::load(&settings);
    start_with_adapters_and_store(port, token, speech, caller, settings)
}

/// Start with explicit voice adapters and a deterministic caller.
pub fn start_with_config(
    port: u16,
    token: String,
    speech: speech::SpeechConfig,
) -> anyhow::Result<(u16, std::thread::JoinHandle<()>)> {
    start_with_adapters(port, token, speech, CallerConfig::default())
}

/// Start with explicit speech and caller adapter configuration.
pub fn start_with_adapters(
    port: u16,
    token: String,
    speech: speech::SpeechConfig,
    caller: CallerConfig,
) -> anyhow::Result<(u16, std::thread::JoinHandle<()>)> {
    start_with_adapters_and_store(
        port,
        token,
        speech,
        caller,
        providers::SettingsStore::from_env(),
    )
}

/// Start with explicit adapters and settings storage. Tests use this to keep
/// config files out of the real user config directory.
pub fn start_with_adapters_and_store(
    port: u16,
    token: String,
    speech: speech::SpeechConfig,
    caller: CallerConfig,
    settings: providers::SettingsStore,
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
        speech: Arc::new(Mutex::new(speech)),
        caller: Arc::new(Mutex::new(caller)),
        settings,
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
        .route("/settings.js", get(settings_js))
        .route("/styles.css", get(styles))
        .route("/api/health", get(health))
        .route("/api/example", get(example))
        .route("/api/validate", post(validate))
        .route("/api/voice/status", get(voice_status))
        .route("/api/providers", get(providers_list))
        .route("/api/providers/test", post(provider_test))
        .route("/api/providers/configure", post(provider_configure))
        .route("/api/providers/reset", post(provider_reset))
        .route("/api/speech/providers", get(speech_providers))
        .route("/api/speech/test", post(speech_test))
        .route("/api/speech/configure", post(speech_configure))
        .route("/api/speech/reset", post(speech_reset))
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

async fn settings_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        SETTINGS_JS,
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
async fn produce_turn(
    state: &AppState,
    id: &str,
    text: &str,
) -> Result<TurnOutcome, Box<Response>> {
    let (scenario, transcript, events, disclosed, elapsed_s) = {
        let sessions = state.sessions.lock().expect("sessions lock");
        let Some(stored) = sessions.get(id) else {
            return Err(Box::new(error_response(
                StatusCode::NOT_FOUND,
                "session not found",
            )));
        };
        if stored.finished {
            return Err(Box::new(error_response(
                StatusCode::CONFLICT,
                "session is finished",
            )));
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

    let caller_config = state.caller.lock().expect("caller lock").clone();
    if caller_config.is_enabled() {
        match caller_llm::decide(
            &state.http,
            &caller_config,
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
        return Err(Box::new(error_response(
            StatusCode::NOT_FOUND,
            "session not found",
        )));
    };
    if stored.finished {
        return Err(Box::new(error_response(
            StatusCode::CONFLICT,
            "session is finished",
        )));
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
        Err(response) => return *response,
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

    let transcript = match speech::transcribe(&state.http, &stt_config, &mime, &body).await {
        Ok(text) => text,
        Err(error) => return error_response(StatusCode::BAD_GATEWAY, error.message()),
    };
    let transcript = transcript.trim().to_string();
    if transcript.is_empty() {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, "no speech detected");
    }

    let outcome = match produce_turn(&state, &id, &transcript).await {
        Ok(outcome) => outcome,
        Err(response) => return *response,
    };

    let mut payload = json!({
        "ok": true,
        "transcript": transcript,
        "caller": outcome.caller.clone(),
        "caller_source": outcome.source,
        "caller_error": outcome.caller_error,
    });

    if let Some(tts_config) = speech.tts.clone() {
        match speech::synthesize(&state.http, &tts_config, &outcome.caller).await {
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
struct SpeechPayload {
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
struct SpeechResetPayload {
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
) -> Option<String> {
    let namespace = match kind {
        speech::SpeechKind::Stt => "stt",
        speech::SpeechKind::Tts => "tts",
    };
    provided
        .or_else(|| state.settings.load_key_ns(namespace, provider_id))
        .or_else(|| {
            let speech = state.speech.lock().expect("speech lock");
            match kind {
                speech::SpeechKind::Stt => speech
                    .stt
                    .as_ref()
                    .filter(|config| config.provider_id.as_deref() == Some(provider_id))
                    .and_then(|config| config.api_key.clone()),
                speech::SpeechKind::Tts => speech
                    .tts
                    .as_ref()
                    .filter(|config| config.provider_id.as_deref() == Some(provider_id))
                    .and_then(|config| config.api_key.clone()),
            }
        })
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

async fn speech_providers(
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

async fn speech_test(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<SpeechPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let provided = provided_speech_key(&payload);
    let api_key = speech_key(&state, payload.kind, &payload.provider_id, provided);
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
            match speech::transcribe(&state.http, &config, "audio/wav", speech::STT_TEST_WAV).await
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

async fn speech_configure(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<SpeechPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let provided = provided_speech_key(&payload);
    let api_key = speech_key(&state, payload.kind, &payload.provider_id, provided.clone());
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
            stored.stt = Some(providers::StoredStt {
                provider_id: payload.provider_id.clone(),
                base_url: config.base_url.clone(),
                model: config.model.clone(),
                language: config.language.clone().unwrap_or_default(),
            });
            let mut speech = state.speech.lock().expect("speech lock");
            speech.stt = Some(config);
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
            stored.tts = Some(providers::StoredTts {
                provider_id: payload.provider_id.clone(),
                base_url: config.base_url.clone(),
                model: config.model.clone(),
                voice: config.voice.clone().unwrap_or_default(),
            });
            let mut speech = state.speech.lock().expect("speech lock");
            speech.tts = Some(config);
        }
    }

    if let Err(error) = state.settings.save_speech(&stored) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    Json(json!({
        "ok": true,
        "active": active_speech(&state),
    }))
    .into_response()
}

async fn speech_reset(
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

#[derive(Debug, Deserialize)]
struct ProviderPayload {
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

fn stored_api_key(state: &AppState, provider_id: &str) -> Option<String> {
    state.settings.load_key(provider_id).or_else(|| {
        let caller = state.caller.lock().expect("caller lock");
        if caller.provider_id.as_deref() == Some(provider_id) {
            caller.api_key.clone()
        } else {
            None
        }
    })
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

async fn providers_list(
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

async fn provider_test(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(payload): Json<ProviderPayload>,
) -> Response {
    if let Some(response) = auth_guard(&state, &headers, &query) {
        return response;
    }
    let api_key =
        provided_api_key(&payload).or_else(|| stored_api_key(&state, &payload.provider_id));
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
        Ok(decision) => match caller_llm::render(&decision, &scenario, &[]) {
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

async fn provider_configure(
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
        .or_else(|| stored_api_key(&state, &payload.provider_id));
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
    Json(json!({
        "ok": true,
        "active": active_provider(&state),
        "keyring": state.settings.keyring_enabled(),
    }))
    .into_response()
}

async fn provider_reset(
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

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
