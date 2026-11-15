# Roadmap

Living document. **Every slice PR updates this file** and links the issues it
closes. Granular work stays in GitHub issues; this page owns direction, status,
and the decisions that should not live only in chat.

Last updated: 2026-11-15.

## Product thesis

Callcraft is a free, MIT-licensed alternative to closed 911 dispatcher training
simulators. The product is the **open scenario, rubric, and agency-pack
format**; the repo is the reference implementation. Agency content stays local
and agency-owned. The AI is a swappable adapter, not the product.

Non-goals: live CAD integration, live-call or CJI handling, certification
claims, reproducing vendor protocol text, telemetry, or cloud storage by
default.

## Now

### Review evidence and library discovery — next

- [ ] Event evidence timestamps so a rubric event can anchor to the log.
- [ ] A hideable observer margin for live runs.
- [ ] Library search, tags, and status filters.
- [ ] A library test pass for switching, empty/error packs, and target selection.

Then bulk ingestion: CSV for facts/rubric/geography, then a geography picker.

## Shipped

### Format and runner foundation

- `callcraft-scenario/v0.1` schema, semantic validation, content-policy lint.
- Deterministic caller restricted to the closed fact base.
- Session transcript, observable checklist anchors, critical items, evidence,
  JSON reports, byte-identical replay, stage latencies.
- CLI: `validate`, `demo`, `sim`, `serve`, `pack validate`.

### Desktop and voice

- Tauri 2 shell on a loopback server, per-launch token, Host checks.
- STT/TTS adapter registries: OpenAI-compatible, OpenAI, whisper.cpp,
  ElevenLabs, Deepgram, Piper shim.
- LLM caller decision contract (fact ids and style only, server-rendered words)
  with provider registry, keychain-backed settings, timeouts, breaker,
  caller-integrity checks, byte-safe report latency.

### Security and provider hardening

- Exact Host matching, header-only tokens, HTTPS/private-network enforcement,
  origin-bound keys, env key binding, fail-closed provider ids.
- Split drift vs adapter failures, per-session turn serialization, stage
  budgets, typed STT/TTS APIs, unified config builders.
- PR #11; issue #10.

### Client: Logbook

- Prepare / Author / Settings utility pages, dedicated Run workspace, Review
  beside the log.
- Paper-light default with a dim token set, ruled transcript, observer marks,
  evidence links, responsive 1280/800 layouts, accessibility repairs.
- Design record: [DESIGN.md](DESIGN.md).
- PR #12.

### Pack management and agency profile

- Pack creation with id, display name, visibility, license, owner agency, and
  contact.
- Pack settings/display-name edits; pack ids stay immutable.
- Multi-pack Save target in Author; library pack switcher.
- Empty and error pack states in the library.
- PR pending.

### Approval and immutable content identity

- Hash-bound `approvals.yaml` ledger: `draft`, `approved`, `retired`, `by`, `at`,
  note, imported marker.
- Approval survives an identical save; a real edit returns to draft.
- Approved-only pack starts; raw YAML stays Test drive marked UNAPPROVED.
- Reports stamp `scenario_sha256`, `pack_id`, `pack_version`, `approval`.
- Imported approvals are carried only on hash match and only when explicitly
  accepted.
- Explicit YAML parser budgets plus alias-amplification and deep-nesting
  fixtures.
- PR #16; issue #14.

### Agency packs

- `callcraft-pack/v0.1` manifest, OS-local store, Save to pack, scenario
  library, pack-scoped start, `callcraft pack validate`.
- PR #13; [PACKS.md](PACKS.md).
- Checksummed zip export, two-step import review, install/replace, backup and
  rollback, zip-safety caps.
- PR #15.
- Approval ledger, approved-only starts, report stamping, and bounded YAML: PR #16.

## Next

Ordered, not dated.

1. **Finish the approval slice** (#14) and merge the pack stack.
2. **Pack management**: create/rename packs, agency profile (name, visibility,
   license, owner), multi-pack Save target.
3. **Bulk ingestion**: CSV templates for facts, rubric items, and geography;
   scenario migration from other instances. No SOP content in the public repo.
4. **Geography packs**: local CSV/YAML geography, placeholders/references, an
   authoring picker. Real geography stays in private packs.
5. **Discovery**: search/tags/status filters in the library.
6. **SOP extraction (gated)**: only after the pilot proves authoring time.
   Paraphrased rubric suggestions with page/section citations, accepted one at
   a time, draft by default, local-first, never vendor-owned protocol text.
7. **Pilot hardening**: Windows installer/signing, native file dialog, backup
   restore drill, pilot measurements.

Deferred until there is a real commons: pack signing and a shared registry.
Cloud sync is not planned; packs are files.

## Pilot gate

The 30-day pilot in [PILOT-30-DAY.md](../PILOT-30-DAY.md) decides whether this
becomes a product. All must hold:

1. Three scenarios authored by trainers from their own SOPs, each under 60
   minutes, no engineer present.
2. Machine checklist agrees with blind human grading on at least 85% of items;
   every critical item agrees.
3. A pack exports, imports on a second instance, and diffs readably in git.
4. Cost and latency measured and published: under $1/session and under 1.5s p50
   round trip.
5. The training officer asks to keep using it unprompted.

## Decisions

| Date | Decision | Where |
|---|---|---|
| 2026-11-15 | Scenario/rubric format is the product; adapters are swappable | [SPEC-scenario-format.md](../SPEC-scenario-format.md) |
| 2026-11-15 | Client motif is Logbook: paper-light, ruled record, no console cosplay | [DESIGN.md](DESIGN.md) |
| 2026-11-15 | Packs are directories; zip is transport, not the source of truth | [PACKS.md](PACKS.md) |
| 2026-11-15 | Approval binds to stored file SHA-256; edits return to draft | [PACKS.md](PACKS.md) |
| 2026-11-15 | Import supports install/replace only; no field-level merge | [PACKS.md](PACKS.md) |
| 2026-11-15 | `sources/` never exports; vendor protocol text is not ingested | [PACKS.md](PACKS.md) |
| 2026-11-15 | Reports identify exact scenario bytes, pack version, and approval | [SPEC-scenario-format.md](../SPEC-scenario-format.md) |
| 2026-11-15 | Untrusted YAML parses under explicit event, depth, alias, and size budgets | `validate.rs` |

## Keeping this current

- A slice is not done until its roadmap section and issue links are updated in
  the same PR.
- New direction goes in **Decisions** before implementation, not only in a
  conversation.
- Completed work moves from **Now/Next** to **Shipped** with its PR and design
  doc links.
- [PR template](../.github/PULL_REQUEST_TEMPLATE.md) includes the roadmap check.
