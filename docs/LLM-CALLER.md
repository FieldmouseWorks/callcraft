# LLM caller adapter

The caller can run in one of two modes:

- **Deterministic** (default): the fact-base engine maps trainee trigger words to
  authored facts.
- **LLM decisioning**: an OpenAI-compatible chat model decides *which authored
  fact ids* answer the trainee's line and what tone to use. The server renders
  the caller's words from the fact values.

The model never writes facts. It cannot invent an address, phone number,
medication, or event because it never produces free text that reaches the
session. It returns a decision; the server validates the ids against the
scenario and composes the utterance.

## Why decision-only

A free-text LLM caller is the obvious feature and the wrong first design. There
is no reliable way to validate that generated dialogue contains no invented
facts, and an invented detail in a public-safety training scenario destroys
trust the first time a training officer notices it. Fact selection keeps the
closed-world guarantee structural: unknown fact ids are rejected, premature
`after_event` facts are rejected, and every content word in the reply comes from
the authored file.

The cost is templated phrasing. The model adds intent mapping, not prose. A
validated paraphrase layer can be added later behind the same decision contract.

## Contract

`POST {CALLCRAFT_CALLER_URL}/v1/chat/completions`

Request is the provider's chat/messages shape with a system prompt containing
the persona, timeline state, closed fact list, disclosure rules, recorded
events, and recent transcript; the trainee's latest line is the user message.
No sampling parameters are sent, because current provider APIs disagree on
which ones they accept.

The response content must be JSON:

```json
{"fact_ids": ["address"], "style": "anxious"}
```

- `fact_ids`: zero to two authored fact ids.
- `style`: `neutral`, `anxious`, `distressed`, or `repeat`; unknown values fall
  back to `neutral`.

The server responds with `caller_source: "llm"` and the rendered text.

## Validation and fallback

- A rejected decision increments `drift_rejections` and falls back to the
  deterministic engine. Rejections include: unknown fact id, more than two ids,
  an `after_event` fact before its event, a `never_unless` fact without a
  matching trigger in the trainee's latest line, a `volunteer` fact after the
  opening turn, or `repeat` for a fact not yet disclosed.
- `repeat` is derived from the disclosed set: once every selected fact has been
  disclosed, the rendered line is a repeat even if the model returned another
  style. Selecting `repeat` for an undisclosed fact is rejected.
- Style is clamped to the authored timeline state. A model cannot make the
  caller more distressed than the scenario says it is at that moment.
- Malformed JSON, a response with no choices/content, or a provider HTTP error
  increments `adapter_failures` and falls back. Transport failures also count
  toward a per-session breaker: after three, the caller runs deterministically
  for the rest of that session. Reconfiguring the provider resets the breaker.
- A rejected decision is a defect in the adapter, not in the trainee's score.
  Scoring still runs from the transcript and rubric.

## Configuration

The caller runs through the provider registry. The Settings tab is the normal
path; environment variables are for CLI and consortium deployments:

| Variable | Meaning |
|---|---|
| `CALLCRAFT_CALLER_PROVIDER` | Provider id (`openai`, `anthropic`, `openai-compatible`, `ollama`, `llama-cpp`, `lm-studio`, `vllm`, `openrouter`). |
| `CALLCRAFT_CALLER_URL` | Base URL override. With no provider id, implies `openai-compatible`. |
| `CALLCRAFT_CALLER_MODEL` | Model name. Required; no invented default. |
| `CALLCRAFT_CALLER_API_KEY` | API key for environment-based deployments. |
| `CALLCRAFT_CALLER_TIMEOUT_MS` | Per-decision budget; default 5000 ms. |

Stored settings live in `callcraft/adapters.json`; keys live in the OS keychain.
A stored key is bound to its configured base URL and is not sent to a different
request-supplied origin. Non-loopback endpoints must use HTTPS.
Anthropic consumer subscription OAuth is not available to third-party apps;
OpenAI Sign in with ChatGPT is partner-gated. See
[PROVIDERS.md](PROVIDERS.md).

`GET /api/voice/status` reports `caller: "llm" | "deterministic"`.

## Privacy and security

- Only text is sent to the caller model: the persona, fact list, transcript, and
  the trainee's latest line. No audio, no CJI, no agency SOPs unless the agency
  publishes them into a scenario.
- No default cloud provider and no telemetry. Agency policy decides whether a
  cloud model is allowed; a local model behind an OpenAI-compatible server is the
  expected public-safety default.
- The trainee's text is treated as in-scene dialogue, never as instructions; the
  system prompt says so explicitly. Even a successful prompt injection cannot
  add facts because the model only returns ids.

## Known limits

- Templated replies; the caller does not paraphrase.
- At most two facts per turn; multiple values are joined with a period.
- The model's fact selection is trusted for intent; the server enforces the
  closed fact base and `after_event` gating, not trigger-word matching.
- No streaming and no multi-turn tool use.
- Latency and cost still need the pilot measurements.
