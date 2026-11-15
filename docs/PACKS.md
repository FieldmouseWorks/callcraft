# Agency packs

Status: slices 1 and 2 implemented. Local storage, Save to pack, library
browse/start, pack-scoped session start, `callcraft pack validate`, checksummed
export/import with dry-run review, backups, and rollback.

An agency pack is a directory under the OS local-data directory. It is never
committed to the repo and never uploaded by the app.

## Layout

```text
packs/<pack-id>/
  callcraft-pack.yaml
  scenarios/<scenario-id>.scenario.yaml
```

The directory is the source of truth. There is no manifest file list and no
database to drift. `scenarios/<id>.scenario.yaml` must contain a scenario whose
`id` matches the filename.

Default store:

- Windows: `%LOCALAPPDATA%\callcraft\packs\` (not Roaming/OneDrive)
- Linux: `$XDG_DATA_HOME/callcraft/packs/` or `~/.local/share/callcraft/packs/`
- macOS: `~/Library/Application Support/callcraft/packs/`
- Override: `CALLCRAFT_PACK_DIR=/path/to/packs`

The app warns when the store path looks like a cloud-sync folder.

## Manifest

```yaml
format: callcraft-pack/v0.1
id: local
name: Local agency pack
version: 0.1.0
visibility: private       # private | shareable | public
license: proprietary
owner:
  agency: Local agency
  contact: null
requires:
  scenario_format: ">=0.1 <0.2"
protocols: []             # references only, never vendor protocol text
sources:
  export: false
```

`id` must match `^[a-z0-9][a-z0-9-]{0,63}$`; `version` is `MAJOR.MINOR.PATCH`.
Protocol entries are `{id, title, owner: agency|vendor, revision?}`.

The first save creates the `local` pack if no pack is installed. A future slice
will add pack creation/rename and a first-run agency profile.

## CLI

```bash
callcraft pack validate <pack-dir>
```

It validates the manifest, every scenario, filename/id agreement, and reports
facts, rubric items, and critical items. It exits non-zero on the first pack
error.

## API

- `GET /api/packs` — packs, scenarios, default pack id, sync-folder warning.
- `POST /api/packs/scenarios` — save YAML into the default pack.
- `POST /api/packs/{pack_id}/scenarios` — save YAML into a named pack.
- `GET /api/packs/{pack_id}/scenarios/{scenario_id}` — read YAML for editing.
- `POST /api/packs/{pack_id}/export` — checksummed `.callcraftpack.zip` download.
- `POST /api/packs/import/inspect` — dry-run review of an uploaded zip.
- `POST /api/packs/import?mode=install|replace` — commit an inspected zip.
- `POST /api/packs/{pack_id}/rollback` — restore the latest pre-import backup.
- `POST /api/sessions` with `{"pack_id","scenario_id"}` — start a saved scenario.
  Raw YAML remains supported as a test-drive path.

Pack and scenario ids are validated before any path join. Saves validate the
scenario, write to a temporary file, set mode `0600` on Unix, then rename.
Scenario save paths never accept arbitrary filesystem paths.

## Transport

Export writes `callcraft-pack.yaml`, `scenarios/`, `geography/` if present, and
a generated `checksums.sha256`. `sources/` is never exported. Import requires
the checksum file, verifies every content file, and rejects unknown files.

Import is two steps: inspect, then commit. The review shows the pack metadata,
checksum result, and a per-scenario state: `new`, `identical`, `changed`,
`diverged`, or `invalid`, plus local scenarios a replace would remove. Actions
are `install` for a new pack and `replace` for an installed pack. There is no
field-level merge.

Before a replace, the existing pack is exported to a backup zip under
`backups/<pack-id>/`. The ten newest backups are kept. Rollback restores the
latest backup and consumes it. A failed import leaves the current pack intact.

Zip handling rejects absolute paths, `..`, backslashes, drive letters, symlinks,
directory entries, case-insensitive duplicates, Windows reserved names, and
trailing dots/spaces. Entry count, per-file size, total uncompressed size, and
compression ratio are capped. The import endpoint accepts zip bytes only; it
never accepts a filesystem path.

## Status and approvals

Library entries are `draft` for now. The approval ledger is slice 3: it will
bind `approved` to the scenario file hash, and an edit will return the entry to
`draft`. The session-start response stamps `approval: "none"`; report scenario
hash, pack id, pack version, and approval stamping land with the ledger.

On a single-user loopback install, approval is a record, not access control.

## Deferred

- Approval ledger, report stamping, server-enforced approved-only starts.
- YAML parser resource limits and malicious-YAML fixtures.
- Pack creation/rename, agency profile, multi-pack Save target.
- Merge, CSV import, SOP extraction, search/tags, geography picker, signing,
  cloud sync.
- Pack creation/rename and agency profile.
- Merge, CSV import, SOP extraction, search/tags, geography picker, signing.
- YAML parser resource limits and malicious-pack corpus.
