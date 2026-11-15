use serde::{Deserialize, Serialize};

use crate::caller::CallerEngine;
use crate::scenario::Scenario;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Trainee,
    Caller,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Utterance {
    pub id: String,
    pub speaker: Speaker,
    pub text: String,
    pub t_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionItem {
    Utterance(Utterance),
    Event { name: String },
}

#[derive(Debug, Clone)]
pub struct Session {
    pub scenario_id: String,
    pub scenario_version: String,
    pub mode: String,
    pub started_at: String,
    pub items: Vec<SessionItem>,
    pub drift_rejections: u32,
    next_id: u64,
}

impl Session {
    pub fn new(scenario: &Scenario, mode: &str, started_at: String) -> Self {
        Self::from_parts(
            scenario.id.clone(),
            scenario.version.clone(),
            mode.to_string(),
            started_at,
        )
    }

    pub fn from_parts(
        scenario_id: String,
        scenario_version: String,
        mode: String,
        started_at: String,
    ) -> Self {
        Self {
            scenario_id,
            scenario_version,
            mode,
            started_at,
            items: Vec::new(),
            drift_rejections: 0,
            next_id: 0,
        }
    }

    pub fn add_utterance(
        &mut self,
        speaker: Speaker,
        text: String,
        t_ms: u64,
        fact_ids: Vec<String>,
    ) -> String {
        self.next_id += 1;
        let id = format!("u{}", self.next_id);
        self.push_utterance(Utterance {
            id,
            speaker,
            text,
            t_ms,
            fact_ids,
        });
        format!("u{}", self.next_id)
    }

    pub fn push_utterance(&mut self, utterance: Utterance) {
        self.items.push(SessionItem::Utterance(utterance));
    }

    pub fn add_event(&mut self, name: String) {
        self.items.push(SessionItem::Event { name });
    }

    pub fn transcript(&self) -> impl Iterator<Item = &Utterance> {
        self.items.iter().filter_map(|item| match item {
            SessionItem::Utterance(utterance) => Some(utterance),
            SessionItem::Event { .. } => None,
        })
    }

    pub fn events(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|item| match item {
            SessionItem::Event { name } => Some(name.as_str()),
            SessionItem::Utterance(_) => None,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Script {
    #[serde(default)]
    pub scenario: Option<String>,
    pub turns: Vec<ScriptTurn>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScriptTurn {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("script targets scenario '{script}', expected '{scenario}'")]
    WrongScenario { script: String, scenario: String },
    #[error("script turn has neither text nor event")]
    EmptyTurn,
    #[error("script YAML error: {0}")]
    Yaml(String),
}

pub fn parse_script(yaml: &str) -> Result<Script, SessionError> {
    serde_saphyr::from_str(yaml).map_err(|err| SessionError::Yaml(err.to_string()))
}

pub fn run_scripted(
    scenario: &Scenario,
    script: &Script,
    started_at: String,
) -> Result<Session, SessionError> {
    if let Some(target) = &script.scenario {
        if target != &scenario.id {
            return Err(SessionError::WrongScenario {
                script: target.clone(),
                scenario: scenario.id.clone(),
            });
        }
    }

    let mut session = Session::new(scenario, "scripted", started_at);
    let mut engine = CallerEngine::new(scenario);
    let mut t_ms: u64 = 0;

    for turn in &script.turns {
        match (&turn.event, &turn.text) {
            (Some(event), _) => {
                session.add_event(event.clone());
                engine.note_event(event);
            }
            (None, Some(text)) => {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                session.add_utterance(Speaker::Trainee, text.to_string(), t_ms, Vec::new());
                let reply = engine.respond(text, t_ms as f64 / 1000.0);
                t_ms += 1_200;
                session.add_utterance(Speaker::Caller, reply.text, t_ms, reply.disclosed);
                t_ms += 13_800;
            }
            (None, None) => return Err(SessionError::EmptyTurn),
        }
    }

    Ok(session)
}
