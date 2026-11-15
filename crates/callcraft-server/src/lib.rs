//! Loopback-only HTTP API and static UI for Callcraft.
//!
//! The Tauri shell starts this server in-process and points its webview at it,
//! so the desktop app and the browser (`callcraft serve`) share one API, one origin
//! model and one test surface.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    let actual_port = listener.local_addr()?.port();
    let state = AppState {
        token: Arc::new(token),
        sessions: Arc::new(Mutex::new(HashMap::new())),
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

    let mut sessions = state.sessions.lock().expect("sessions lock");
    let Some(stored) = sessions.get_mut(&id) else {
        return error_response(StatusCode::NOT_FOUND, "session not found");
    };
    if stored.finished {
        return error_response(StatusCode::CONFLICT, "session is finished");
    }

    let t_ms = stored.started.elapsed().as_millis() as u64;
    stored
        .session
        .add_utterance(Speaker::Trainee, text.to_string(), t_ms, Vec::new());
    let reply = stored.engine.respond(text, t_ms as f64 / 1000.0);
    stored.session.add_utterance(
        Speaker::Caller,
        reply.text.clone(),
        t_ms.saturating_add(1),
        reply.disclosed,
    );

    let transcript: Vec<&Utterance> = stored.session.transcript().collect();
    let events: Vec<&str> = stored.session.events().collect();
    Json(json!({
        "ok": true,
        "caller": reply.text,
        "transcript": transcript,
        "events": events,
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

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
