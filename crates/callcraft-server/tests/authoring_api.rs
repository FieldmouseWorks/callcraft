use std::path::PathBuf;

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
fn authoring_round_trip() {
    let token = "test-token";
    let (port, _handle) = callcraft_server::start(0, token.to_string()).expect("server starts");
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let yaml = read("examples/chest-pain-001.scenario.yaml");

    let parsed: Value = client
        .post(format!("{base}/api/scenario/parse"))
        .header("X-Callcraft-Token", token)
        .body(yaml)
        .send()
        .expect("parse request")
        .json()
        .expect("parse JSON");
    assert_eq!(parsed["ok"], json!(true));
    let scenario = parsed["scenario"].clone();
    assert_eq!(scenario["id"], "chest-pain-001");
    assert_eq!(scenario["facts"].as_array().expect("facts").len(), 13);

    let validated: Value = client
        .post(format!("{base}/api/scenario/validate"))
        .header("X-Callcraft-Token", token)
        .json(&scenario)
        .send()
        .expect("validate request")
        .json()
        .expect("validate JSON");
    assert_eq!(validated["ok"], json!(true));

    let rendered: Value = client
        .post(format!("{base}/api/scenario/render"))
        .header("X-Callcraft-Token", token)
        .json(&scenario)
        .send()
        .expect("render request")
        .json()
        .expect("render JSON");
    assert_eq!(rendered["ok"], json!(true));
    let rendered_yaml = rendered["yaml"].as_str().expect("rendered yaml");
    assert!(rendered_yaml.contains("chest-pain-001"));
    assert!(rendered_yaml.contains("format"));

    let reparsed: Value = client
        .post(format!("{base}/api/scenario/parse"))
        .header("X-Callcraft-Token", token)
        .body(rendered_yaml.to_string())
        .send()
        .expect("reparse request")
        .json()
        .expect("reparse JSON");
    assert_eq!(reparsed["scenario"]["id"], "chest-pain-001");
    assert_eq!(
        reparsed["scenario"]["rubric"]["items"]
            .as_array()
            .expect("rubric items")
            .len(),
        8
    );

    let mut broken = scenario;
    broken.as_object_mut().expect("object").remove("rubric");
    let response = client
        .post(format!("{base}/api/scenario/validate"))
        .header("X-Callcraft-Token", token)
        .json(&broken)
        .send()
        .expect("broken validate request");
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}
