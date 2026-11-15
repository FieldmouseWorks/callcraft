//! Pack export/import transport: checksummed zips, dry-run review, and
//! backup/rollback.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use callcraft_core::pack::{PackExport, PackManifest, safe_id, validate_scenario_text};
use callcraft_core::validate::validate_yaml;

use super::{PackStore, write_bytes_private};

const MAX_ENTRIES: usize = 2_000;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
const KEEP_BACKUPS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    Install,
    Replace,
}

impl ImportMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "install" => Ok(Self::Install),
            "replace" => Ok(Self::Replace),
            other => Err(format!("unknown import mode '{other}'")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Replace => "replace",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportScenario {
    pub id: String,
    pub title: String,
    pub version: String,
    pub state: String,
    pub critical_items: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub pack_id: String,
    pub name: String,
    pub version: String,
    pub visibility: String,
    pub license: String,
    pub action: String,
    pub checksum: String,
    pub can_import: bool,
    pub scenarios: Vec<ImportScenario>,
    pub local_only: Vec<String>,
    pub errors: Vec<String>,
}

struct StagedPack {
    staging: TempDir,
    report: ImportReport,
}

impl PackStore {
    /// Build a checksummed zip for transport. `sources/` is never exported.
    pub fn export(&self, pack_id: &str) -> Result<(Vec<u8>, String), String> {
        if !safe_id(pack_id) {
            return Err(format!("invalid pack id '{pack_id}'"));
        }
        let pack_dir = self.root().join(pack_id);
        let manifest_path = pack_dir.join("callcraft-pack.yaml");
        let manifest_text = fs::read_to_string(&manifest_path)
            .map_err(|error| format!("read pack manifest: {error}"))?;
        let manifest = PackManifest::parse_yaml(&manifest_text)?;
        manifest.validate()?;

        let mut files = collect_files(pack_dir.clone())?;
        let mut exported = manifest.clone();
        exported.exported = Some(PackExport {
            at: crate::util::now_iso(),
            by: "callcraft".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
        });
        let exported_manifest = exported.render_yaml()?;
        files.insert(
            "callcraft-pack.yaml".to_string(),
            exported_manifest.into_bytes(),
        );

        let mut names: Vec<String> = files.keys().cloned().collect();
        names.sort();
        let mut checksums = String::new();
        for name in &names {
            checksums.push_str(&format!(
                "{}  {name}\n",
                hash_hex(files.get(name).expect("file entry"))
            ));
        }
        files.insert("checksums.sha256".to_string(), checksums.into_bytes());

        let mut names: Vec<String> = files.keys().cloned().collect();
        names.sort();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        for name in names {
            writer
                .start_file(&name, options)
                .map_err(|error| format!("zip entry '{name}': {error}"))?;
            writer
                .write_all(files.get(&name).expect("file entry"))
                .map_err(|error| format!("zip write '{name}': {error}"))?;
        }
        let cursor = writer
            .finish()
            .map_err(|error| format!("zip finish: {error}"))?;
        let filename = format!("{}-{}.callcraftpack.zip", manifest.id, manifest.version);
        Ok((cursor.into_inner(), filename))
    }

    /// Stage and validate an incoming zip without changing the store.
    pub fn inspect_bytes(&self, bytes: &[u8]) -> Result<ImportReport, String> {
        Ok(self.stage(bytes)?.report)
    }

    /// Import a zip after the caller has reviewed the dry run.
    pub fn import_bytes(&self, bytes: &[u8], mode: ImportMode) -> Result<ImportReport, String> {
        let staged = self.stage(bytes)?;
        if staged.report.action != mode.as_str() {
            return Err(format!(
                "import action is '{}', not '{}'",
                staged.report.action,
                mode.as_str()
            ));
        }
        self.commit(staged, true)
    }

    /// Restore the most recent pre-import backup for a pack.
    pub fn rollback(&self, pack_id: &str) -> Result<ImportReport, String> {
        let backup = self.latest_backup(pack_id)?;
        let bytes = fs::read(&backup).map_err(|error| format!("read backup: {error}"))?;
        let staged = self.stage(&bytes)?;
        if staged.report.action != ImportMode::Replace.as_str() {
            return Err("backup is not a replacement for an installed pack".to_string());
        }
        let report = self.commit(staged, false)?;
        fs::remove_file(&backup).map_err(|error| format!("remove used backup: {error}"))?;
        Ok(report)
    }

    fn stage(&self, bytes: &[u8]) -> Result<StagedPack, String> {
        fs::create_dir_all(self.root()).map_err(|error| format!("create pack store: {error}"))?;
        let staging = tempfile::Builder::new()
            .prefix(".callcraft-import-")
            .tempdir_in(self.root())
            .map_err(|error| format!("create import staging: {error}"))?;

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| format!("zip archive: {error}"))?;
        if archive.len() > MAX_ENTRIES {
            return Err(format!("pack has too many entries (max {MAX_ENTRIES})"));
        }

        let mut seen = HashSet::new();
        let mut total = 0u64;
        let mut actual_total = 0u64;
        let mut extracted = HashMap::new();
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| format!("zip entry {index}: {error}"))?;
            let raw_name = entry
                .name()
                .map_err(|error| format!("zip entry name: {error}"))?
                .to_string();
            if entry.is_dir() {
                return Err(format!(
                    "directory zip entries are not accepted: {raw_name}"
                ));
            }
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(format!("symlink zip entry is not accepted: {raw_name}"));
            }
            let name = safe_zip_path(&raw_name)?;
            if !seen.insert(name.to_ascii_lowercase()) {
                return Err(format!("duplicate zip entry: {raw_name}"));
            }
            let size = entry.size();
            if size > MAX_FILE_BYTES {
                return Err(format!("zip entry too large: {raw_name}"));
            }
            if entry.compressed_size() == 0 && size > 0 {
                return Err(format!("invalid compressed size for {raw_name}"));
            }
            if entry.compressed_size() > 0
                && size / entry.compressed_size().max(1) > MAX_COMPRESSION_RATIO
            {
                return Err(format!("zip entry compression ratio too high: {raw_name}"));
            }
            total = total.saturating_add(size);
            if total > MAX_TOTAL_BYTES {
                return Err("pack expands beyond the size limit".to_string());
            }
            let mut data = Vec::new();
            entry
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|error| format!("read zip entry {raw_name}: {error}"))?;
            if data.len() as u64 > MAX_FILE_BYTES {
                return Err(format!("zip entry too large: {raw_name}"));
            }
            actual_total = actual_total.saturating_add(data.len() as u64);
            if actual_total > MAX_TOTAL_BYTES {
                return Err("pack expands beyond the size limit".to_string());
            }
            let path = staging.path().join(&name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create import directory: {error}"))?;
            }
            write_bytes_private(&path, &data)?;
            extracted.insert(name, hash_hex(&data));
        }

        let manifest_path = staging.path().join("callcraft-pack.yaml");
        let manifest_text = fs::read_to_string(&manifest_path)
            .map_err(|_| "pack zip is missing callcraft-pack.yaml".to_string())?;
        let manifest = PackManifest::parse_yaml(&manifest_text)?;
        manifest.validate()?;

        let checksums_path = staging.path().join("checksums.sha256");
        let checksums_text = fs::read_to_string(&checksums_path)
            .map_err(|_| "pack zip is missing checksums.sha256".to_string())?;
        let checksums = parse_checksums(&checksums_text)?;
        for (path, expected) in &checksums {
            let actual = extracted
                .get(path)
                .ok_or_else(|| format!("checksum references missing file '{path}'"))?;
            if actual != expected {
                return Err(format!("checksum mismatch for '{path}'"));
            }
        }
        for path in extracted.keys() {
            if path == "checksums.sha256" {
                continue;
            }
            if !checksums.contains_key(path) {
                return Err(format!("file '{path}' is not covered by checksums.sha256"));
            }
        }

        let mut scenarios = Vec::new();
        let mut errors = Vec::new();
        let scenarios_dir = staging.path().join("scenarios");
        if let Ok(entries) = fs::read_dir(&scenarios_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(id) = name.strip_suffix(".scenario.yaml") else {
                    continue;
                };
                let text = fs::read_to_string(&path)
                    .map_err(|error| format!("read imported scenario: {error}"))?;
                match validate_scenario_text(&text, id, false) {
                    Ok(validated) => {
                        let scenario = validated.scenario;
                        scenarios.push(ImportScenario {
                            id: id.to_string(),
                            title: scenario.title,
                            version: scenario.version,
                            state: String::new(),
                            critical_items: scenario
                                .rubric
                                .items
                                .iter()
                                .filter(|item| item.critical)
                                .count(),
                            errors: Vec::new(),
                            warnings: validated.warnings,
                        });
                    }
                    Err(error) => {
                        errors.push(format!("{id}: {error}"));
                        scenarios.push(ImportScenario {
                            id: id.to_string(),
                            title: String::new(),
                            version: String::new(),
                            state: "invalid".to_string(),
                            critical_items: 0,
                            errors: vec![error],
                            warnings: Vec::new(),
                        });
                    }
                }
            }
        }
        scenarios.sort_by_key(|scenario| scenario.id.clone());

        let local_dir = self.root().join(&manifest.id);
        let installed = local_dir.join("callcraft-pack.yaml").is_file();
        if local_dir.exists() && !installed {
            errors.push(format!(
                "pack directory '{}' exists without a manifest",
                manifest.id
            ));
        }
        let local_hashes = if installed {
            local_scenario_hashes(&local_dir).unwrap_or_default()
        } else {
            HashMap::new()
        };
        let local_only = local_hashes
            .keys()
            .filter(|id| !scenarios.iter().any(|scenario| &scenario.id == *id))
            .cloned()
            .collect::<Vec<_>>();
        for scenario in &mut scenarios {
            if scenario.state == "invalid" {
                continue;
            }
            scenario.state = match local_hashes.get(&scenario.id) {
                None => "new".to_string(),
                Some(local) if local.hash == incoming_hash(&scenario.id, staging.path()) => {
                    "identical".to_string()
                }
                Some(local) if local.version == scenario.version => "diverged".to_string(),
                Some(_) => "changed".to_string(),
            };
        }

        let report = ImportReport {
            pack_id: manifest.id.clone(),
            name: manifest.name,
            version: manifest.version,
            visibility: format!("{:?}", manifest.visibility).to_lowercase(),
            license: manifest.license,
            action: if installed { "replace" } else { "install" }.to_string(),
            checksum: "ok".to_string(),
            can_import: errors.is_empty(),
            scenarios,
            local_only,
            errors,
        };
        Ok(StagedPack { staging, report })
    }

    fn commit(&self, staged: StagedPack, backup: bool) -> Result<ImportReport, String> {
        if !staged.report.can_import {
            return Err("pack has blocking errors".to_string());
        }
        let pack_id = staged.report.pack_id.clone();
        let target = self.root().join(&pack_id);
        let staging_path = staged.staging.path().to_path_buf();
        let old_path = if target.exists() {
            if backup {
                self.backup_pack(&pack_id)?;
            }
            let old = self.root().join(format!(".old-{}", uuid::Uuid::new_v4()));
            fs::rename(&target, &old).map_err(|error| format!("move old pack: {error}"))?;
            Some(old)
        } else {
            None
        };
        if let Err(error) = fs::rename(&staging_path, &target) {
            if let Some(old) = &old_path {
                let _ = fs::rename(old, &target);
            }
            return Err(format!("install pack: {error}"));
        }
        if let Some(old) = old_path {
            let _ = fs::remove_dir_all(old);
        }
        Ok(staged.report)
    }

    fn backup_pack(&self, pack_id: &str) -> Result<(), String> {
        let (bytes, _) = self.export(pack_id)?;
        let backups = self.backup_dir(pack_id);
        fs::create_dir_all(&backups)
            .map_err(|error| format!("create backup directory: {error}"))?;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let hash = hash_hex(&bytes);
        let path = backups.join(format!("{now}-{}.zip", &hash[..12]));
        write_bytes_private(&path, &bytes)?;
        prune_backups(&backups)?;
        Ok(())
    }

    fn latest_backup(&self, pack_id: &str) -> Result<PathBuf, String> {
        let backups = self.backup_dir(pack_id);
        let mut files = backup_files(&backups)?;
        files.sort();
        files
            .pop()
            .ok_or_else(|| format!("no backups for pack '{pack_id}'"))
    }

    fn backup_dir(&self, pack_id: &str) -> PathBuf {
        let parent = self.root().parent().unwrap_or(self.root());
        parent.join("backups").join(pack_id)
    }
}

fn collect_files(pack_dir: PathBuf) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut files = HashMap::new();
    let scenarios = pack_dir.join("scenarios");
    if let Ok(entries) = fs::read_dir(&scenarios) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(id) = name.strip_suffix(".scenario.yaml") else {
                continue;
            };
            if !safe_id(id) {
                return Err(format!("invalid scenario filename '{name}'"));
            }
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("read scenario '{name}': {error}"))?;
            validate_scenario_text(&text, id, false)?;
            files.insert(format!("scenarios/{name}"), text.into_bytes());
        }
    }
    let geography = pack_dir.join("geography");
    if let Ok(entries) = fs::read_dir(&geography) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(id) = name.strip_suffix(".yaml") else {
                continue;
            };
            if !safe_geo_id(id) {
                return Err(format!("invalid geography filename '{name}'"));
            }
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("read geography '{name}': {error}"))?;
            files.insert(format!("geography/{name}"), text.into_bytes());
        }
    }
    Ok(files)
}

fn safe_zip_path(name: &str) -> Result<String, String> {
    if name.is_empty()
        || name.len() > 255
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(format!("unsafe zip path '{name}'"));
    }
    let parts: Vec<&str> = name.split('/').collect();
    if parts.iter().any(|part| {
        part.is_empty()
            || *part == "."
            || *part == ".."
            || part.ends_with('.')
            || part.ends_with(' ')
            || is_reserved_windows_name(part)
    }) {
        return Err(format!("unsafe zip path '{name}'"));
    }
    match parts.as_slice() {
        ["callcraft-pack.yaml"] | ["checksums.sha256"] => Ok(name.to_string()),
        ["scenarios", file] => {
            let id = file
                .strip_suffix(".scenario.yaml")
                .ok_or_else(|| format!("unexpected scenario file '{name}'"))?;
            if safe_id(id) {
                Ok(name.to_string())
            } else {
                Err(format!("invalid scenario id in '{name}'"))
            }
        }
        ["geography", file] => {
            let id = file
                .strip_suffix(".yaml")
                .ok_or_else(|| format!("unexpected geography file '{name}'"))?;
            if safe_geo_id(id) {
                Ok(name.to_string())
            } else {
                Err(format!("invalid geography id in '{name}'"))
            }
        }
        _ => Err(format!("unexpected file in pack zip: '{name}'")),
    }
}

fn safe_geo_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    id.len() <= 64
        && id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        })
}

fn is_reserved_windows_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn parse_checksums(text: &str) -> Result<HashMap<String, String>, String> {
    let mut checksums = HashMap::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (hash, path) = line
            .split_once("  ")
            .ok_or_else(|| format!("malformed checksum line: {line}"))?;
        if hash.len() != 64 || !hash.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(format!("invalid checksum hash for '{path}'"));
        }
        let path = safe_zip_path(path)?;
        if path == "checksums.sha256" {
            return Err("checksums.sha256 cannot checksum itself".to_string());
        }
        if checksums
            .insert(path.clone(), hash.to_ascii_lowercase())
            .is_some()
        {
            return Err(format!("duplicate checksum for '{path}'"));
        }
    }
    Ok(checksums)
}

struct LocalScenario {
    hash: String,
    version: String,
}

fn local_scenario_hashes(pack_dir: &Path) -> Result<HashMap<String, LocalScenario>, String> {
    let mut hashes = HashMap::new();
    let scenarios = pack_dir.join("scenarios");
    if let Ok(entries) = fs::read_dir(&scenarios) {
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
            let bytes = fs::read(&path).map_err(|error| format!("read local scenario: {error}"))?;
            let version = fs::read_to_string(&path)
                .ok()
                .and_then(|text| validate_yaml(&text, false).ok())
                .map(|validated| validated.scenario.version)
                .unwrap_or_default();
            hashes.insert(
                id.to_string(),
                LocalScenario {
                    hash: hash_hex(&bytes),
                    version,
                },
            );
        }
    }
    Ok(hashes)
}

fn incoming_hash(id: &str, staging: &Path) -> String {
    fs::read(
        staging
            .join("scenarios")
            .join(format!("{id}.scenario.yaml")),
    )
    .map(|bytes| hash_hex(&bytes))
    .unwrap_or_default()
}

fn hash_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn backup_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn prune_backups(dir: &Path) -> Result<(), String> {
    let mut files = backup_files(dir)?;
    files.sort();
    while files.len() > KEEP_BACKUPS {
        let oldest = files.remove(0);
        fs::remove_file(&oldest).map_err(|error| format!("prune backup: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn scenario_yaml(id: &str, title: &str) -> String {
        format!(
            r#"format: callcraft-scenario/v0.1
id: {id}
title: {title}
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

    fn zip_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("zip file");
            writer.write_all(bytes).expect("zip write");
        }
        writer.finish().expect("zip finish").into_inner()
    }

    #[test]
    fn export_import_round_trips_scenario_bytes() {
        let source_dir = tempfile::tempdir().expect("source dir");
        let source = PackStore::with_root(source_dir.path().join("packs"));
        source
            .save_default(&scenario_yaml("test-call", "Test call"))
            .expect("save");
        let (bytes, filename) = source.export("local").expect("export");
        assert!(filename.ends_with(".callcraftpack.zip"));

        let target_dir = tempfile::tempdir().expect("target dir");
        let target = PackStore::with_root(target_dir.path().join("packs"));
        let report = target.inspect_bytes(&bytes).expect("inspect");
        assert_eq!(report.action, "install");
        assert_eq!(report.scenarios[0].state, "new");
        assert!(report.can_import);
        target
            .import_bytes(&bytes, ImportMode::Install)
            .expect("import");
        let (yaml, validated) = target.open("local", "test-call").expect("open");
        assert_eq!(validated.scenario.title, "Test call");
        assert!(yaml.contains("Test call"));
    }

    #[test]
    fn replace_creates_a_rollback_point() {
        let source_dir = tempfile::tempdir().expect("source dir");
        let source = PackStore::with_root(source_dir.path().join("packs"));
        source
            .save_default(&scenario_yaml("test-call", "First title"))
            .expect("save");
        let (first, _) = source.export("local").expect("export first");

        let target_dir = tempfile::tempdir().expect("target dir");
        let target = PackStore::with_root(target_dir.path().join("packs"));
        target
            .import_bytes(&first, ImportMode::Install)
            .expect("install first");

        source
            .save_default(&scenario_yaml("test-call", "Second title"))
            .expect("save second");
        let (second, _) = source.export("local").expect("export second");
        let report = target.inspect_bytes(&second).expect("inspect replace");
        assert_eq!(report.action, "replace");
        assert_eq!(report.scenarios[0].state, "diverged");
        target
            .import_bytes(&second, ImportMode::Replace)
            .expect("replace");
        assert_eq!(
            target
                .open("local", "test-call")
                .expect("open replaced")
                .1
                .scenario
                .title,
            "Second title"
        );

        target.rollback("local").expect("rollback");
        assert_eq!(
            target
                .open("local", "test-call")
                .expect("open restored")
                .1
                .scenario
                .title,
            "First title"
        );
    }

    #[test]
    fn rejects_zip_slip_paths() {
        let dir = tempfile::tempdir().expect("dir");
        let store = PackStore::with_root(dir.path().join("packs"));
        let bytes = zip_entries(&[("../evil.yaml", b"no")]);
        let error = store.inspect_bytes(&bytes).expect_err("zip slip");
        assert!(error.contains("unsafe") || error.contains("unexpected"));
    }

    #[test]
    fn requires_checksums() {
        let dir = tempfile::tempdir().expect("dir");
        let store = PackStore::with_root(dir.path().join("packs"));
        let manifest = PackStore::with_root(dir.path().join("unused"));
        drop(manifest);
        let bytes = zip_entries(&[
            (
                "callcraft-pack.yaml",
                br#"format: callcraft-pack/v0.1
id: local
name: Test
version: 0.1.0
visibility: private
license: proprietary
owner:
  agency: Test
requires:
  scenario_format: ">=0.1 <0.2"
"#,
            ),
            (
                "scenarios/test-call.scenario.yaml",
                scenario_yaml("test-call", "Test").as_bytes(),
            ),
        ]);
        let error = store.inspect_bytes(&bytes).expect_err("missing checksums");
        assert!(error.contains("checksums.sha256"));
    }
}
