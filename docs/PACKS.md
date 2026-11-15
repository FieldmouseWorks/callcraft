# Agency packs

Status: slice 1 implemented. Local storage, Save to pack, library browse/start,
pack-scoped session start, and `callcraft pack validate`.

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
- `POST /api/sessions` with `{"pack_id","scenario_id"}` — start a saved scenario.
  Raw YAML remains supported as a test-drive path.

Pack and scenario ids are validated before any path join. Saves validate the
scenario, write to a temporary file, set mode `0600` on Unix, then rename.
Scenario save paths never accept arbitrary filesystem paths.

## Status and approvals

Library entries are `draft` for now. The approval ledger is slice 3: it will
bind `approved` to the scenario file hash, and an edit will return the entry to
`draft`. The session-start response stamps `approval: "none"`; report scenario
hash, pack id, pack version, and approval stamping land with the ledger.

On a single-user loopback install, approval is a record, not access control.

## Deferred

- Export/import zip, dry-run review, install/replace, rollback.
- Approval ledger, report stamping, server-enforced approved-only starts.
- Pack creation/rename and agency profile.
- Merge, CSV import, SOP extraction, search/tags, geography picker, signing.
- YAML parser resource limits and malicious-pack corpus.
