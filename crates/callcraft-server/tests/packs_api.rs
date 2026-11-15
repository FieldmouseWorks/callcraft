use std::path::PathBuf;

use callcraft_server::packs::PackStore;
use callcraft_server::providers::SettingsStore;
use callcraft_server::speech::SpeechConfig;
use callcraft_server::{CallerConfig, start_with_adapters_store_and_packs};
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

#[test]
fn pack_api_saves_lists_and_starts_a_saved_scenario() {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = SettingsStore::with_path(dir.path().join("adapters.json"), false);
    let packs = PackStore::with_root(dir.path().join("packs"));
    let (port, _handle) = start_with_adapters_store_and_packs(
        0,
        "test-token".to_string(),
        SpeechConfig::default(),
        CallerConfig::default(),
        settings,
        packs,
    )
    .expect("server starts");
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();
    let scenario = read("examples/chest-pain-001.scenario.yaml");

    let saved: Value = client
        .post(format!("{base}/api/packs/scenarios"))
        .header("X-Callcraft-Token", "test-token")
        .header("Content-Type", "text/yaml")
        .body(scenario.clone())
        .send()
        .expect("save scenario")
        .json()
        .expect("save JSON");
    assert_eq!(saved["ok"], json!(true));
    assert_eq!(saved["pack_id"], "local");
    assert_eq!(saved["scenario"]["id"], "chest-pain-001");
    assert_eq!(saved["scenario"]["status"], "draft");
    assert_eq!(saved["scenario"]["critical_items"], 7);

    let listing: Value = client
        .get(format!("{base}/api/packs"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("list packs")
        .json()
        .expect("list JSON");
    assert_eq!(listing["packs"][0]["id"], "local");
    assert_eq!(listing["packs"][0]["scenarios"][0]["id"], "chest-pain-001");

    let opened: Value = client
        .get(format!("{base}/api/packs/local/scenarios/chest-pain-001"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("open scenario")
        .json()
        .expect("open JSON");
    assert!(
        opened["yaml"]
            .as_str()
            .expect("yaml")
            .contains("chest-pain-001")
    );

    let started: Value = client
        .post(format!("{base}/api/sessions"))
        .header("X-Callcraft-Token", "test-token")
        .header("Content-Type", "application/json")
        .json(&json!({
            "pack_id": "local",
            "scenario_id": "chest-pain-001"
        }))
        .send()
        .expect("start saved scenario")
        .json()
        .expect("start JSON");
    assert_eq!(started["ok"], json!(true));
    assert_eq!(started["source"], "pack");
    assert_eq!(started["pack_id"], "local");
    assert_eq!(started["scenario"]["id"], "chest-pain-001");
    assert!(!started["events"].as_array().expect("events").is_empty());
}

#[test]
fn pack_api_rejects_invalid_scenario_and_unknown_pack() {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = SettingsStore::with_path(dir.path().join("adapters.json"), false);
    let packs = PackStore::with_root(dir.path().join("packs"));
    let (port, _handle) = start_with_adapters_store_and_packs(
        0,
        "test-token".to_string(),
        SpeechConfig::default(),
        CallerConfig::default(),
        settings,
        packs,
    )
    .expect("server starts");
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let invalid = client
        .post(format!("{base}/api/packs/scenarios"))
        .header("X-Callcraft-Token", "test-token")
        .body("format: not-a-scenario")
        .send()
        .expect("invalid save");
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);

    let unknown = client
        .post(format!("{base}/api/sessions"))
        .header("X-Callcraft-Token", "test-token")
        .header("Content-Type", "application/json")
        .json(&json!({"pack_id": "missing", "scenario_id": "nope"}))
        .send()
        .expect("unknown pack start");
    assert_eq!(unknown.status(), reqwest::StatusCode::BAD_REQUEST);
}
