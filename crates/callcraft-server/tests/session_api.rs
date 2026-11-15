use std::path::PathBuf;

use callcraft_core::session::parse_script;
use serde_json::json;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative)).expect("fixture read")
}

#[test]
fn session_api_scores_a_good_call() {
    let token = "test-token";
    let (port, _handle) = callcraft_server::start(0, token.to_string()).expect("server starts");
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();
    let scenario = read("examples/chest-pain-001.scenario.yaml");
    let script = parse_script(&read("scripts/good-call.yaml")).expect("script parses");

    // Unauthorized requests are rejected.
    let response = client
        .post(format!("{base}/api/sessions"))
        .body(scenario.clone())
        .send()
        .expect("request");
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    let response = client
        .post(format!("{base}/api/sessions"))
        .header("X-Callcraft-Token", token)
        .body(scenario)
        .send()
        .expect("create session");
    assert!(response.status().is_success());
    let created: serde_json::Value = response.json().expect("create JSON");
    let session_id = created["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert_eq!(created["scenario"]["id"], "chest-pain-001");
    assert!(
        created["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event.as_str() == Some("protocol_card_delivered"))
    );

    for turn in script.turns {
        if let Some(event) = turn.event {
            let response = client
                .post(format!("{base}/api/sessions/{session_id}/events"))
                .header("X-Callcraft-Token", token)
                .json(&json!({"name": event}))
                .send()
                .expect("event request");
            assert!(response.status().is_success(), "event {event}");
        } else if let Some(text) = turn.text {
            let response = client
                .post(format!("{base}/api/sessions/{session_id}/turns"))
                .header("X-Callcraft-Token", token)
                .json(&json!({"text": text}))
                .send()
                .expect("turn request");
            assert!(response.status().is_success(), "turn {text}");
            let body: serde_json::Value = response.json().expect("turn JSON");
            assert!(!body["caller"].as_str().unwrap_or_default().is_empty());
        }
    }

    let response = client
        .post(format!("{base}/api/sessions/{session_id}/finish"))
        .header("X-Callcraft-Token", token)
        .send()
        .expect("finish request");
    assert!(response.status().is_success());
    let finished: serde_json::Value = response.json().expect("finish JSON");
    assert_eq!(finished["report"]["approval"], "none");
    assert!(finished["report"]["pack_id"].is_null());
    assert_eq!(
        finished["report"]["scenario_sha256"]
            .as_str()
            .expect("scenario hash")
            .len(),
        64
    );
    assert!(
        finished["report"]["stages"]
            .as_array()
            .expect("stages")
            .iter()
            .any(|stage| stage["stage"] == "turn")
    );
    assert!(
        finished["report"]["latency_ms"]["p50_ms"]
            .as_u64()
            .is_some()
    );
    assert_eq!(finished["report"]["evaluation"]["overall"], "pass");
    assert_eq!(
        finished["report"]["evaluation"]["critical_failures"]
            .as_array()
            .expect("critical failures")
            .len(),
        0
    );
    assert_eq!(
        finished["report"]["evaluation"]["drift_rejections"],
        json!(0)
    );
}
