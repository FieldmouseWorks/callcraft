//! Loopback-only HTTP API and static UI for Callcraft.
//!
//! The Tauri shell starts this server in-process and points its webview at it,
//! so the desktop app and the browser (`callcraft serve`) share one API, one origin
//! model and one test surface.

mod auth;
mod caller_llm;
mod handlers;
pub mod providers;
pub mod speech;
mod util;

pub use providers::CallerConfig;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use callcraft_core::CallerEngine;
use callcraft_core::scenario::Scenario;
use callcraft_core::session::Session;

const EXAMPLE_SCENARIO: &str = include_str!("../../../examples/chest-pain-001.scenario.yaml");

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
    caller_failures: u32,
    caller_disabled: bool,
    turn_lock: Arc<tokio::sync::Mutex<()>>,
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
        .route("/", get(handlers::static_files::index))
        .route("/app.js", get(handlers::static_files::app_js))
        .route("/author.js", get(handlers::static_files::author_js))
        .route("/settings.js", get(handlers::static_files::settings_js))
        .route("/styles.css", get(handlers::static_files::styles))
        .route("/api/health", get(handlers::static_files::health))
        .route("/api/example", get(handlers::static_files::example))
        .route("/api/validate", post(handlers::scenario::validate))
        .route("/api/voice/status", get(handlers::speech::voice_status))
        .route("/api/providers", get(handlers::providers::providers_list))
        .route(
            "/api/providers/test",
            post(handlers::providers::provider_test),
        )
        .route(
            "/api/providers/configure",
            post(handlers::providers::provider_configure),
        )
        .route(
            "/api/providers/reset",
            post(handlers::providers::provider_reset),
        )
        .route(
            "/api/speech/providers",
            get(handlers::speech::speech_providers),
        )
        .route("/api/speech/test", post(handlers::speech::speech_test))
        .route(
            "/api/speech/configure",
            post(handlers::speech::speech_configure),
        )
        .route("/api/speech/reset", post(handlers::speech::speech_reset))
        .route(
            "/api/scenario/parse",
            post(handlers::scenario::parse_scenario),
        )
        .route(
            "/api/scenario/validate",
            post(handlers::scenario::validate_scenario_json),
        )
        .route(
            "/api/scenario/render",
            post(handlers::scenario::render_scenario),
        )
        .route("/api/sessions", post(handlers::sessions::create_session))
        .route(
            "/api/sessions/{id}",
            get(handlers::sessions::get_session).delete(handlers::sessions::delete_session),
        )
        .route("/api/sessions/{id}/turns", post(handlers::sessions::turn))
        .route("/api/sessions/{id}/events", post(handlers::sessions::event))
        .route(
            "/api/sessions/{id}/finish",
            post(handlers::sessions::finish),
        )
        .route(
            "/api/sessions/{id}/voice-turn",
            post(handlers::speech::voice_turn),
        )
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .with_state(state)
}
