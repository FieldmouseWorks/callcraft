use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use callcraft_core::report;
use callcraft_core::scenario::Scenario;
use callcraft_core::session::{Session, Speaker, Utterance};
use callcraft_core::{CallerEngine, report::Report};

use crate::caller_llm;

use crate::auth::{TokenQuery, auth_guard, error_response};
use crate::util::now_iso;
use crate::{AppState, StoredSession};

const SESSION_TTL: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_SESSIONS: usize = 64;

#[derive(Debug, Deserialize)]
pub(crate) struct TurnPayload {
    text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventPayload {
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

pub(crate) async fn create_session(
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
                caller_failures: 0,
                caller_disabled: false,
                turn_lock: Arc::new(tokio::sync::Mutex::new(())),
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

pub(crate) async fn get_session(
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

pub(crate) async fn delete_session(
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

pub(crate) struct TurnOutcome {
    pub(crate) caller: String,
    pub(crate) source: &'static str,
    pub(crate) caller_error: Option<String>,
    pub(crate) transcript: Vec<Utterance>,
    pub(crate) events: Vec<String>,
}

pub(crate) fn record_latency(state: &AppState, id: &str, stage: &str, ms: u64) {
    let mut sessions = state.sessions.lock().expect("sessions lock");
    if let Some(stored) = sessions.get_mut(id) {
        stored.session.add_latency(stage.to_string(), ms);
    }
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
pub(crate) async fn produce_turn(
    state: &AppState,
    id: &str,
    text: &str,
) -> Result<TurnOutcome, Box<Response>> {
    let turn_lock = {
        let sessions = state.sessions.lock().expect("sessions lock");
        let Some(stored) = sessions.get(id) else {
            return Err(Box::new(error_response(
                StatusCode::NOT_FOUND,
                "session not found",
            )));
        };
        stored.turn_lock.clone()
    };
    let _turn_guard = turn_lock.lock().await;

    let (scenario, transcript, events, disclosed, elapsed_s, caller_disabled) = {
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
            stored.caller_disabled,
        )
    };
    let first_caller_turn = transcript
        .iter()
        .all(|(speaker, _)| speaker.as_str() != "Caller");

    let mut llm_reply: Option<(String, Vec<String>)> = None;
    let mut caller_error: Option<String> = None;
    let mut decision_drift = false;
    let mut adapter_failure = false;
    let mut transport_failure = false;

    let caller_config = state.caller.lock().expect("caller lock").clone();
    if caller_config.is_enabled() && !caller_disabled {
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
            Ok(decision) => match caller_llm::render(
                &decision,
                &scenario,
                &events,
                text,
                &disclosed,
                elapsed_s,
                first_caller_turn,
            ) {
                Ok((reply, fact_ids)) => llm_reply = Some((reply, fact_ids)),
                Err(error) => {
                    decision_drift = error.is_decision();
                    adapter_failure = error.is_adapter();
                    transport_failure = error.is_transport();
                    caller_error = Some(error.message().to_string());
                }
            },
            Err(error) => {
                decision_drift = error.is_decision();
                adapter_failure = error.is_adapter();
                transport_failure = error.is_transport();
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
    if decision_drift {
        stored.session.drift_rejections += 1;
    }
    if adapter_failure {
        stored.session.adapter_failures += 1;
    }
    if transport_failure {
        stored.caller_failures = stored.caller_failures.saturating_add(1);
        if stored.caller_failures >= 3 {
            stored.caller_disabled = true;
            caller_error =
                Some("caller provider disabled after repeated transport failures".to_string());
        }
    }
    if llm_reply.is_some() {
        stored.caller_failures = 0;
        stored.caller_disabled = false;
    }

    let t_ms = stored.started.elapsed().as_millis() as u64;
    stored
        .session
        .add_utterance(Speaker::Trainee, text.to_string(), t_ms, Vec::new());
    let (caller, source, fact_ids) = match llm_reply {
        Some((reply, fact_ids)) => {
            stored.engine.mark_volunteered();
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

pub(crate) async fn turn(
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

    let started = Instant::now();
    let outcome = match produce_turn(&state, &id, text).await {
        Ok(outcome) => outcome,
        Err(response) => return *response,
    };
    record_latency(&state, &id, "turn", started.elapsed().as_millis() as u64);
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

pub(crate) async fn event(
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

pub(crate) async fn finish(
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
