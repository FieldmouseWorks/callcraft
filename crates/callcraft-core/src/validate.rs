use std::sync::OnceLock;

use jsonschema::Validator;
use regex::Regex;
use serde_json::Value;

use crate::scenario::{Disclosure, FORMAT_ID, Scenario};

const SCHEMA_JSON: &str = include_str!("../../../schemas/scenario.v0.1.schema.json");

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("YAML parse error: {0}")]
    Yaml(String),
    #[error("scenario validation failed:\n{0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct Validated {
    pub scenario: Scenario,
    /// Non-fatal findings (content policy, rubric items without anchors, ...).
    pub warnings: Vec<String>,
}

pub fn validate_yaml(text: &str, strict: bool) -> Result<Validated, ValidationError> {
    let value: Value =
        serde_saphyr::from_str(text).map_err(|e| ValidationError::Yaml(e.to_string()))?;
    validate_value(&value, strict)
}

pub fn validate_value(value: &Value, strict: bool) -> Result<Validated, ValidationError> {
    let schema_findings = schema_errors(value);
    if !schema_findings.is_empty() {
        return Err(ValidationError::Invalid(schema_findings.join("\n")));
    }

    let scenario: Scenario = serde_json::from_value(value.clone())
        .map_err(|e| ValidationError::Invalid(format!("type mismatch: {e}")))?;

    let semantic = semantic_errors(&scenario);
    if !semantic.is_empty() {
        return Err(ValidationError::Invalid(semantic.join("\n")));
    }

    let warnings = content_policy(&scenario);
    if strict && !warnings.is_empty() {
        return Err(ValidationError::Invalid(format!(
            "content policy (strict):\n{}",
            warnings.join("\n")
        )));
    }

    Ok(Validated { scenario, warnings })
}

/// Render a validated scenario back to YAML. The authoring round-trip lives
/// here so the YAML crate stays inside the core.
pub fn render_yaml(scenario: &Scenario) -> Result<String, String> {
    serde_saphyr::to_string(scenario).map_err(|error| error.to_string())
}

fn validator() -> &'static Validator {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(SCHEMA_JSON).expect("embedded scenario schema is valid JSON");
        jsonschema::validator_for(&schema).expect("embedded scenario schema compiles")
    })
}

fn schema_errors(value: &Value) -> Vec<String> {
    let mut findings: Vec<String> = validator()
        .iter_errors(value)
        .map(|err| format!("{}: {err}", err.instance_path()))
        .collect();
    findings.sort();
    findings
}

fn semantic_errors(scenario: &Scenario) -> Vec<String> {
    let mut problems = Vec::new();

    if scenario.format != FORMAT_ID {
        problems.push(format!(
            "format must be '{FORMAT_ID}', found '{}'",
            scenario.format
        ));
    }

    let mut fact_ids = std::collections::BTreeSet::new();
    for fact in &scenario.facts {
        if !fact_ids.insert(fact.id.clone()) {
            problems.push(format!("duplicate fact id '{}'", fact.id));
        }
        if fact.disclosure == Disclosure::AfterEvent && fact.requires_event.is_none() {
            problems.push(format!(
                "fact '{}' uses after_event without requires_event",
                fact.id
            ));
        }
    }

    let mut item_ids = std::collections::BTreeSet::new();
    for item in &scenario.rubric.items {
        if !item_ids.insert(item.id.clone()) {
            problems.push(format!("duplicate rubric item id '{}'", item.id));
        }
        if item.anchors.is_none() {
            problems.push(format!(
                "rubric item '{}' has no anchors; it can never pass",
                item.id
            ));
        }
        if let Some(anchors) = &item.anchors {
            for anchor in anchors.all() {
                if let Some(pattern) = &anchor.transcript_regex {
                    if let Err(err) = Regex::new(pattern) {
                        problems.push(format!(
                            "rubric item '{}': invalid transcript_regex: {err}",
                            item.id
                        ));
                    }
                }
                if let Some(pair) = &anchor.utterance_pair {
                    let reference = pair.first().map(String::as_str).unwrap_or_default();
                    if let Some(fact_id) = reference.strip_prefix("caller_") {
                        if !fact_ids.contains(fact_id) {
                            problems.push(format!(
                                "rubric item '{}': utterance_pair references unknown fact '{fact_id}'",
                                item.id
                            ));
                        }
                    }
                }
            }
        }
    }

    problems
}

fn content_policy(scenario: &Scenario) -> Vec<String> {
    let mut warnings = Vec::new();
    let ten_digit = ten_digit_phone();
    let seven_digit = seven_digit_phone();

    for fact in &scenario.facts {
        let text = fact.value_text();
        for captures in ten_digit.captures_iter(&text) {
            warnings.push(format!(
                "fact '{}': phone-like value '{}' is not synthetic; public content must use 555-01xx numbers",
                fact.id,
                captures.get(0).map(|m| m.as_str()).unwrap_or_default()
            ));
        }
        for captures in seven_digit.captures_iter(&text) {
            let whole = captures.get(0).map(|m| m.as_str()).unwrap_or_default();
            let area = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
            let rest = captures.get(2).map(|m| m.as_str()).unwrap_or_default();
            let synthetic = area == "555" && rest.starts_with("01");
            if !synthetic {
                warnings.push(format!(
                    "fact '{}': phone-like value '{whole}' is not synthetic; public content must use 555-01xx numbers",
                    fact.id
                ));
            }
        }
    }

    warnings
}

fn ten_digit_phone() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{3})[-.\s]?(\d{3})[-.\s]?(\d{4})\b").expect("phone regex"))
}

fn seven_digit_phone() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{3})[-.\s](\d{4})\b").expect("phone regex"))
}
