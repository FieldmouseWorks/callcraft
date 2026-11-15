# Callcraft

[![ci](https://github.com/FieldmouseWorks/callcraft/actions/workflows/ci.yml/badge.svg)](https://github.com/FieldmouseWorks/callcraft/actions/workflows/ci.yml)

A free, MIT-licensed alternative to closed 911 dispatcher training simulators.
The product is the **open scenario and rubric format**; this repo is the
reference implementation: a validator, a text-mode runner, and a checklist
scorer. Voice and model adapters come later behind the same format.

## Why

Every commercial simulator locks the agency's training content inside the
vendor. Scenarios, rubrics, and scoring don't leave when the contract does.
This project makes the content layer open, portable, and owned by the agency.
The AI is a swappable commodity behind an adapter.

## Quick start

```bash
cargo test -p callcraft-core
cargo run -p callcraft-cli -- validate examples/chest-pain-001.scenario.yaml
cargo run -p callcraft-cli -- demo examples/chest-pain-001.scenario.yaml       # scripted good/bad calls
cargo run -p callcraft-cli -- sim examples/chest-pain-001.scenario.yaml --interactive
cargo run -p callcraft-cli -- sim examples/chest-pain-001.scenario.yaml --script scripts/good-call.yaml --out report.json
cargo run -p callcraft-cli -- serve                                            # loopback UI + API
```

Build the desktop shell on Windows for the pilot:

```bash
cargo build -p callcraft-app
```

The shell starts the loopback server in-process and points its webview at it.
The same server powers `callcraft serve` in a browser.

## Runner UI

`callcraft serve` prints a loopback URL with a per-launch token. Open it and:

1. Load a scenario YAML (file picker) or click **Load example**.
2. **Start session** — the server creates an in-memory session from the file.
3. Type trainee turns; the deterministic caller answers only from the fact base.
4. Record rubric events with the chips generated from the scenario's event anchors.
5. **Finish & score** — get the checklist result, evidence, critical failures,
   drift count, and a downloadable JSON report.

The Tauri shell serves the same UI in a native window. No Node, no build step.

## Authoring UI

The **Author** tab edits the scenario as structured fields — metadata, safety,
persona, facts, rubric items, and anchors — against the same schema the runner
enforces. Validation runs on every edit (debounced); rendering is blocked while
the scenario is invalid. **Render & download** writes YAML, and **Use in
runner** loads the rendered scenario straight into a session. Unknown fields in
a loaded file are preserved.

Authoring round-trips through the core: YAML parses into the typed model, the
form edits that model, and the server validates then serializes it back.

## Voice

Voice is I/O around the same deterministic caller. Configure out-of-process
STT and TTS adapters; unset means text-only:

```bash
CALLCRAFT_STT_URL=http://127.0.0.1:8080 \
CALLCRAFT_TTS_URL=http://127.0.0.1:8081 \
CALLCRAFT_CALLER_URL=http://127.0.0.1:8082 \
cargo run -p callcraft-cli -- serve
```

Contracts, provider notes, and privacy rules:
[docs/VOICE-ADAPTERS.md](docs/VOICE-ADAPTERS.md). The LLM caller selects
authored fact ids only; the server renders the words:
[docs/LLM-CALLER.md](docs/LLM-CALLER.md).

## Windows spike

Before voice work, the desktop shell must be proven on the pilot's platform.
The build comes from the Windows CI job as `callcraft-windows`; the machine
checklist, diagnostics expectations, and failure modes are in
[docs/WINDOWS-SPIKE.md](docs/WINDOWS-SPIKE.md). The in-app **Diagnostics**
panel checks secure context, Tauri IPC, and the microphone prompt.

## Content policy — read this first

- **No agency content in this repo.** Scenarios for a real communications
  center, its SOPs, its geography packs, and anything derived from them stay in
  a private local pack. They are published only if the agency explicitly says
  so in writing.
- **No PII, no CJI.** Public examples use synthetic people, addresses, and
  phone numbers in the 555-01xx range. The validator warns on phone numbers
  outside that range; `--strict` makes it an error.
- **No real call audio or transcripts** enter the tool at any point in v0.1.
- The tool is a practice aid. It is not a certification and never implies that
  passing a scenario qualifies a telecommunicator.

## Format

See [SPEC-scenario-format.md](SPEC-scenario-format.md). The two load-bearing
rules:

1. **The caller may not invent.** The scenario declares a closed fact base;
   the caller may reveal, defer, or deny facts from it and nothing else.
2. **Score only what is observable.** Rubric items anchor to questions asked,
   addresses confirmed, instructions delivered. No tone or empathy scoring.

A scenario is YAML, schema-validated against
[schemas/scenario.v0.1.schema.json](schemas/scenario.v0.1.schema.json),
diffable in git, and licensed CC BY 4.0 by default (code is MIT; agency-local
packs may be proprietary and unpublished).

## Repo layout

```
crates/callcraft-core/   schema types, deterministic caller, scoring, reports (pure)
crates/callcraft-server/ loopback HTTP API + static UI hosting
crates/callcraft-cli/    `callcraft` binary: validate, demo, sim, serve
crates/callcraft-app/    Tauri desktop shell (Windows-first for the pilot)
web/               static authoring UI, no Node build
examples/          worked example scenario (synthetic)
scripts/           canned trainee scripts for demo and tests
fixtures/          valid/invalid scenario corpus for CI
schemas/           JSON Schema for the format
docs/              dependency policy and project docs
tests live beside callcraft-core, plus the pilot plan at the root
SPEC-scenario-format.md
PILOT-30-DAY.md    pilot plan and success criteria
```

## Status

v0.1 — text and voice turns, deterministic caller with optional LLM
decisioning, checklist scoring, JSON reports, byte-identical replay, session
API, runner UI, structured authoring UI, and a Tauri shell scaffold. Not built
yet: native file dialog, auth beyond the per-launch token. The 30-day pilot criteria in
[PILOT-30-DAY.md](PILOT-30-DAY.md) decide whether any of that gets built.

Dependency and release rules: [docs/DEPENDENCY-POLICY.md](docs/DEPENDENCY-POLICY.md).

## License

Code: MIT (see [LICENSE](LICENSE)). Scenarios in `examples/`: CC BY 4.0.
Agency-local packs: not published, not licensed.
