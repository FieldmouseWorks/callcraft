use std::path::PathBuf;

use axum::extract::Path;
use axum::http::header;
use axum::response::IntoResponse;
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

async fn mock_stt() -> Json<Value> {
    Json(json!({"text": "Where are you located?"}))
}

async fn mock_eleven_stt() -> Json<Value> {
    Json(json!({"text": "callcraft speech test"}))
}

async fn mock_tts() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "audio/wav")], b"RIFFmock".to_vec())
}

async fn mock_eleven_tts(Path(_voice): Path<String>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "audio/mpeg")], b"ID3mock".to_vec())
}

async fn mock_deepgram() -> Json<Value> {
    Json(json!({
        "results": {
            "channels": [{
                "alternatives": [{"transcript": "callcraft speech test"}]
            }]
        }
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

fn speech_payload(kind: &str, provider_id: &str, base: &str, key: Option<&str>) -> Value {
    let mut payload = json!({
        "kind": kind,
        "provider_id": provider_id,
        "base_url": base,
        "model": "mock",
        "language": "en",
    });
    if kind == "tts" {
        payload["voice"] = json!("mock-voice");
    }
    if let Some(key) = key {
        payload["api_key"] = json!(key);
    }
    payload
}

#[test]
fn speech_registry_and_provider_tests() {
    let mock_port = spawn(
        Router::new()
            .route("/v1/audio/transcriptions", post(mock_stt))
            .route("/v1/audio/speech", post(mock_tts))
            .route("/v1/speech-to-text", post(mock_eleven_stt))
            .route("/v1/text-to-speech/{voice}", post(mock_eleven_tts))
            .route("/v1/listen", post(mock_deepgram)),
    );
    let base_mock = format!("http://127.0.0.1:{mock_port}");
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SettingsStore::with_path(dir.path().join("adapters.json"), false);
    let (port, _handle) = start_server("test-token", store);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let listing: Value = client
        .get(format!("{base}/api/speech/providers"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("speech providers")
        .json()
        .expect("speech JSON");
    let stt = listing["stt"].as_array().expect("stt");
    let tts = listing["tts"].as_array().expect("tts");
    assert!(stt.iter().any(|provider| provider["id"] == "deepgram"));
    assert!(tts.iter().any(|provider| provider["id"] == "elevenlabs"));

    let cases = [
        ("stt", "openai-compatible", None, "transcript"),
        ("stt", "elevenlabs", Some("test-key"), "transcript"),
        ("stt", "deepgram", Some("test-key"), "transcript"),
        ("tts", "openai-compatible", None, "audio_base64"),
        ("tts", "elevenlabs", Some("test-key"), "audio_base64"),
    ];

    for (kind, provider, key, field) in cases {
        let result: Value = client
            .post(format!("{base}/api/speech/test"))
            .header("X-Callcraft-Token", "test-token")
            .json(&speech_payload(kind, provider, &base_mock, key))
            .send()
            .expect("speech test")
            .json()
            .expect("speech test JSON");
        assert_eq!(
            result["ok"],
            json!(true),
            "{kind}/{provider} failed: {result}"
        );
        assert!(
            result[field].as_str().unwrap_or_default().len() > 2,
            "{kind}/{provider} missing {field}: {result}"
        );
    }
}

#[test]
fn configure_stt_and_tts_switches_live_voice() {
    let mock_port = spawn(
        Router::new()
            .route("/v1/audio/transcriptions", post(mock_stt))
            .route("/v1/audio/speech", post(mock_tts)),
    );
    let base_mock = format!("http://127.0.0.1:{mock_port}");
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("speech.json");
    let store = SettingsStore::with_path(dir.path().join("adapters.json"), false);
    let (port, _handle) = start_server("test-token", store);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    for (kind, provider) in [("stt", "openai-compatible"), ("tts", "openai-compatible")] {
        let saved: Value = client
            .post(format!("{base}/api/speech/configure"))
            .header("X-Callcraft-Token", "test-token")
            .json(&speech_payload(kind, provider, &base_mock, None))
            .send()
            .expect("configure speech")
            .json()
            .expect("configure JSON");
        assert_eq!(saved["ok"], json!(true), "{kind}: {saved}");
    }
    assert!(config_path.exists(), "speech config should be saved");

    let status: Value = client
        .get(format!("{base}/api/voice/status"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("voice status")
        .json()
        .expect("status JSON");
    assert_eq!(status["stt"], json!(true));
    assert_eq!(status["tts"], json!(true));

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

    let voice: Value = client
        .post(format!("{base}/api/sessions/{session_id}/voice-turn"))
        .header("X-Callcraft-Token", "test-token")
        .header("Content-Type", "audio/webm")
        .body(vec![1u8, 2, 3, 4])
        .send()
        .expect("voice turn")
        .json()
        .expect("voice JSON");

    assert_eq!(voice["transcript"], "Where are you located?");
    assert_eq!(voice["caller"], "1234 Firwood Lane, Cedar Hollow");
    assert!(
        voice["audio_base64"].as_str().unwrap_or_default().len() > 2,
        "voice reply should include audio"
    );
}
