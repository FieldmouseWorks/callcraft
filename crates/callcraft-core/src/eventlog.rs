use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::scenario::{FORMAT_VERSION, Scenario};
use crate::session::{Session, SessionItem, Speaker};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Record {
    Header {
        format_version: String,
        scenario_id: String,
        scenario_version: String,
        mode: String,
        started_at: String,
    },
    Utterance {
        id: String,
        speaker: Speaker,
        text: String,
        t_ms: u64,
        #[serde(default)]
        fact_ids: Vec<String>,
    },
    Event {
        name: String,
    },
    Latency {
        stage: String,
        ms: u64,
    },
    End,
}

#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
    #[error("bad JSONL at line {line}: {source}")]
    Json {
        line: usize,
        source: serde_json::Error,
    },
    #[error("event log missing header")]
    MissingHeader,
    #[error("event log record before header")]
    RecordBeforeHeader,
    #[error("event log missing end marker")]
    MissingEnd,
}

pub fn to_jsonl(session: &Session, scenario: &Scenario) -> String {
    let mut lines = Vec::new();
    lines.push(
        serde_json::to_string(&Record::Header {
            format_version: FORMAT_VERSION.to_string(),
            scenario_id: scenario.id.clone(),
            scenario_version: scenario.version.clone(),
            mode: session.mode.clone(),
            started_at: session.started_at.clone(),
        })
        .expect("header serializes"),
    );

    for item in &session.items {
        let record = match item {
            SessionItem::Utterance(utterance) => Record::Utterance {
                id: utterance.id.clone(),
                speaker: utterance.speaker,
                text: utterance.text.clone(),
                t_ms: utterance.t_ms,
                fact_ids: utterance.fact_ids.clone(),
            },
            SessionItem::Event { name } => Record::Event { name: name.clone() },
            SessionItem::Latency(latency) => Record::Latency {
                stage: latency.stage.clone(),
                ms: latency.ms,
            },
        };
        lines.push(serde_json::to_string(&record).expect("record serializes"));
    }

    lines.push(serde_json::to_string(&Record::End).expect("end serializes"));
    let mut log = lines.join("\n");
    log.push('\n');
    log
}

pub fn session_id(log_jsonl: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(log_jsonl.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn replay(jsonl: &str) -> Result<Session, EventLogError> {
    let mut session: Option<Session> = None;
    let mut saw_end = false;

    for (index, line) in jsonl.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: Record = serde_json::from_str(line).map_err(|source| EventLogError::Json {
            line: index + 1,
            source,
        })?;

        match record {
            Record::Header {
                format_version: _,
                scenario_id,
                scenario_version,
                mode,
                started_at,
            } => {
                session = Some(Session::from_parts(
                    scenario_id,
                    scenario_version,
                    mode,
                    started_at,
                ));
            }
            Record::Utterance {
                id,
                speaker,
                text,
                t_ms,
                fact_ids,
            } => {
                let session = session.as_mut().ok_or(EventLogError::RecordBeforeHeader)?;
                session.push_utterance(crate::session::Utterance {
                    id,
                    speaker,
                    text,
                    t_ms,
                    fact_ids,
                });
            }
            Record::Event { name } => {
                let session = session.as_mut().ok_or(EventLogError::RecordBeforeHeader)?;
                session.add_event(name);
            }
            Record::Latency { stage, ms } => {
                let session = session.as_mut().ok_or(EventLogError::RecordBeforeHeader)?;
                session.add_latency(stage, ms);
            }
            Record::End => saw_end = true,
        }
    }

    if !saw_end {
        return Err(EventLogError::MissingEnd);
    }

    session.ok_or(EventLogError::MissingHeader)
}
