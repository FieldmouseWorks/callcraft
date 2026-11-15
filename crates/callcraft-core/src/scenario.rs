use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The scenario format identifier carried in every file.
pub const FORMAT_ID: &str = "callcraft-scenario/v0.1";
/// The format version emitted in reports and event logs.
pub const FORMAT_VERSION: &str = "0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub format: String,
    pub id: String,
    pub title: String,
    pub version: String,
    pub license: String,
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incident_type: Option<IncidentType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<Difficulty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_minutes: Option<f64>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub persona: Persona,
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub timeline: Vec<TimelineEvent>,
    pub rubric: Rubric,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geography: Option<Geography>,
    pub safety: Safety,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xapi: Option<Value>,
    /// Unknown fields are preserved so newer files survive older tools.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IncidentType {
    Medical,
    Fire,
    Police,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Basic,
    Intermediate,
    Advanced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotional_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Value>,
    #[serde(default)]
    pub impairments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub value: FactValue,
    pub disclosure: Disclosure,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifiable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_event: Option<String>,
}

impl Fact {
    pub fn value_text(&self) -> String {
        self.value.as_text()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FactValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

impl FactValue {
    pub fn as_text(&self) -> String {
        match self {
            FactValue::Text(s) => s.clone(),
            FactValue::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            FactValue::Bool(b) => b.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Disclosure {
    Volunteer,
    OnQuestion,
    IfAsked,
    NeverUnless,
    AfterEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub at_seconds: f64,
    pub caller_state: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rubric {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scoring: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub critical_must_pass: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_review_required: Option<bool>,
    pub items: Vec<RubricItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricItem {
    pub id: String,
    pub description: String,
    pub critical: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_review: Option<HumanReview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchors: Option<Anchors>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum HumanReview {
    Bool(bool),
    Text(String),
}

impl HumanReview {
    pub fn required(&self) -> bool {
        match self {
            HumanReview::Bool(b) => *b,
            HumanReview::Text(s) => s.eq_ignore_ascii_case("required"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Anchors {
    #[serde(default)]
    pub any_of: Vec<Anchor>,
    #[serde(default)]
    pub all_of: Vec<Anchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utterance_pair: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
}

impl Anchors {
    pub fn direct_anchor(&self) -> Option<Anchor> {
        if self.transcript_regex.is_none() && self.utterance_pair.is_none() && self.event.is_none()
        {
            return None;
        }
        Some(Anchor {
            transcript_regex: self.transcript_regex.clone(),
            utterance_pair: self.utterance_pair.clone(),
            event: self.event.clone(),
        })
    }

    /// All anchors in evaluation order: any_of, all_of, then a direct anchor.
    pub fn all(&self) -> Vec<Anchor> {
        let mut anchors: Vec<Anchor> = self.any_of.to_vec();
        anchors.extend(self.all_of.iter().cloned());
        if let Some(anchor) = self.direct_anchor() {
            anchors.push(anchor);
        }
        anchors
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Anchor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utterance_pair: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Geography {
    pub pack: String,
    #[serde(default)]
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Safety {
    pub disclaimer: String,
    #[serde(default)]
    pub content_warnings: Vec<String>,
}
