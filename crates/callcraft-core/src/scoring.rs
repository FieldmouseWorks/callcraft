use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::scenario::{Anchor, RubricItem, Scenario};
use crate::session::{Session, Speaker, Utterance};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Pass,
    Fail,
    NeedsHuman,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemResult {
    pub id: String,
    pub description: String,
    pub critical: bool,
    pub result: ItemStatus,
    /// The deterministic machine result, retained even when human review is
    /// required, so agreement with blind human grading can be computed.
    pub machine_result: ItemStatus,
    pub evidence: Vec<String>,
    pub human_review: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub items: Vec<ItemResult>,
    pub critical_failures: Vec<String>,
    pub human_review_required: Vec<String>,
    pub overall: ItemStatus,
    /// Rejections of caller output that was not backed by the fact base.
    pub drift_rejections: u32,
    /// Provider/response-shape failures that are not closed-world violations.
    pub adapter_failures: u32,
}

pub fn evaluate(scenario: &Scenario, session: &Session) -> Evaluation {
    let utterances: Vec<&Utterance> = session.transcript().collect();
    let mut items = Vec::with_capacity(scenario.rubric.items.len());

    for item in &scenario.rubric.items {
        let (machine_pass, evidence) = evaluate_item(item, scenario, session, &utterances);
        let human_review = item
            .human_review
            .as_ref()
            .map(|review| review.required())
            .unwrap_or(false);
        let machine_result = if machine_pass {
            ItemStatus::Pass
        } else {
            ItemStatus::Fail
        };
        let result = if human_review {
            ItemStatus::NeedsHuman
        } else {
            machine_result
        };

        items.push(ItemResult {
            id: item.id.clone(),
            description: item.description.clone(),
            critical: item.critical,
            result,
            machine_result,
            evidence,
            human_review,
        });
    }

    let critical_failures = items
        .iter()
        .filter(|item| item.critical && item.machine_result == ItemStatus::Fail)
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let human_review_required = items
        .iter()
        .filter(|item| item.human_review)
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let overall = if critical_failures.is_empty() {
        ItemStatus::Pass
    } else {
        ItemStatus::Fail
    };

    Evaluation {
        items,
        critical_failures,
        human_review_required,
        overall,
        drift_rejections: session.drift_rejections,
        adapter_failures: session.adapter_failures,
    }
}

fn evaluate_item(
    item: &RubricItem,
    scenario: &Scenario,
    session: &Session,
    utterances: &[&Utterance],
) -> (bool, Vec<String>) {
    let Some(anchors) = &item.anchors else {
        return (false, Vec::new());
    };

    if !anchors.any_of.is_empty() {
        let mut passed = false;
        let mut evidence = Vec::new();
        for anchor in &anchors.any_of {
            let (ok, mut found) = match_anchor(anchor, scenario, session, utterances);
            passed |= ok;
            evidence.append(&mut found);
        }
        return (passed, evidence);
    }

    if !anchors.all_of.is_empty() {
        let mut passed = true;
        let mut evidence = Vec::new();
        for anchor in &anchors.all_of {
            let (ok, mut found) = match_anchor(anchor, scenario, session, utterances);
            passed &= ok;
            evidence.append(&mut found);
        }
        return (passed, evidence);
    }

    match anchors.direct_anchor() {
        Some(anchor) => match_anchor(&anchor, scenario, session, utterances),
        None => (false, Vec::new()),
    }
}

fn match_anchor(
    anchor: &Anchor,
    scenario: &Scenario,
    session: &Session,
    utterances: &[&Utterance],
) -> (bool, Vec<String>) {
    if let Some(pattern) = &anchor.transcript_regex {
        let Ok(regex) = Regex::new(pattern) else {
            return (false, Vec::new());
        };
        let evidence = utterances
            .iter()
            .filter(|utterance| utterance.speaker == Speaker::Trainee)
            .filter(|utterance| regex.is_match(&utterance.text))
            .map(|utterance| utterance.id.clone())
            .collect::<Vec<_>>();
        return (!evidence.is_empty(), evidence);
    }

    if let Some(pair) = &anchor.utterance_pair {
        let reference = pair.first().map(String::as_str).unwrap_or_default();
        let fact_id = reference.strip_prefix("caller_").unwrap_or(reference);
        let Some(fact) = scenario.facts.iter().find(|fact| fact.id == fact_id) else {
            return (false, Vec::new());
        };
        let Some(disclosure_index) = utterances.iter().position(|utterance| {
            utterance.speaker == Speaker::Caller
                && utterance.fact_ids.iter().any(|id| id == fact_id)
        }) else {
            return (false, Vec::new());
        };
        for utterance in utterances.iter().skip(disclosure_index + 1) {
            if utterance.speaker == Speaker::Trainee
                && value_in_utterance(&fact.value_text(), &utterance.text)
            {
                return (
                    true,
                    vec![
                        utterances[disclosure_index].id.clone(),
                        utterance.id.clone(),
                    ],
                );
            }
        }
        return (false, Vec::new());
    }

    if let Some(event) = &anchor.event {
        let found = session.events().any(|recorded| recorded == event);
        return (
            found,
            if found {
                vec![format!("event:{event}")]
            } else {
                Vec::new()
            },
        );
    }

    (false, Vec::new())
}

fn value_in_utterance(value: &str, text: &str) -> bool {
    let value_digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if value_digits.len() >= 7 {
        let text_digits: String = text.chars().filter(char::is_ascii_digit).collect();
        return text_digits.contains(&value_digits);
    }

    let tokens = tokens(value);
    if tokens.is_empty() {
        return false;
    }
    let normalized = normalize(text);
    let hits = tokens
        .iter()
        .filter(|token| word_regex(token).is_match(&normalized))
        .count();
    hits >= 2.min(tokens.len())
}

fn tokens(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() > 1)
        .map(str::to_string)
        .collect()
}

fn word_regex(token: &str) -> Regex {
    Regex::new(&format!(r"\b{}\b", regex::escape(token))).expect("escaped token compiles")
}

fn normalize(text: &str) -> String {
    let lowered = text.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}
