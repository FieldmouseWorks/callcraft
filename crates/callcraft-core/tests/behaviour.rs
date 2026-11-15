use std::path::PathBuf;

use callcraft_core::report;
use callcraft_core::scoring::{self, ItemStatus};
use callcraft_core::session::{Script, parse_script, run_scripted};
use callcraft_core::{CallerEngine, Scenario, replay, to_jsonl};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative)).expect("fixture read")
}

fn scenario() -> Scenario {
    callcraft_core::validate::validate_yaml(&read("examples/chest-pain-001.scenario.yaml"), false)
        .expect("example validates")
        .scenario
}

fn script(relative: &str) -> Script {
    parse_script(&read(relative)).expect("script parses")
}

fn started_at() -> String {
    "2026-11-15T00:00:00Z".to_string()
}

#[test]
fn caller_discloses_asked_fact_only() {
    let scenario = scenario();
    let mut engine = CallerEngine::new(&scenario);

    let address = engine.respond("Where are you located?", 0.0);
    assert_eq!(address.text, "1234 Firwood Lane, Cedar Hollow");
    assert_eq!(address.disclosed, vec!["address".to_string()]);

    let mut engine = CallerEngine::new(&scenario);
    let callback = engine.respond("What's the best number to reach you?", 0.0);
    assert_eq!(callback.text, "555-0142");
    assert!(!callback.text.contains("Firwood"));
}

#[test]
fn good_call_passes_and_bad_call_fails() {
    let scenario = scenario();

    let good = run_scripted(&scenario, &script("scripts/good-call.yaml"), started_at())
        .expect("good script runs");
    let good_eval = scoring::evaluate(&scenario, &good);
    assert_eq!(good_eval.overall, ItemStatus::Pass);
    assert!(good_eval.critical_failures.is_empty());

    let bad = run_scripted(&scenario, &script("scripts/bad-call.yaml"), started_at())
        .expect("bad script runs");
    let bad_eval = scoring::evaluate(&scenario, &bad);
    assert_eq!(bad_eval.overall, ItemStatus::Fail);
    assert!(
        bad_eval
            .critical_failures
            .contains(&"callback_verified".to_string()),
        "callback_verified should be a critical failure, got {:?}",
        bad_eval.critical_failures
    );
}

#[test]
fn replay_produces_byte_identical_report() {
    let scenario = scenario();
    let session = run_scripted(&scenario, &script("scripts/good-call.yaml"), started_at())
        .expect("script runs");
    let log = to_jsonl(&session, &scenario);

    let first_eval = scoring::evaluate(&scenario, &session);
    let first_report = report::build(&scenario, &session, first_eval, &log);
    let first_json = report::to_json(&first_report);

    let replayed = replay(&log).expect("log replays");
    let second_eval = scoring::evaluate(&scenario, &replayed);
    let second_report = report::build(&scenario, &replayed, second_eval, &log);
    let second_json = report::to_json(&second_report);

    assert_eq!(first_json, second_json);
}
