use serde::Serialize;

use crate::eventlog::session_id;
use crate::scenario::{FORMAT_VERSION, Scenario};
use crate::scoring::Evaluation;
use crate::session::{Session, Utterance};

#[derive(Debug, Serialize)]
pub struct Cost {
    pub stt_usd: f64,
    pub llm_usd: f64,
    pub tts_usd: f64,
    pub total_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct Latency {
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub format_version: String,
    pub scenario_id: String,
    pub scenario_version: String,
    pub session_id: String,
    pub mode: String,
    pub started_at: String,
    pub transcript: Vec<Utterance>,
    pub events: Vec<String>,
    pub evaluation: Evaluation,
    pub cost: Cost,
    pub latency_ms: Latency,
}

pub fn build(
    scenario: &Scenario,
    session: &Session,
    evaluation: Evaluation,
    log_jsonl: &str,
) -> Report {
    Report {
        format_version: FORMAT_VERSION.to_string(),
        scenario_id: scenario.id.clone(),
        scenario_version: scenario.version.clone(),
        session_id: session_id(log_jsonl),
        mode: session.mode.clone(),
        started_at: session.started_at.clone(),
        transcript: session.transcript().cloned().collect(),
        events: session.events().map(str::to_string).collect(),
        evaluation,
        cost: Cost {
            stt_usd: 0.0,
            llm_usd: 0.0,
            tts_usd: 0.0,
            total_usd: 0.0,
        },
        latency_ms: Latency {
            p50_ms: None,
            p95_ms: None,
        },
    }
}

pub fn to_json(report: &Report) -> String {
    serde_json::to_string_pretty(report).expect("report serializes")
}
