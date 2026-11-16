# Client design: Logbook

The client is a **ruled record with a margin**. A scenario is a brief, a live
session is a log being written, and a debrief is the same record annotated
against the rubric. The record leads; judgment sits beside it.

The motif comes from the Fable design review on 2026-11-15. It replaces the
generic dark developer-tool shell and the amber-on-black console look that
preceded it.

## Principles

- Paper-light, flat, typographic. The top bar is the only ink-dark surface.
- Score only what is observable, so the transcript is the primary object, not
  a chat bubble stream.
- Run hides the answer key. The scenario editor and YAML facts are gone while a
  call is live.
- Red is rationed. `fail` is a word plus a mark; `needs_human` is a first-class
  outcome; the only filled red is the recording control.
- A session is a workspace, not another tab: Prepare, Run, and Review are stages
  of one call.
- No CAD imitation, no gamification, no countdown pressure, no telemetry.

## Screens

| Screen | Primary | Secondary |
|---|---|---|
| Prepare | parsed scenario brief, Start session | source editor, example, readiness, voice state |
| Run | transcript log, composer | observer marks, finish, recording |
| Review | result line and rubric vs. evidence | download, back to brief |
| Author | structured form, validation | render, use in runner |
| Settings | caller / STT / TTS cards | diagnostics |

The top bar carries the wordmark, Prepare / Author / Settings, and the status
light. Author and Settings are utility pages. The live bar carries the scenario
title, stage label, progress status, and the PRACTICE tag.

## Visual tokens

Light is the default; `prefers-color-scheme: dark` swaps the tokens for a dim
comm-floor variant.

| Token | Light | Dim | Role |
|---|---|---|---|
| `--paper` | `#f4f1ea` | `#171b1e` | page |
| `--card` | `#fbfaf6` | `#1f2529` | surfaces |
| `--sunk` | `#eae6dc` | `#121518` | inputs, code |
| `--ink` | `#1b2328` | `#e4e1d8` | text |
| `--muted` | `#566169` | `#a3aba8` | secondary |
| `--line` | `#d6d0c2` | `#2e363b` | rules |
| `--edge` | `#6f7975` | `#748079` | control borders |
| `--accent` | `#0f5c63` | `#6fc3c9` | primary, focus, trainee |
| `--pass` | `#22643a` | `#7cc492` | pass |
| `--fail` | `#9c2b23` | `#f0958a` | fail, recording |
| `--review` | `#7a4e08` | `#e0b565` | needs human |
| `--bar` | `#1b2328` | `#0e1113` | top bar |

Type: Segoe UI Variable Text / system sans for UI, Cascadia Mono / Consolas /
ui-monospace for metadata, IDs, times, and the transcript gutter. Uppercase
tracking only on speaker and field labels. Radii are 4px for controls and 6px
for cards; log rows have square corners and a single rule between them.

## Transcript and review

The transcript is an ordered list. Each row is time gutter, speaker, text. A
trainee row gets a 3px accent rule; a system note is muted and unboxed. There
are no bubbles.

The report renders the rubric beside evidence tokens. Utterance evidence tokens
scroll to and highlight the matching log row; `event:` evidence has no row and
stays a tag. The summary is a ruled strip, not a dashboard: overall, checklist
count, failed count, human-review count.

## Accessibility

- Every control has a visible `:focus-visible` outline.
- File inputs are visually hidden but keyboard reachable.
- Tabs expose `aria-selected` and panel roles.
- Status regions use `aria-live`.
- Contrast was chosen against the light and dim tokens; re-measure if the
  palette changes.
- Motion is limited to a 120ms color/opacity transition; the recording pulse is
  disabled under `prefers-reduced-motion`.

## Implementation

The client remains static HTML/CSS/vanilla JS with no build step. `index.html`
is the shell; `styles.css` holds tokens, base, layout, components, modes, and
print rules. `app.js` owns Prepare/Run/Review and the log; `author.js` owns the
structured form; `settings.js` owns provider cards.

High-impact slice shipped: tokens and type, focus and file-input repair, the
mode shell, Prepare brief fed by `/api/scenario/parse`, Run log and composer,
observer marks, Debrief with evidence links, responsive layouts at 1024 and 720,
and the dim token set.

Not yet built from the review: one-section-at-a-time authoring rail, Open
report on Prepare, a persisted theme toggle, event-evidence anchors with
timestamps, and a two-window observer split. Those need server or state work
and are tracked separately rather than faked in the client.

## Non-goals

- Faux CAD chrome, maps, ANI/ALI strips, or status boards.
- Scores styled as grades, certificates, or streaks.
- A session-history UI with no persistence behind it.
