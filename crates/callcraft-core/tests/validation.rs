use std::path::PathBuf;

use callcraft_core::validate::validate_yaml;

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
fn valid_fixtures_validate() {
    for relative in [
        "examples/chest-pain-001.scenario.yaml",
        "fixtures/valid/minimal.yaml",
    ] {
        let text = read(relative);
        let validated = validate_yaml(&text, false).expect(relative);
        assert!(!validated.scenario.id.is_empty());
    }
}

#[test]
fn invalid_fixtures_are_rejected() {
    for relative in [
        "fixtures/invalid/bad-format.yaml",
        "fixtures/invalid/missing-rubric.yaml",
        "fixtures/invalid/duplicate-fact-ids.yaml",
        "fixtures/invalid/unknown-utterance-fact.yaml",
        "fixtures/invalid/yaml-deep-nesting.yaml",
        "fixtures/invalid/yaml-alias-bomb.yaml",
    ] {
        let text = read(relative);
        assert!(
            validate_yaml(&text, false).is_err(),
            "{relative} should fail"
        );
    }
}

#[test]
fn content_policy_warns_and_strict_rejects() {
    let text = read("fixtures/invalid/real-phone.yaml");
    let validated = validate_yaml(&text, false).expect("schema-valid, policy-warning");
    assert!(
        validated
            .warnings
            .iter()
            .any(|warning| warning.contains("555-01xx")),
        "expected a content-policy warning"
    );

    let error = validate_yaml(&text, true).expect_err("strict mode rejects");
    assert!(error.to_string().contains("555-01xx"));
}
