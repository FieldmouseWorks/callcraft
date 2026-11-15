//! Agency pack manifest types and validation.
//!
//! The pack filesystem store lives in `callcraft-server`; this module is pure
//! and owns the manifest contract plus the safety rules for ids.

use serde::{Deserialize, Serialize};

use crate::validate::{Validated, validate_yaml};

pub const FORMAT_ID: &str = "callcraft-pack/v0.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub format: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub visibility: PackVisibility,
    pub license: String,
    pub owner: PackOwner,
    pub requires: PackRequires,
    #[serde(default)]
    pub protocols: Vec<ProtocolRef>,
    #[serde(default)]
    pub sources: PackSources,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported: Option<PackExport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackVisibility {
    Private,
    Shareable,
    Public,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackOwner {
    pub agency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackRequires {
    pub scenario_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRef {
    pub id: String,
    pub title: String,
    pub owner: ProtocolOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolOwner {
    Agency,
    Vendor,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackSources {
    #[serde(default)]
    pub export: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackExport {
    pub at: String,
    pub by: String,
    pub tool_version: String,
}

impl PackManifest {
    pub fn parse_yaml(text: &str) -> Result<Self, String> {
        serde_saphyr::from_str(text).map_err(|error| format!("pack manifest YAML: {error}"))
    }

    pub fn render_yaml(&self) -> Result<String, String> {
        serde_saphyr::to_string(self).map_err(|error| format!("pack manifest render: {error}"))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT_ID {
            return Err(format!(
                "pack format must be '{FORMAT_ID}', found '{}'",
                self.format
            ));
        }
        if !safe_id(&self.id) {
            return Err(format!(
                "pack id '{}' must match ^[a-z0-9][a-z0-9-]{{0,63}}$",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            return Err("pack name is required".to_string());
        }
        if !is_semver(&self.version) {
            return Err(format!(
                "pack version '{}' must be MAJOR.MINOR.PATCH",
                self.version
            ));
        }
        if self.license.trim().is_empty() {
            return Err("pack license is required".to_string());
        }
        if self.owner.agency.trim().is_empty() {
            return Err("pack owner.agency is required".to_string());
        }
        if self.requires.scenario_format.trim().is_empty() {
            return Err("pack requires.scenario_format is required".to_string());
        }
        for protocol in &self.protocols {
            if protocol.id.trim().is_empty() || protocol.title.trim().is_empty() {
                return Err("protocol references need an id and title".to_string());
            }
        }
        Ok(())
    }
}

pub fn validate_scenario_text(
    text: &str,
    expected_id: &str,
    strict: bool,
) -> Result<Validated, String> {
    let validated = validate_yaml(text, strict).map_err(|error| error.to_string())?;
    if validated.scenario.id != expected_id {
        return Err(format!(
            "scenario id '{}' does not match filename id '{expected_id}'",
            validated.scenario.id
        ));
    }
    Ok(validated)
}

pub fn safe_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    id.len() <= 64
        && id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn is_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> PackManifest {
        PackManifest {
            format: FORMAT_ID.to_string(),
            id: "cedar-county".to_string(),
            name: "Cedar County Training".to_string(),
            version: "0.1.0".to_string(),
            visibility: PackVisibility::Private,
            license: "proprietary".to_string(),
            owner: PackOwner {
                agency: "Cedar County ECC".to_string(),
                contact: None,
            },
            requires: PackRequires {
                scenario_format: ">=0.1 <0.2".to_string(),
            },
            protocols: Vec::new(),
            sources: PackSources::default(),
            exported: None,
        }
    }

    #[test]
    fn manifest_round_trips() {
        let manifest = manifest();
        manifest.validate().expect("valid");
        let yaml = manifest.render_yaml().expect("render");
        let parsed = PackManifest::parse_yaml(&yaml).expect("parse");
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn ids_must_be_safe() {
        assert!(safe_id("cedar-county"));
        assert!(!safe_id("../cedar"));
        assert!(!safe_id("Cedar"));
        assert!(!safe_id(""));
    }

    #[test]
    fn bad_versions_are_rejected() {
        let mut manifest = manifest();
        manifest.version = "one".to_string();
        assert!(manifest.validate().is_err());
    }
}
