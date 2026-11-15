use std::collections::BTreeSet;

use regex::Regex;

use crate::scenario::{Disclosure, Scenario};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerTurn {
    pub text: String,
    pub disclosed: Vec<String>,
    pub repeated: Vec<String>,
}

/// Deterministic caller. It can only speak from the scenario's fact base; it
/// cannot invent. The LLM adapter, when added, sits behind this same contract.
pub struct CallerEngine {
    scenario: Scenario,
    patterns: Vec<(usize, Vec<Regex>)>,
    disclosed: BTreeSet<String>,
    volunteered: bool,
    fallback_index: usize,
    events: BTreeSet<String>,
}

const DEFAULT_FALLBACKS: &[&str] = &[
    "I don't know what you mean. Please, just send someone.",
    "I'm not sure. Please hurry.",
];

const DISTRESSED_FALLBACKS: &[&str] = &[
    "Please hurry, I don't understand what you're asking.",
    "I don't know. Please send help.",
];

const STRUGGLING_FALLBACKS: &[&str] = &["Please... hurry.", "I can't... just send someone."];

impl CallerEngine {
    pub fn new(scenario: &Scenario) -> Self {
        let mut patterns = Vec::with_capacity(scenario.facts.len());
        for (index, fact) in scenario.facts.iter().enumerate() {
            let mut compiled = Vec::new();
            for trigger in &fact.triggers {
                let normalized = normalize(trigger);
                if normalized.is_empty() {
                    continue;
                }
                if let Ok(pattern) = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(&normalized)))
                {
                    compiled.push(pattern);
                }
            }
            patterns.push((index, compiled));
        }

        Self {
            scenario: scenario.clone(),
            patterns,
            disclosed: BTreeSet::new(),
            volunteered: false,
            fallback_index: 0,
            events: BTreeSet::new(),
        }
    }

    pub fn note_event(&mut self, name: &str) {
        self.events.insert(name.to_string());
    }

    /// Marks a fact as already disclosed so a later deterministic fallback does
    /// not re-reveal it. Used after a validated LLM caller decision.
    pub fn mark_disclosed(&mut self, fact_id: &str) {
        if self.scenario.facts.iter().any(|fact| fact.id == fact_id) {
            self.disclosed.insert(fact_id.to_string());
        }
    }

    /// Facts already disclosed, for prompt context and repeat handling.
    pub fn disclosed_fact_ids(&self) -> impl Iterator<Item = &str> {
        self.disclosed.iter().map(String::as_str)
    }

    pub fn respond(&mut self, trainee_text: &str, elapsed_s: f64) -> CallerTurn {
        let normalized = normalize(trainee_text);

        if !self.volunteered {
            self.volunteered = true;
            if let Some(fact) = self
                .scenario
                .facts
                .iter()
                .find(|fact| fact.disclosure == Disclosure::Volunteer)
            {
                self.disclosed.insert(fact.id.clone());
                return CallerTurn {
                    text: fact.value_text(),
                    disclosed: vec![fact.id.clone()],
                    repeated: Vec::new(),
                };
            }
        }

        let matched = self
            .patterns
            .iter()
            .find(|(index, compiled)| {
                let fact = &self.scenario.facts[*index];
                if fact.disclosure == Disclosure::Volunteer {
                    return false;
                }
                if fact.disclosure == Disclosure::AfterEvent {
                    let ready = fact
                        .requires_event
                        .as_ref()
                        .map(|event| self.events.contains(event))
                        .unwrap_or(false);
                    if !ready {
                        return false;
                    }
                }
                compiled.iter().any(|pattern| pattern.is_match(&normalized))
            })
            .map(|(index, _)| *index);

        if let Some(index) = matched {
            let fact = &self.scenario.facts[index];
            let fact_id = fact.id.clone();
            let value = fact.value_text();
            if self.disclosed.contains(&fact_id) {
                return CallerTurn {
                    text: format!("I already told you — {value}"),
                    disclosed: Vec::new(),
                    repeated: vec![fact_id],
                };
            }
            self.disclosed.insert(fact_id.clone());
            return CallerTurn {
                text: value,
                disclosed: vec![fact_id],
                repeated: Vec::new(),
            };
        }

        let state = self.state_at(elapsed_s);
        let fallbacks = if state.contains("struggl") {
            STRUGGLING_FALLBACKS
        } else if state.contains("distress") {
            DISTRESSED_FALLBACKS
        } else {
            DEFAULT_FALLBACKS
        };
        let text = fallbacks[self.fallback_index % fallbacks.len()].to_string();
        self.fallback_index += 1;

        CallerTurn {
            text,
            disclosed: Vec::new(),
            repeated: Vec::new(),
        }
    }

    fn state_at(&self, elapsed_s: f64) -> String {
        self.scenario
            .timeline
            .iter()
            .filter(|event| event.at_seconds <= elapsed_s)
            .max_by(|left, right| {
                left.at_seconds
                    .partial_cmp(&right.at_seconds)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|event| event.caller_state.to_lowercase())
            .unwrap_or_default()
    }
}

fn normalize(text: &str) -> String {
    let lowered = text.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}
