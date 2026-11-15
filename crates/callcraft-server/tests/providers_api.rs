use std::path::PathBuf;

use axum::routing::post;
use axum::{Json, Router};
use callcraft_server::providers::SettingsStore;
use callcraft_server::speech::SpeechConfig;
use callcraft_server::{CallerConfig, start_with_adapters_and_store};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative)).expect("fixture read")
}

fn spawn(app: Router) -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("mock bind");
    listener.set_nonblocking(true).expect("mock nonblocking");
    let port = listener.local_addr().expect("mock addr").port();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("mock runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("mock listener");
            axum::serve(listener, app).await.expect("mock server");
        });
    });
    port
}

async fn chat_decision() -> Json<Value> {
    Json(json!({
        "choices": [{
            "message": {
                "content": "{\"fact_ids\":[\"address\"],\"style\":\"neutral\"}"
            }
        }]
    }))
}

async fn anthropic_decision() -> Json<Value> {
    Json(json!({
        "content": [{
            "type": "text",
            "text": "{\"fact_ids\":[\"address\"],\"style\":\"neutral\"}"
        }]
    }))
}

fn start_server(token: &str, store: SettingsStore) -> (u16, std::thread::JoinHandle<()>) {
    start_with_adapters_and_store(
        0,
        token.to_string(),
        SpeechConfig::default(),
        CallerConfig::default(),
        store,
    )
    .expect("server starts")
}

#[test]
fn provider_registry_exposes_auth_reality() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SettingsStore::with_path(dir.path().join("adapters.json"), false);
    let (port, _handle) = start_server("test-token", store);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let listing: Value = client
        .get(format!("{base}/api/providers"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("providers request")
        .json()
        .expect("providers JSON");

    let providers = listing["providers"].as_array().expect("providers");
    let anthropic = providers
        .iter()
        .find(|provider| provider["id"] == "anthropic")
        .expect("anthropic");
    let openai = providers
        .iter()
        .find(|provider| provider["id"] == "openai")
        .expect("openai");
    assert_eq!(anthropic["subscription_oauth"], "prohibited");
    assert_eq!(openai["subscription_oauth"], "partner-gated");
    assert_eq!(listing["keyring"], json!(false));
    assert_eq!(listing["active"]["enabled"], json!(false));
}

#[test]
fn provider_test_supports_openai_compatible_and_anthropic_shapes() {
    let chat_port = spawn(Router::new().route("/v1/chat/completions", post(chat_decision)));
    let messages_port = spawn(Router::new().route("/v1/messages", post(anthropic_decision)));
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SettingsStore::with_path(dir.path().join("adapters.json"), false);
    let (port, _handle) = start_server("test-token", store);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let openai: Value = client
        .post(format!("{base}/api/providers/test"))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({
            "provider_id": "openai-compatible",
            "base_url": format!("http://127.0.0.1:{chat_port}"),
            "model": "mock"
        }))
        .send()
        .expect("openai test")
        .json()
        .expect("openai test JSON");
    assert_eq!(openai["ok"], json!(true));
    assert_eq!(openai["caller"], "1234 Firwood Lane, Cedar Hollow");

    let anthropic: Value = client
        .post(format!("{base}/api/providers/test"))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({
            "provider_id": "anthropic",
            "base_url": format!("http://127.0.0.1:{messages_port}"),
            "model": "claude-test",
            "api_key": "test-key"
        }))
        .send()
        .expect("anthropic test")
        .json()
        .expect("anthropic test JSON");
    assert_eq!(anthropic["ok"], json!(true));
    assert_eq!(anthropic["caller"], "1234 Firwood Lane, Cedar Hollow");

    let missing_key = client
        .post(format!("{base}/api/providers/test"))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({
            "provider_id": "anthropic",
            "base_url": "http://127.0.0.1:1",
            "model": "claude-test"
        }))
        .send()
        .expect("missing key test");
    assert_eq!(missing_key.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[test]
fn configure_switches_the_live_caller() {
    let chat_port = spawn(Router::new().route("/v1/chat/completions", post(chat_decision)));
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("adapters.json");
    let store = SettingsStore::with_path(config_path.clone(), false);
    let (port, _handle) = start_server("test-token", store);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let saved: Value = client
        .post(format!("{base}/api/providers/configure"))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({
            "provider_id": "openai-compatible",
            "base_url": format!("http://127.0.0.1:{chat_port}"),
            "model": "mock"
        }))
        .send()
        .expect("configure")
        .json()
        .expect("configure JSON");
    assert_eq!(saved["ok"], json!(true));
    assert_eq!(saved["active"]["enabled"], json!(true));
    assert!(config_path.exists(), "non-secret config should be saved");

    let scenario = read("examples/chest-pain-001.scenario.yaml");
    let created: Value = client
        .post(format!("{base}/api/sessions"))
        .header("X-Callcraft-Token", "test-token")
        .body(scenario)
        .send()
        .expect("create session")
        .json()
        .expect("create JSON");
    let session_id = created["session_id"].as_str().expect("session id");

    let turn: Value = client
        .post(format!("{base}/api/sessions/{session_id}/turns"))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({"text": "Where are you located?"}))
        .send()
        .expect("turn")
        .json()
        .expect("turn JSON");
    assert_eq!(turn["caller_source"], "llm");
    assert_eq!(turn["caller"], "1234 Firwood Lane, Cedar Hollow");
}
