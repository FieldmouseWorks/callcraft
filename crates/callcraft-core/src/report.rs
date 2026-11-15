use serde::Serialize;

use crate::eventlog::session_id;
use crate::scenario::{FORMAT_VERSION, Scenario};
use crate::scoring::Evaluation;
use crate::session::{Session, StageLatency, Utterance};

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

#[derive(Debug, Clone, Serialize)]
pub struct RunContext {
    pub scenario_sha256: String,
    pub pack_id: Option<String>,
    pub pack_version: Option<String>,
    pub approval: String,
}

impl RunContext {
    pub fn direct(scenario_sha256: impl Into<String>) -> Self {
        Self {
            scenario_sha256: scenario_sha256.into(),
            pack_id: None,
            pack_version: None,
            approval: "none".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub format_version: String,
    pub scenario_id: String,
    pub scenario_version: String,
    pub scenario_sha256: String,
    pub pack_id: Option<String>,
    pub pack_version: Option<String>,
    pub approval: String,
    pub session_id: String,
    pub mode: String,
    pub started_at: String,
    pub transcript: Vec<Utterance>,
    pub events: Vec<String>,
    pub evaluation: Evaluation,
    pub stages: Vec<StageLatency>,
    pub cost: Cost,
    pub latency_ms: Latency,
}

pub fn build(
    scenario: &Scenario,
    session: &Session,
    evaluation: Evaluation,
    log_jsonl: &str,
    context: &RunContext,
) -> Report {
    let latencies = session
        .latencies()
        .map(|latency| latency.ms)
        .collect::<Vec<_>>();

    Report {
        format_version: FORMAT_VERSION.to_string(),
        scenario_id: scenario.id.clone(),
        scenario_version: scenario.version.clone(),
        scenario_sha256: context.scenario_sha256.clone(),
        pack_id: context.pack_id.clone(),
        pack_version: context.pack_version.clone(),
        approval: context.approval.clone(),
        session_id: session_id(log_jsonl),
        mode: session.mode.clone(),
        started_at: session.started_at.clone(),
        transcript: session.transcript().cloned().collect(),
        events: session.events().map(str::to_string).collect(),
        evaluation,
        stages: session.latencies().cloned().collect(),
        cost: Cost {
            stt_usd: 0.0,
            llm_usd: 0.0,
            tts_usd: 0.0,
            total_usd: 0.0,
        },
        latency_ms: Latency {
            p50_ms: percentile_ms(&latencies, 0.50),
            p95_ms: percentile_ms(&latencies, 0.95),
        },
    }
}

pub fn to_json(report: &Report) -> String {
    serde_json::to_string_pretty(report).expect("report serializes")
}

fn percentile_ms(values: &[u64], percentile: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted.get(index).copied()
}
