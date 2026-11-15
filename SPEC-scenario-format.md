# Callcraft — Scenario & Rubric Format v0.1

Status: draft for design-partner review. The goal is simple: an agency's training
content is portable, diffable, and owned by the agency. The code is MIT; the format
is the product.

## 1. Design principles

1. **The caller may not invent.** A scenario declares a closed fact base. The AI
   caller may reveal, defer, or deny facts from that base, and nothing else. A
   runtime that cannot enforce this is non-conforming. This is the anti-hallucination
   architecture, not a feature.
2. **Score only what can be observed.** Rubric items are checks with anchors in
   the transcript: questions asked, address confirmed, instructions delivered.
   No tone, empathy, or sentiment scoring in v0.1. Human review is always allowed
   and always recorded.
3. **Files are the truth.** Scenarios and rubrics are YAML, schema-validated,
   stable-ID'd, and readable in a git diff. Anything an authoring UI produces is
   these files.
4. **Content licensing is separate from code licensing.** Code is MIT. Commons
   scenarios default to CC BY 4.0. Agency-local packs may be marked proprietary
   and stay unpublished.
5. **No PII, no CJI.** Commons content uses synthetic people, addresses, and
   numbers. The authoring tool warns on values that look like real phone numbers
   or resident addresses. Real geography (public roads, landmarks) is allowed;
   resident data never is.
6. **Text first, voice second.** Text mode is a first-class drill mode; voice
   mode is for assessment. The same scenario file runs both.

## 2. File anatomy

```yaml
format: callcraft-scenario/v0.1   # required
id: chest-pain-001                    # required, stable, globally unique
title: Chest Pain — Adult             # required
version: 1.0.0                        # required, semver of this content
license: CC-BY-4.0                    # required; or proprietary
language: en-US                       # required
incident_type: medical                # medical | fire | police | other
difficulty: basic                     # basic | intermediate | advanced
duration_minutes: 6
authors: ["Name <handle>"]
tags: [medical, chest-pain, als]

persona:
  display_name: Maria
  age: 58
  voice:
    sex: female
    accent: neutral
    pace: normal
  emotional_state: anxious
  environment:
    noise: [dog, television]
    signal: cellular
  impairments: []                     # e.g. hearing loss, language barrier

facts: [...]                          # closed fact base, see §3
timeline: [...]                       # caller-state changes over time
rubric: [...]                         # observable checks, see §4
geography:
  pack: synthetic                     # or an agency-local pack id
  references: []
safety:
  disclaimer: "Training simulation. Not a certification."
  content_warnings: [medical emergency]
xapi:
  enabled: true
```

### Required fields

`format`, `id`, `title`, `version`, `license`, `language`, `persona`,
`facts`, `rubric`, `safety.disclaimer`.

### Reserved for later versions

`multi_caller`, `radio_traffic`, `escalation_events`, `localization_overrides`.
Unknown fields are preserved on read/write so newer files don't lose data in
older tools.

## 3. The fact base and disclosure

Each fact the caller knows is declared:

```yaml
facts:
  - id: chief_complaint
    value: "Chest pain, started about 20 minutes ago"
    disclosure: on_question          # volunteer | on_question | if_asked | never_unless | after_event
    triggers: [pain, hurt, chest]   # question intents that unlock it
    verifiable: false

  - id: address
    value: "1234 Firwood Lane, Cedar Hollow"
    disclosure: on_question
    triggers: [where, address, location]
    verifiable: true                # rubric can anchor to confirm/repeat

  - id: callback
    value: "555-0142"
    disclosure: on_question
    triggers: [number, call back, phone]
    verifiable: true
```

**Runtime contract.** The caller's system prompt is built from this fact base.
Every caller utterance is validated against it: if the model produces a fact not
declared here (an address, a name, a medication, a time), the runtime rejects and
regenerates; repeated failures end the session with a `drift` flag. The report
records every rejection. This is the number that has to be zero in the pilot.

**Timeline.** Caller state may change over time (more distressed, harder to
understand, goes silent). Timeline events are declarative and auditable; they
never add facts.

## 4. Rubric

```yaml
rubric:
  scoring: checklist                  # v0.1: checklist only
  critical_must_pass: true
  human_review_required: true
  items:
    - id: address_verified
      description: "Confirms exact address and nearest cross street"
      critical: true
      anchors:
        any_of:
          - transcript_regex: '(?i)(confirm|verify|read back).{0,60}(address|location)'
          - utterance_pair: [caller_address, trainee_address_repeat]
      evidence_required: true

    - id: pre_arrival_instructions
      description: "Delivers the agency's pre-arrival instructions for chest pain"
      critical: true
      anchors:
        any_of:
          - event: protocol_card_delivered
      evidence_required: true
      human_review: required          # protocol content is agency-owned
```

**Results are three-valued:** `pass`, `fail`, `needs_human`. Machine results
carry evidence (transcript utterance ids and timestamps). A human can override
any item; overrides are stored in the report. Agreement between machine results
and blind human grading is computed per session and across the pilot.

Anchors are deliberately dumb: regex, utterance pairs, and explicit events.
The model does not get to decide whether the trainee was good; it only produces
the transcript the anchors run over.

## 5. Geography packs

A pack declares where the scenario lives. `synthetic` packs use invented
addresses. Agency-local packs reference real public geography and are marked
`license: proprietary` by default. Packs are referenced, never embedded, so a
scenario can be shared without leaking a jurisdiction's map data.

## 6. Evaluation report

```json
{
  "scenario_id": "chest-pain-001",
  "scenario_version": "1.0.0",
  "scenario_sha256": "…",
  "pack_id": "local",
  "pack_version": "0.1.0",
  "approval": "approved",
  "session_id": "uuid",
  "mode": "voice",
  "started_at": "2026-11-15T18:00:00Z",
  "transcript": [
    {"id": "u1", "speaker": "caller", "text": "...", "t_ms": 0},
    {"id": "u2", "speaker": "trainee", "text": "...", "t_ms": 2400}
  ],
  "evaluation": {
    "items": [
      {"id": "address_verified", "result": "pass", "evidence": ["u2", "u7"],
       "machine": true, "overridden_by": null}
    ],
    "critical_failures": [],
    "human_overrides": [],
    "drift_rejections": 0,
    "adapter_failures": 0
  },
  "stages": [
    {"stage": "stt", "ms": 340},
    {"stage": "turn", "ms": 820},
    {"stage": "tts", "ms": 460}
  ],
  "cost": {"stt_usd": 0.04, "llm_usd": 0.11, "tts_usd": 0.22, "total_usd": 0.37},
  "latency_ms": {"p50_ms": 820, "p95_ms": 1400}
}
```

The report is the export format. xAPI emission (optional) wraps it as a
statement; the runner never owns a dashboard.

## 7. Versioning and compatibility

- The `format` field carries a version. Breaking changes bump the major part.
- Runners declare a supported range and refuse files outside it with a clear
  error.
- Read/write preserves unknown fields (forward compatibility).
- Scenario content has its own semver, independent of the format.

## 8. Deliberate exclusions in v0.1

Live-call integration. Recorder import. Any CJI handling. ASSIST-style live
guidance. Protocol text (MPDS, ProQA, APCO) — agencies supply their own
checklists. CE accreditation. Tone or empathy scoring. Languages beyond the
configured pack, done well.

## 9. Conformance

A **conforming runner**: validates files against the schema, enforces the fact
base, emits the report, and measures cost, latency, and drift. A **conforming
authoring tool**: produces schema-valid files, lints them, and previews a run.
Conformance is testable; marketing claims are not.
