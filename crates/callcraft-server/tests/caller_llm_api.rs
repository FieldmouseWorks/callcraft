use std::path::PathBuf;

use axum::routing::post;
use axum::{Json, Router};
use callcraft_server::providers::config_from_request;
use callcraft_server::speech::SpeechConfig;
use callcraft_server::start_with_adapters;
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

async fn chat_selects_address() -> Json<Value> {
    Json(json!({
        "choices": [{
            "message": {
                "content": "{\"fact_ids\":[\"address\"],\"style\":\"anxious\"}"
            }
        }]
    }))
}

async fn chat_invents_fact() -> Json<Value> {
    Json(json!({
        "choices": [{
            "message": {
                "content": "{\"fact_ids\":[\"invented_fact\"],\"style\":\"neutral\"}"
            }
        }]
    }))
}

fn create_session(client: &reqwest::blocking::Client, base: &str, token: &str) -> String {
    let scenario = read("examples/chest-pain-001.scenario.yaml");
    let created: Value = client
        .post(format!("{base}/api/sessions"))
        .header("X-Callcraft-Token", token)
        .body(scenario)
        .send()
        .expect("create session")
        .json()
        .expect("create JSON");
    created["session_id"]
        .as_str()
        .expect("session id")
        .to_string()
}

fn start_with_mock(chat_port: u16) -> (u16, String) {
    let token = "test-token".to_string();
    let caller = config_from_request(
        "openai-compatible",
        Some(format!("http://127.0.0.1:{chat_port}")),
        Some("mock".to_string()),
        None,
    )
    .expect("caller config");
    let (port, _handle) = start_with_adapters(0, token.clone(), SpeechConfig::default(), caller)
        .expect("server starts");
    (port, token)
}

#[test]
fn llm_caller_selects_an_authored_fact() {
    let chat_port = spawn(Router::new().route("/v1/chat/completions", post(chat_selects_address)));
    let (port, token) = start_with_mock(chat_port);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let status: Value = client
        .get(format!("{base}/api/voice/status"))
        .header("X-Callcraft-Token", &token)
        .send()
        .expect("status request")
        .json()
        .expect("status JSON");
    assert_eq!(status["caller"], "llm");

    let session_id = create_session(&client, &base, &token);
    let response: Value = client
        .post(format!("{base}/api/sessions/{session_id}/turns"))
        .header("X-Callcraft-Token", &token)
        .json(&json!({"text": "Where are you located?"}))
        .send()
        .expect("turn request")
        .json()
        .expect("turn JSON");

    assert_eq!(response["caller_source"], "llm");
    assert_eq!(
        response["caller"],
        "1234 Firwood Lane, Cedar Hollow. Please hurry."
    );
    assert!(response["caller_error"].is_null());
}

#[test]
fn invalid_llm_choice_falls_back_and_counts_drift() {
    let chat_port = spawn(Router::new().route("/v1/chat/completions", post(chat_invents_fact)));
    let (port, token) = start_with_mock(chat_port);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let session_id = create_session(&client, &base, &token);
    let response: Value = client
        .post(format!("{base}/api/sessions/{session_id}/turns"))
        .header("X-Callcraft-Token", &token)
        .json(&json!({"text": "Where are you located?"}))
        .send()
        .expect("turn request")
        .json()
        .expect("turn JSON");

    assert_eq!(response["caller_source"], "deterministic");
    assert_eq!(response["caller"], "1234 Firwood Lane, Cedar Hollow");
    assert!(
        response["caller_error"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown fact")
    );

    let finished: Value = client
        .post(format!("{base}/api/sessions/{session_id}/finish"))
        .header("X-Callcraft-Token", &token)
        .send()
        .expect("finish request")
        .json()
        .expect("finish JSON");
    assert_eq!(
        finished["report"]["evaluation"]["drift_rejections"],
        json!(1)
    );
}
