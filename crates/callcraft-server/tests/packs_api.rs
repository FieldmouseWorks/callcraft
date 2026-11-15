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

    let approved: Value = client
        .post(format!(
            "{base}/api/packs/local/scenarios/chest-pain-001/approve"
        ))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({"by": "test trainer"}))
        .send()
        .expect("approve")
        .json()
        .expect("approve JSON");
    assert_eq!(approved["scenario"]["status"], "approved");

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
    assert_eq!(started["approval"], "approved");
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

#[test]
fn pack_export_and_import_round_trip_through_the_api() {
    let dir_a = tempfile::tempdir().expect("tempdir a");
    let (port_a, _handle_a) = start_with_adapters_store_and_packs(
        0,
        "test-token".to_string(),
        SpeechConfig::default(),
        CallerConfig::default(),
        SettingsStore::with_path(dir_a.path().join("adapters.json"), false),
        PackStore::with_root(dir_a.path().join("packs")),
    )
    .expect("server a");
    let dir_b = tempfile::tempdir().expect("tempdir b");
    let (port_b, _handle_b) = start_with_adapters_store_and_packs(
        0,
        "test-token".to_string(),
        SpeechConfig::default(),
        CallerConfig::default(),
        SettingsStore::with_path(dir_b.path().join("adapters.json"), false),
        PackStore::with_root(dir_b.path().join("packs")),
    )
    .expect("server b");
    let client = reqwest::blocking::Client::new();
    let scenario = read("examples/chest-pain-001.scenario.yaml");

    client
        .post(format!("http://127.0.0.1:{port_a}/api/packs/scenarios"))
        .header("X-Callcraft-Token", "test-token")
        .body(scenario)
        .send()
        .expect("save a")
        .error_for_status()
        .expect("save status");

    let export = client
        .post(format!("http://127.0.0.1:{port_a}/api/packs/local/export"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("export");
    assert_eq!(export.status(), reqwest::StatusCode::OK);
    assert_eq!(
        export
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/zip")
    );
    let bytes = export.bytes().expect("zip bytes").to_vec();

    let inspect: Value = client
        .post(format!(
            "http://127.0.0.1:{port_b}/api/packs/import/inspect"
        ))
        .header("X-Callcraft-Token", "test-token")
        .header("Content-Type", "application/zip")
        .body(bytes.clone())
        .send()
        .expect("inspect")
        .json()
        .expect("inspect JSON");
    assert_eq!(inspect["report"]["action"], "install");
    assert_eq!(inspect["report"]["can_import"], json!(true));

    let imported: Value = client
        .post(format!(
            "http://127.0.0.1:{port_b}/api/packs/import?mode=install"
        ))
        .header("X-Callcraft-Token", "test-token")
        .header("Content-Type", "application/zip")
        .body(bytes)
        .send()
        .expect("import")
        .json()
        .expect("import JSON");
    assert_eq!(imported["ok"], json!(true));

    let listing: Value = client
        .get(format!("http://127.0.0.1:{port_b}/api/packs"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("list b")
        .json()
        .expect("list JSON");
    assert_eq!(listing["packs"][0]["scenarios"][0]["id"], "chest-pain-001");
}

#[test]
fn draft_scenarios_cannot_start_and_identical_save_keeps_approval() {
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

    client
        .post(format!("{base}/api/packs/scenarios"))
        .header("X-Callcraft-Token", "test-token")
        .body(scenario.clone())
        .send()
        .expect("save")
        .error_for_status()
        .expect("save status");

    let rejected = client
        .post(format!("{base}/api/sessions"))
        .header("X-Callcraft-Token", "test-token")
        .header("Content-Type", "application/json")
        .json(&json!({"pack_id": "local", "scenario_id": "chest-pain-001"}))
        .send()
        .expect("unapproved start");
    assert_eq!(rejected.status(), reqwest::StatusCode::BAD_REQUEST);

    client
        .post(format!(
            "{base}/api/packs/local/scenarios/chest-pain-001/approve"
        ))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({"by": "test trainer"}))
        .send()
        .expect("approve")
        .error_for_status()
        .expect("approve status");

    // Same bytes again: approval survives a no-op save.
    let same = client
        .post(format!("{base}/api/packs/scenarios"))
        .header("X-Callcraft-Token", "test-token")
        .body(scenario.clone())
        .send()
        .expect("same save")
        .json::<Value>()
        .expect("same JSON");
    assert_eq!(same["scenario"]["status"], "approved");

    // A real edit invalidates approval.
    let changed = scenario.replace("Chest Pain — Adult, Alone at Home", "Changed title");
    let edited = client
        .post(format!("{base}/api/packs/scenarios"))
        .header("X-Callcraft-Token", "test-token")
        .body(changed)
        .send()
        .expect("changed save")
        .json::<Value>()
        .expect("changed JSON");
    assert_eq!(edited["scenario"]["status"], "draft");
}

#[test]
fn creates_and_updates_a_named_pack_and_saves_into_it() {
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

    let created: Value = client
        .post(format!("{base}/api/packs"))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({
            "id": "cedar-county",
            "name": "Cedar County training",
            "visibility": "private",
            "license": "proprietary",
            "owner_agency": "Cedar County ECC",
            "owner_contact": "Training Division"
        }))
        .send()
        .expect("create pack")
        .json()
        .expect("create JSON");
    assert_eq!(created["pack"]["id"], "cedar-county");

    let updated: Value = client
        .patch(format!("{base}/api/packs/cedar-county"))
        .header("X-Callcraft-Token", "test-token")
        .json(&json!({
            "name": "Cedar County ECC training",
            "visibility": "shareable",
            "license": "CC-BY-4.0",
            "owner_agency": "Cedar County",
            "owner_contact": null
        }))
        .send()
        .expect("update pack")
        .json()
        .expect("update JSON");
    assert_eq!(updated["pack"]["name"], "Cedar County ECC training");
    assert_eq!(updated["pack"]["visibility"], "shareable");

    let saved: Value = client
        .post(format!("{base}/api/packs/cedar-county/scenarios"))
        .header("X-Callcraft-Token", "test-token")
        .body(scenario)
        .send()
        .expect("save to named pack")
        .json()
        .expect("save JSON");
    assert_eq!(saved["pack_id"], "cedar-county");

    let listing: Value = client
        .get(format!("{base}/api/packs"))
        .header("X-Callcraft-Token", "test-token")
        .send()
        .expect("list packs")
        .json()
        .expect("list JSON");
    let pack = listing["packs"]
        .as_array()
        .expect("packs")
        .iter()
        .find(|pack| pack["id"] == "cedar-county")
        .expect("named pack");
    assert_eq!(pack["scenarios"][0]["id"], "chest-pain-001");
}
