//! Local agency-pack storage.
//!
//! Packs are directories under the OS local-data directory, never the repo and
//! never Roaming/OneDrive by default. The directory is the source of truth:
//! there is no manifest file list or database to drift.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use callcraft_core::pack::{
    ApprovalLedger, ApprovalStatus, FORMAT_ID, PackManifest, PackOwner, PackRequires, PackSources,
    PackVisibility, ProtocolRef, safe_id, sha256_hex, validate_scenario_text,
};
use callcraft_core::scenario::{Difficulty, IncidentType};
use callcraft_core::validate::{Validated, validate_yaml};

pub const DEFAULT_PACK_ID: &str = "local";

mod transport;
pub use transport::{ImportMode, ImportReport, ImportScenario};

#[derive(Debug, Clone)]
pub struct PackStore {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct PacksView {
    pub packs: Vec<PackView>,
    pub default_pack_id: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub visibility: String,
    pub license: String,
    pub owner: String,
    pub scenarios: Vec<ScenarioEntry>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioEntry {
    pub id: String,
    pub title: String,
    pub version: String,
    pub license: String,
    pub difficulty: Option<Difficulty>,
    pub incident_type: Option<IncidentType>,
    pub duration_minutes: Option<f64>,
    pub facts: usize,
    pub rubric_items: usize,
    pub critical_items: usize,
    pub modified: String,
    pub file: String,
    pub valid: bool,
    pub status: String,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SaveResult {
    pub pack_id: String,
    pub scenario: ScenarioEntry,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OpenedScenario {
    pub yaml: String,
    pub validated: Validated,
    pub scenario_sha256: String,
    pub pack_version: String,
    pub approval: String,
}

impl PackStore {
    pub fn from_env() -> Self {
        if let Ok(path) = env::var("CALLCRAFT_PACK_DIR")
            && !path.trim().is_empty()
        {
            return Self::with_root(PathBuf::from(path));
        }
        Self::with_root(default_pack_root())
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn warning(&self) -> Option<String> {
        let value = self.root.to_string_lossy().to_ascii_lowercase();
        for marker in ["onedrive", "dropbox", "google drive", "icloud"] {
            if value.contains(marker) {
                return Some(format!(
                    "pack store appears to be inside a cloud-sync folder ({marker}); keep agency content local"
                ));
            }
        }
        None
    }

    pub fn default_pack_id(&self) -> String {
        let mut ids = self.installed_pack_ids();
        ids.sort();
        ids.into_iter()
            .next()
            .unwrap_or_else(|| DEFAULT_PACK_ID.to_string())
    }

    pub fn list(&self) -> PacksView {
        let mut packs = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                packs.push(self.read_pack(id, &path));
            }
        }
        packs.sort_by_key(|left| left.name.to_lowercase());
        PacksView {
            default_pack_id: packs
                .first()
                .map(|pack| pack.id.clone())
                .unwrap_or_else(|| DEFAULT_PACK_ID.to_string()),
            packs,
            warning: self.warning(),
        }
    }

    pub fn save_default(&self, yaml: &str) -> Result<SaveResult, String> {
        self.save(&self.default_pack_id(), yaml)
    }

    pub fn save(&self, pack_id: &str, yaml: &str) -> Result<SaveResult, String> {
        if !safe_id(pack_id) {
            return Err(format!("invalid pack id '{pack_id}'"));
        }
        let validated = validate_yaml(yaml, false).map_err(|error| error.to_string())?;
        let scenario_id = validated.scenario.id.clone();
        if !safe_id(&scenario_id) {
            return Err(format!("invalid scenario id '{scenario_id}'"));
        }

        let pack_dir = self.root.join(pack_id);
        if !pack_dir.join("callcraft-pack.yaml").is_file() {
            if pack_id != DEFAULT_PACK_ID {
                return Err(format!("pack '{pack_id}' is not installed"));
            }
            self.write_default_manifest(&pack_dir)?;
        }

        let manifest_text = fs::read_to_string(pack_dir.join("callcraft-pack.yaml"))
            .map_err(|error| format!("read pack manifest: {error}"))?;
        let manifest = PackManifest::parse_yaml(&manifest_text)?;
        manifest.validate()?;
        if manifest.id != pack_id {
            return Err(format!(
                "pack manifest id '{}' does not match directory '{pack_id}'",
                manifest.id
            ));
        }

        let scenarios_dir = pack_dir.join("scenarios");
        fs::create_dir_all(&scenarios_dir)
            .map_err(|error| format!("create scenarios directory: {error}"))?;
        let path = scenarios_dir.join(format!("{scenario_id}.scenario.yaml"));
        let approvals_path = pack_dir.join("approvals.yaml");
        let mut ledger = read_ledger(&approvals_path)?;
        let existing_hash = fs::read(&path).ok().map(|bytes| sha256_hex(&bytes));
        write_private(&path, yaml)?;
        let new_hash = sha256_hex(yaml.as_bytes());
        if existing_hash.as_deref() != Some(new_hash.as_str()) {
            let had_approval = ledger
                .entries
                .iter()
                .any(|entry| entry.scenario_id == scenario_id);
            ledger.remove(&scenario_id);
            if had_approval {
                write_ledger(&approvals_path, &ledger)?;
            }
        }

        let entry = self.scenario_entry(&path, &scenario_id, yaml, &ledger);
        Ok(SaveResult {
            pack_id: pack_id.to_string(),
            scenario: entry,
            warnings: validated.warnings,
        })
    }

    pub fn open(&self, pack_id: &str, scenario_id: &str) -> Result<(String, Validated), String> {
        let opened = self.open_with_metadata(pack_id, scenario_id)?;
        Ok((opened.yaml, opened.validated))
    }

    pub fn open_with_metadata(
        &self,
        pack_id: &str,
        scenario_id: &str,
    ) -> Result<OpenedScenario, String> {
        if !safe_id(pack_id) || !safe_id(scenario_id) {
            return Err("invalid pack or scenario id".to_string());
        }
        let pack_dir = self.root.join(pack_id);
        let manifest_text = fs::read_to_string(pack_dir.join("callcraft-pack.yaml"))
            .map_err(|error| format!("read pack manifest: {error}"))?;
        let manifest = PackManifest::parse_yaml(&manifest_text)?;
        manifest.validate()?;
        if manifest.id != pack_id {
            return Err(format!(
                "pack manifest id '{}' does not match directory '{pack_id}'",
                manifest.id
            ));
        }
        let path = pack_dir
            .join("scenarios")
            .join(format!("{scenario_id}.scenario.yaml"));
        let yaml = fs::read_to_string(&path).map_err(|error| format!("read scenario: {error}"))?;
        let validated = validate_scenario_text(&yaml, scenario_id, false)?;
        let scenario_sha256 = sha256_hex(yaml.as_bytes());
        let ledger = read_ledger(&pack_dir.join("approvals.yaml"))?;
        let approval = ledger
            .status_for(scenario_id, &scenario_sha256)
            .as_str()
            .to_string();
        Ok(OpenedScenario {
            yaml,
            validated,
            scenario_sha256,
            pack_version: manifest.version,
            approval,
        })
    }

    pub fn set_approval(
        &self,
        pack_id: &str,
        scenario_id: &str,
        status: ApprovalStatus,
        by: &str,
        note: Option<String>,
    ) -> Result<ScenarioEntry, String> {
        if !safe_id(pack_id) || !safe_id(scenario_id) {
            return Err("invalid pack or scenario id".to_string());
        }
        let opened = self.open_with_metadata(pack_id, scenario_id)?;
        let pack_dir = self.root.join(pack_id);
        let approvals_path = pack_dir.join("approvals.yaml");
        let mut ledger = read_ledger(&approvals_path)?;
        ledger.set(
            scenario_id,
            &opened.scenario_sha256,
            status,
            by,
            &crate::util::now_iso(),
            note,
        );
        write_ledger(&approvals_path, &ledger)?;
        let path = pack_dir
            .join("scenarios")
            .join(format!("{scenario_id}.scenario.yaml"));
        Ok(self.scenario_entry(&path, scenario_id, &opened.yaml, &ledger))
    }

    fn installed_pack_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let Some(id) = entry.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if safe_id(&id) && entry.path().join("callcraft-pack.yaml").is_file() {
                    ids.push(id);
                }
            }
        }
        ids
    }

    fn read_pack(&self, id: &str, path: &Path) -> PackView {
        let manifest_path = path.join("callcraft-pack.yaml");
        let manifest_text = match fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(error) => {
                return PackView {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: String::new(),
                    visibility: String::new(),
                    license: String::new(),
                    owner: String::new(),
                    scenarios: Vec::new(),
                    error: Some(format!("read callcraft-pack.yaml: {error}")),
                };
            }
        };
        let manifest = match PackManifest::parse_yaml(&manifest_text) {
            Ok(manifest) => manifest,
            Err(error) => {
                return PackView {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: String::new(),
                    visibility: String::new(),
                    license: String::new(),
                    owner: String::new(),
                    scenarios: Vec::new(),
                    error: Some(error),
                };
            }
        };
        if let Err(error) = manifest.validate() {
            return PackView {
                id: id.to_string(),
                name: manifest.name,
                version: manifest.version,
                visibility: visibility_label(manifest.visibility).to_string(),
                license: manifest.license,
                owner: manifest.owner.agency,
                scenarios: Vec::new(),
                error: Some(error),
            };
        }
        let ledger = match read_ledger(&path.join("approvals.yaml")) {
            Ok(ledger) => ledger,
            Err(error) => {
                return PackView {
                    id: manifest.id,
                    name: manifest.name,
                    version: manifest.version,
                    visibility: visibility_label(manifest.visibility).to_string(),
                    license: manifest.license,
                    owner: manifest.owner.agency,
                    scenarios: Vec::new(),
                    error: Some(error),
                };
            }
        };

        let mut scenarios = Vec::new();
        if let Ok(entries) = fs::read_dir(path.join("scenarios")) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(id) = name.strip_suffix(".scenario.yaml") else {
                    continue;
                };
                if !safe_id(id) {
                    continue;
                }
                match fs::read_to_string(&path) {
                    Ok(text) => scenarios.push(self.scenario_entry(&path, id, &text, &ledger)),
                    Err(error) => scenarios.push(ScenarioEntry {
                        id: id.to_string(),
                        title: String::new(),
                        version: String::new(),
                        license: String::new(),
                        difficulty: None,
                        incident_type: None,
                        duration_minutes: None,
                        facts: 0,
                        rubric_items: 0,
                        critical_items: 0,
                        modified: modified(&path),
                        file: format!("scenarios/{name}"),
                        valid: false,
                        status: "draft".to_string(),
                        warnings: Vec::new(),
                        error: Some(format!("read scenario: {error}")),
                    }),
                }
            }
        }
        scenarios.sort_by_key(|left| left.title.to_lowercase());

        PackView {
            id: manifest.id.clone(),
            name: manifest.name,
            version: manifest.version,
            visibility: visibility_label(manifest.visibility).to_string(),
            license: manifest.license,
            owner: manifest.owner.agency,
            scenarios,
            error: None,
        }
    }

    fn scenario_entry(
        &self,
        path: &Path,
        expected_id: &str,
        text: &str,
        ledger: &ApprovalLedger,
    ) -> ScenarioEntry {
        let modified = modified(path);
        let file = path
            .strip_prefix(self.root.join(expected_id).join("scenarios"))
            .ok()
            .and_then(|relative| relative.to_str())
            .map(|relative| format!("scenarios/{relative}"))
            .unwrap_or_else(|| format!("scenarios/{expected_id}.scenario.yaml"));
        match validate_scenario_text(text, expected_id, false) {
            Ok(validated) => {
                let status = ledger
                    .status_for(expected_id, &sha256_hex(text.as_bytes()))
                    .as_str()
                    .to_string();
                let scenario = validated.scenario;
                ScenarioEntry {
                    id: scenario.id,
                    title: scenario.title,
                    version: scenario.version,
                    license: scenario.license,
                    difficulty: scenario.difficulty,
                    incident_type: scenario.incident_type,
                    duration_minutes: scenario.duration_minutes,
                    facts: scenario.facts.len(),
                    rubric_items: scenario.rubric.items.len(),
                    critical_items: scenario
                        .rubric
                        .items
                        .iter()
                        .filter(|item| item.critical)
                        .count(),
                    modified,
                    file,
                    valid: true,
                    status,
                    warnings: validated.warnings,
                    error: None,
                }
            }
            Err(error) => ScenarioEntry {
                id: expected_id.to_string(),
                title: String::new(),
                version: String::new(),
                license: String::new(),
                difficulty: None,
                incident_type: None,
                duration_minutes: None,
                facts: 0,
                rubric_items: 0,
                critical_items: 0,
                modified,
                file,
                valid: false,
                status: "draft".to_string(),
                warnings: Vec::new(),
                error: Some(error),
            },
        }
    }

    fn write_default_manifest(&self, pack_dir: &Path) -> Result<(), String> {
        fs::create_dir_all(pack_dir).map_err(|error| format!("create pack directory: {error}"))?;
        let manifest = PackManifest {
            format: FORMAT_ID.to_string(),
            id: DEFAULT_PACK_ID.to_string(),
            name: "Local agency pack".to_string(),
            version: "0.1.0".to_string(),
            visibility: PackVisibility::Private,
            license: "proprietary".to_string(),
            owner: PackOwner {
                agency: "Local agency".to_string(),
                contact: None,
            },
            requires: PackRequires {
                scenario_format: ">=0.1 <0.2".to_string(),
            },
            protocols: Vec::<ProtocolRef>::new(),
            sources: PackSources { export: false },
            exported: None,
        };
        write_private(
            &pack_dir.join("callcraft-pack.yaml"),
            &manifest.render_yaml()?,
        )
    }
}

/// Validate a pack directory without installing it. Used by `callcraft pack validate`.
pub fn validate_pack_dir(path: &Path) -> Result<PackView, String> {
    let store = PackStore::with_root(
        path.parent()
            .ok_or_else(|| "pack path has no parent".to_string())?
            .to_path_buf(),
    );
    let id = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "pack path has no directory name".to_string())?;
    if !safe_id(id) {
        return Err(format!("invalid pack directory name '{id}'"));
    }
    let view = store.read_pack(id, path);
    if let Some(error) = &view.error {
        return Err(error.clone());
    }
    let mut errors = Vec::new();
    for scenario in &view.scenarios {
        if let Some(error) = &scenario.error {
            errors.push(format!("{}: {error}", scenario.id));
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(view)
}

fn read_ledger(path: &Path) -> Result<ApprovalLedger, String> {
    if !path.is_file() {
        return Ok(ApprovalLedger::default());
    }
    let text = fs::read_to_string(path).map_err(|error| format!("read approvals: {error}"))?;
    let ledger = ApprovalLedger::parse_yaml(&text)?;
    ledger.validate()?;
    Ok(ledger)
}

fn write_ledger(path: &Path, ledger: &ApprovalLedger) -> Result<(), String> {
    write_private(path, &ledger.render_yaml()?)
}

fn visibility_label(visibility: PackVisibility) -> &'static str {
    match visibility {
        PackVisibility::Private => "private",
        PackVisibility::Shareable => "shareable",
        PackVisibility::Public => "public",
    }
}

fn modified(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| OffsetDateTime::from(modified).format(&Rfc3339).ok())
        .unwrap_or_default()
}

fn default_pack_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("callcraft").join("packs");
        }
        if let Ok(roaming) = env::var("APPDATA") {
            return PathBuf::from(roaming).join("callcraft").join("packs");
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("callcraft")
            .join("packs");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(data) = env::var("XDG_DATA_HOME") {
            return PathBuf::from(data).join("callcraft").join("packs");
        }
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("callcraft")
                .join("packs");
        }
    }
    PathBuf::from(".callcraft").join("packs")
}

pub(super) fn write_bytes_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("pack directory: {error}"))?;
    }
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, bytes).map_err(|error| format!("pack write: {error}"))?;
    set_private_permissions(&temp_path)?;
    fs::rename(&temp_path, path).map_err(|error| format!("pack replace: {error}"))
}

fn write_private(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("pack directory: {error}"))?;
    }
    let temp_path = path.with_extension("yaml.tmp");
    fs::write(&temp_path, text).map_err(|error| format!("pack write: {error}"))?;
    set_private_permissions(&temp_path)?;
    fs::rename(&temp_path, path).map_err(|error| format!("pack replace: {error}"))
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("pack permissions: {error}"))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario_yaml(id: &str) -> String {
        format!(
            r#"format: callcraft-scenario/v0.1
id: {id}
title: Test call
version: 1.0.0
license: CC-BY-4.0
language: en-US
incident_type: medical
difficulty: basic
duration_minutes: 5
persona:
  display_name: Test
  age: 40
facts:
  - id: address
    value: "1 Test Street"
    disclosure: on_question
    triggers: [where, address]
rubric:
  scoring: checklist
  items:
    - id: address_verified
      description: Confirms the address
      critical: true
      anchors:
        any_of:
          - transcript_regex: "(?i)address"
safety:
  disclaimer: Training simulation.
"#
        )
    }

    #[test]
    fn saves_lists_and_opens_scenarios() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = PackStore::with_root(dir.path().join("packs"));
        let result = store
            .save_default(&scenario_yaml("test-call"))
            .expect("save");
        assert_eq!(result.pack_id, "local");
        assert_eq!(result.scenario.id, "test-call");
        assert!(result.scenario.valid);

        let listing = store.list();
        assert_eq!(listing.packs.len(), 1);
        assert_eq!(listing.packs[0].scenarios.len(), 1);
        let (yaml, validated) = store.open("local", "test-call").expect("open");
        assert_eq!(validated.scenario.id, "test-call");
        assert!(yaml.contains("Test call"));
    }

    #[test]
    fn rejects_unknown_pack_and_bad_ids() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = PackStore::with_root(dir.path().to_path_buf());
        assert!(store.save("other", &scenario_yaml("test-call")).is_err());
        assert!(store.save("local", &scenario_yaml("test-call")).is_ok());
        assert!(store.open("local", "../test-call").is_err());
    }
}
