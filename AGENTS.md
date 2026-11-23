# Callcraft

Callcraft is an open scenario and rubric format for 911 dispatcher training,
and its reference implementation: a validator, a deterministic text and voice
runner, a checklist scorer, a loopback server with a web UI, and a Tauri
desktop shell. [README.md](README.md) says what it does and how to run it.

<!-- house:rules v1 begin. Copied from FieldmouseWorks/burrow house/rules.md, verbatim. Change it there, not here: burrow's house sync rewrites everything between these markers. -->
## House rules

These hold in every repo. This file's own sections add to them and win where
they're more specific.

- Design the class, not the instance. When a bug, finding or stall shares a
  cause with others, fix the cause once, where it lives (one owning path, a
  type, a check), so none of its cases come back.
- Do work once. Look for what's already open or built first, extend the
  shared crate, client or job that does it rather than write another copy,
  and never run work twice that once would do.
- Fix the whole cause: no workarounds, shims, fallbacks or half-migrations.
  When you replace something, delete the old path in the same change.
- One fact, one home. Each rule or fact lives in one place; link to it
  instead of restating it.
- Pipeline options live in the settings source: change them through its
  door, read them with `proj settings show`, and link the
  [settings reference](https://github.com/FieldmouseWorks/burrow/blob/main/docs/settings-reference.md)
  rather than restate a value or where one is kept.
- Before you edit, say what done looks like and how it will be checked.
- Keep your context small: every step re-sends all you've read, and a
  long wait makes it pay again. Read the lines you need (`rg -n`, ranges,
  a diff's stat then its hunks), not whole files or logs; cap command
  output; hand broad searches to a subagent; don't re-read what you have.
- Report only the checks you ran and what they said. Skipped, unavailable
  or "should pass" is never a pass. Keep local and CI results apart.
- A new check comes with proof that it fails on the bug it's for.
- The code is the memory. When something goes wrong, make the code, a type,
  a test or a check stop it happening again; a new rule here is the last
  resort. Say in the PR which one you did.
- Keep going on work you were asked to do: don't ask again for what's
  already been asked for.
- Never commit what the repo keeps private: secrets, tokens, personal data,
  and anything under this file's "Private" section.
- `Closes #N` only when the issue is fully done; otherwise `Refs #N`.

CI's house check holds every PR to its title's style and to the rules below,
each of which a PR can pass with a line in its body starting as shown and a
reason of four words or more. Editing the body or title runs it again.
No line passes one more: a PR's title and body never write the review
connector's at-sign mention, even in backticks, as the connector then reads
the PR as a task and doesn't review it. Write "the connector's bare review
mention" in its place.

- Rust by default, shell included: new code meant to last starts in Rust
  unless its platform picks the language, and a coin toss goes to Rust. A PR
  that adds Python, or grows Python and shell past a fix, says `Why not
  Rust:` (a file a tool generates, glue that only runs another program, a
  throwaway script, a tool still being tuned). Port existing Python and
  shell whenever Rust buys anything, system and pipeline code first.
- Work starts from an event (a push, a webhook, a unit finishing), not a
  clock. A PR that adds a timer or a cron schedule says `Why a timer:`.
- Split a code file before it passes 1,000 lines, or say `Why not split:`.
- AGENTS.md is a map: what the repo is, its own rules, and where things live
  (a feature map, the doc that owns each subject). Detail goes in the doc
  that owns it, linked from here. A PR that grows the repo's own part (the
  lines outside these blocks) past 100 lines says `Why a longer guide:`.
- Tracked files never name a model: which model does what is the machine's
  business, and names go stale in weeks. A line that has to (a recorded
  screen in a test) says `Why a model name:`.

### Commits

Subjects are `area: plain sentence`, the area being the module, directory or
tool the change is about (`autoland: merge only the head that was checked`):
lowercase after the colon, no trailing period, no `feat:`/`fix:`. The body,
if any, says why. A PR's title is the subject main gets, so it's written the
same way. Keep the git identity your machine gives you: a commit signed with
a key that isn't its author's shows as Unverified, and CI fails it.
<!-- house:rules end -->

## Layout

| Path | What |
|---|---|
| `crates/callcraft-core/` | schema types, deterministic caller, scoring, reports (pure) |
| `crates/callcraft-server/` | loopback HTTP API, static UI hosting, providers, packs |
| `crates/callcraft-cli/` | the `callcraft` binary: validate, demo, sim, serve, pack |
| `crates/callcraft-app/` | Tauri desktop shell (Windows first for the pilot) |
| `web/` | the static UI, no Node build |
| `schemas/`, `SPEC-scenario-format.md` | the scenario format |
| `examples/`, `scripts/`, `fixtures/` | synthetic scenario, trainee scripts, valid/invalid corpus |

Where each subject lives: direction, status and decisions in
[docs/ROADMAP.md](docs/ROADMAP.md); dependencies and CI gates in
[docs/DEPENDENCY-POLICY.md](docs/DEPENDENCY-POLICY.md); the client in
[docs/DESIGN.md](docs/DESIGN.md); packs in [docs/PACKS.md](docs/PACKS.md);
providers and voice in [docs/PROVIDERS.md](docs/PROVIDERS.md),
[docs/SPEECH-PROVIDERS.md](docs/SPEECH-PROVIDERS.md),
[docs/VOICE-ADAPTERS.md](docs/VOICE-ADAPTERS.md) and
[docs/LLM-CALLER.md](docs/LLM-CALLER.md); CodeQL test-alert dismissals in
[docs/CODEQL.md](docs/CODEQL.md).

## Build and check

- `nix develop` gives the toolchain `rust-toolchain.toml` pins, with the GTK
  and WebKitGTK libraries the desktop shell links on Linux.
- `nix flake check` is the gate CI runs: rustfmt, clippy on every target of
  the whole workspace with warnings as errors, the workspace's tests, and a
  secrets scan.
- `ci.yml` adds what the flake can't hold: `cargo audit` and `cargo deny
  check` (they fetch advisories), and the Windows build of the desktop shell.

## Project rules

- Every slice PR updates [docs/ROADMAP.md](docs/ROADMAP.md).
- The README's content policy holds: no agency content, no PII or CJI, no
  real call audio or transcripts. Public examples use synthetic people and
  555-01xx phone numbers.

## Private

Agency packs and anything derived from an agency's scenarios, SOPs or
geography stay local and never enter the repo, as do provider API keys.

<!-- house:prs v1 begin. Copied from FieldmouseWorks/burrow house/prs.md, verbatim. Change it there, not here: burrow's house sync rewrites everything between these markers. -->
## Pull requests

One task, one PR: finish what the task set out to do, then open one PR, so
it goes through review and CI once. No PR per step or round, and no stacks.
A fix to the pipeline (the steward, CI, the machine) lands as soon as it's
ready, and so does a change to these rules that the work needs, in a PR of
its own: reviewers read the default branch's rules, not the PR's.

Open PRs ready for review, never as drafts, and leave them to the steward:
once the review bot finds nothing on your head, every thread is resolved and
CI is green, it merges. Don't merge it yourself. Answer every finding: push
the fix and reply with the commit, or reply with why it's wrong, then
resolve the thread. A PR touching a one-way path (secrets, CODEOWNERS) gets
a judgment first, and lands unless it can't be undone or is the owner's
call.

A red default branch is the steward's to fix, from a branch named
`autoland/red-` and the commit. Don't fix it yourself, in any repo, unless
the pipeline's stuck issue says the steward's jobs gave up; then fix it from
a branch of that name.
<!-- house:prs end -->

## Code Review Rules

For reviewers, the review bot included. Flag as P1:

- a change that makes a check pass without fixing the thing it checks
- tests or checks weakened, skipped or deleted to get green
- secrets, tokens or personal data in the diff or the logs it adds
- behavior changed beyond what the PR says it does
- agency content, real PII or CJI, real call audio or transcripts, or a
  public example phone number outside 555-01xx
- the caller saying anything outside the scenario's closed fact base, or a
  rubric item scored on something not observable in the transcript or events
- the server listening beyond loopback, an API route without the per-launch
  token, or an API key sent anywhere but the OS keychain and its provider
- a non-loopback provider endpoint reached over plain HTTP without the
  explicit opt-in
- telemetry, or agency data uploaded or stored outside the local machine
