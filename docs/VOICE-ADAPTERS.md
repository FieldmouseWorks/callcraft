# Voice adapters

Voice is I/O around the same deterministic caller used in text mode. The core
never embeds a speech or model runtime; STT and TTS are out-of-process HTTP
services behind a narrow contract. That keeps the MIT core clean, keeps local
and cloud providers swappable, and keeps provider credentials out of the
repository.

## Scope in this version

- One trainee utterance per turn: browser records, server transcribes, the
  deterministic caller answers, server optionally synthesizes the reply.
- No streaming, no barge-in, no live-call audio.
- No LLM caller yet. The caller is still the deterministic fact-base engine.
- Audio is forwarded, never written to disk, and never stored in the report.

## Contracts

### Speech-to-text

OpenAI-compatible endpoint: `POST {CALLCRAFT_STT_URL}/v1/audio/transcriptions`

- `multipart/form-data`
- `file`: the recorded audio (WebM/Opus from the browser by default)
- `model`: `CALLCRAFT_STT_MODEL` (default `whisper-1`)
- `language`: `CALLCRAFT_STT_LANGUAGE` (default `en`)
- Response: JSON with a `text` field

Any server that implements that contract works. For local use, point it at a
whisper.cpp server build that exposes the OpenAI-compatible route, or at a
small shim. Do not send agency audio to a cloud provider unless the agency has
approved that data path.

### Text-to-speech

OpenAI-compatible endpoint: `POST {CALLCRAFT_TTS_URL}/v1/audio/speech`

- JSON body: `model`, `input`, `voice`, `response_format: "wav"`
- Response: audio bytes with an `audio/*` content type

Piper is GPL-3.0. Run it as a separate process and connect through a shim or an
OpenAI-compatible wrapper. The process boundary is deliberate; do not link
Piper into the core.

## Configuration

| Variable | Meaning |
|---|---|
| `CALLCRAFT_STT_URL` | Base URL of the STT service. Unset disables voice input. |
| `CALLCRAFT_STT_MODEL` | Model name sent to STT (default `whisper-1`). |
| `CALLCRAFT_STT_LANGUAGE` | Language hint (default `en`). |
| `CALLCRAFT_TTS_URL` | Base URL of the TTS service. Unset disables spoken replies. |
| `CALLCRAFT_TTS_MODEL` | Model name sent to TTS (default `tts-1`). |
| `CALLCRAFT_TTS_VOICE` | Voice name (default `default`). |
| `CALLCRAFT_ADAPTER_TOKEN` | Optional bearer token for both adapters. |

Start the server with the variables set:

```bash
CALLCRAFT_STT_URL=http://127.0.0.1:8080 \
CALLCRAFT_TTS_URL=http://127.0.0.1:8081 \
cargo run -p callcraft-cli -- serve
```

If STT is unset, `/api/voice/status` reports `stt: false` and the UI stays
text-only. If TTS is unset or fails, the turn still scores; the UI shows the
caller text and reports that audio was unavailable.

## API

- `GET /api/voice/status` — `{ok, stt, tts}` booleans.
- `POST /api/sessions/{id}/voice-turn` — body is raw audio with an `audio/*`
  content type. Returns `{ok, transcript, caller, audio_base64?, audio_mime?,
  audio_error?}`.

Both are token-gated and loopback-only like the rest of the API.

## Security and privacy

- Audio goes only to the endpoints configured above. There is no default cloud
  provider and no telemetry.
- Audio is not stored by Callcraft. Transcripts are part of the session report,
  just like text mode.
- The desktop shell allows microphone capture in the Tauri permission handler;
  capture starts only after an explicit Record click. The browser build asks
  the browser's own permission.
- Treat trainee recordings as sensitive. Agency policy decides whether cloud
  STT/TTS is allowed at all; local sidecars are the default assumption for
  public-safety deployments.
- The adapter boundary exists partly for licensing (Piper) and partly so a
  provider compromise or outage cannot take the core down.

## Pilot measurements still required

- Round-trip latency, p50 and p95. Target: under 1.5 seconds.
- Per-session cost for STT and TTS. Target: under $1 per session.
- Transcript accuracy against the training officer's blind grading, especially
  addresses and callback numbers.
