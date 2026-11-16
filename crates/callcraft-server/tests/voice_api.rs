use std::path::PathBuf;

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine;
use callcraft_server::speech::{SpeechConfig, stt_from_request, tts_from_request};
use callcraft_server::start_with_config;
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

async fn mock_stt() -> Json<Value> {
    Json(json!({"text": "Where are you located?"}))
}

async fn mock_tts() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "audio/wav")], b"RIFFmock".to_vec())
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

#[test]
fn voice_turn_uses_stt_and_tts() {
    let stt_port = spawn(Router::new().route("/v1/audio/transcriptions", post(mock_stt)));
    let tts_port = spawn(Router::new().route("/v1/audio/speech", post(mock_tts)));

    let token = "test-token";
    let speech = SpeechConfig {
        stt: Some(
            stt_from_request(
                "openai-compatible",
                Some(format!("http://127.0.0.1:{stt_port}")),
                Some("whisper-1".to_string()),
                Some("en".to_string()),
                None,
            )
            .expect("stt config"),
        ),
        tts: Some(
            tts_from_request(
                "openai-compatible",
                Some(format!("http://127.0.0.1:{tts_port}")),
                Some("tts-1".to_string()),
                Some("alloy".to_string()),
                None,
            )
            .expect("tts config"),
        ),
    };
    let (port, _handle) = start_with_config(0, token.to_string(), speech).expect("server starts");
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let status: Value = client
        .get(format!("{base}/api/voice/status"))
        .header("X-Callcraft-Token", token)
        .send()
        .expect("status request")
        .json()
        .expect("status JSON");
    assert_eq!(status["stt"], json!(true));
    assert_eq!(status["tts"], json!(true));

    let session_id = create_session(&client, &base, token);

    let response = client
        .post(format!("{base}/api/sessions/{session_id}/voice-turn"))
        .header("X-Callcraft-Token", token)
        .header("Content-Type", "audio/webm")
        .body(vec![1u8, 2, 3, 4])
        .send()
        .expect("voice turn");
    assert!(response.status().is_success());

    let body: Value = response.json().expect("voice JSON");
    assert_eq!(body["transcript"], "Where are you located?");
    assert_eq!(body["caller"], "1234 Firwood Lane, Cedar Hollow");
    let audio = base64::engine::general_purpose::STANDARD
        .decode(body["audio_base64"].as_str().expect("audio base64"))
        .expect("audio decodes");
    assert_eq!(audio, b"RIFFmock");
    assert_eq!(body["audio_mime"], "audio/wav");
}

#[test]
fn voice_turn_requires_configuration() {
    let token = "test-token";
    let (port, _handle) =
        start_with_config(0, token.to_string(), SpeechConfig::default()).expect("server starts");
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let status: Value = client
        .get(format!("{base}/api/voice/status"))
        .header("X-Callcraft-Token", token)
        .send()
        .expect("status request")
        .json()
        .expect("status JSON");
    assert_eq!(status["stt"], json!(false));
    assert_eq!(status["tts"], json!(false));

    let response = client
        .post(format!("{base}/api/sessions/unknown/voice-turn"))
        .header("X-Callcraft-Token", token)
        .header("Content-Type", "audio/webm")
        .body(vec![1u8, 2, 3])
        .send()
        .expect("voice turn");
    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
}
