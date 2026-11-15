# Speech providers

STT and TTS are separate registries. A provider that transcribes rarely
synthesizes with the same auth and API shape, and pretending otherwise makes a
worse settings UI.

## Registry

### Speech-to-text

| Provider id | API shape | Auth | Default |
|---|---|---|---|
| `openai-compatible` | OpenAI transcription | Optional key | local or gateway |
| `openai` | OpenAI transcription | API key | `whisper-1` |
| `whisper-cpp` | OpenAI-compatible local | None | `127.0.0.1:8080` |
| `elevenlabs` | ElevenLabs Scribe | API key | `scribe_v1` |
| `deepgram` | Deepgram listen | API key | `nova-3` |

### Text-to-speech

| Provider id | API shape | Auth | Default |
|---|---|---|---|
| `openai-compatible` | OpenAI speech | Optional key | local or gateway |
| `openai` | OpenAI speech | API key | `tts-1`, voice `alloy` |
| `elevenlabs` | ElevenLabs text-to-speech | API key + voice id | `eleven_multilingual_v2` |
| `piper` | OpenAI-compatible local shim | None | set the shim URL |

Adding a provider is a module under `crates/callcraft-server/src/speech/` plus a
manifest entry. Providers that share an API shape share a module.

## Configuration

Environment variables (CLI/consortium deployments) win over stored settings:

| Variable | Meaning |
|---|---|
| `CALLCRAFT_STT_PROVIDER`, `CALLCRAFT_TTS_PROVIDER` | Provider id. |
| `CALLCRAFT_STT_URL`, `CALLCRAFT_TTS_URL` | Base URL override. A URL alone implies `openai-compatible`. |
| `CALLCRAFT_STT_MODEL`, `CALLCRAFT_TTS_MODEL` | Model name. |
| `CALLCRAFT_STT_LANGUAGE` | Language hint (default `en`). |
| `CALLCRAFT_TTS_VOICE` | Voice name or id. |
| `CALLCRAFT_STT_API_KEY`, `CALLCRAFT_TTS_API_KEY` | API key. `CALLCRAFT_ADAPTER_TOKEN` remains a fallback. |
| `CALLCRAFT_STT_TIMEOUT_MS`, `CALLCRAFT_TTS_TIMEOUT_MS` | Per-stage request budget; default 8000 ms. |
| `CALLCRAFT_ALLOW_INSECURE_ADAPTERS` | Set to `1` only to allow plaintext HTTP to a non-local endpoint. |

Stored settings live in `callcraft/speech.json` next to the caller config. API
keys live in the OS keychain under `stt:<provider>` and `tts:<provider>`; the
key file never contains a secret. A stored key is bound to its configured base
URL and is not sent to a different request-supplied origin. Non-loopback
endpoints must use HTTPS. Local-only presets reject a remote base URL; use
`openai-compatible` for an arbitrary remote endpoint.

STT and TTS APIs are separate types (`SttApi`, `TtsApi`), so a provider that only
transcribes cannot be selected for synthesis. Deepgram is STT-only.

## API

- `GET /api/speech/providers` — both registries plus active config.
- `POST /api/speech/test` — `{kind: "stt"|"tts", provider_id, base_url, model,
  language?, voice?, api_key?}`.
  - STT tests with the bundled synthetic WebM/Opus clip in
    `assets/stt-test.webm` — the same container a browser MediaRecorder sends.
  - TTS synthesizes "Callcraft voice connection test." and returns the audio
    bytes as base64 so the UI can play it.
  - Neither endpoint stores audio.
- `POST /api/speech/configure` — validates, stores the config and key, and
  switches the live direction.
- `POST /api/speech/reset` — `{kind}` clears that direction.

## Preset coverage

The STT and TTS registries are contract-tested for unique ids, metadata, and
local-only presets; the OpenAI-compatible, ElevenLabs, and Deepgram request
shapes have adapter tests against mock servers. Cloud presets are not tested
against live vendor accounts in CI. Run `POST /api/speech/test` with the
agency's own account before a class.

## Tone and steering: there is no universal standard

This is mostly the wild west above the markup layer.

- **SSML** is the W3C standard for prosody and markup. Azure supports it broadly
  (including `mstts:express-as` styles such as cheerful, sad, angry), Google
  supports a subset, Polly supports a subset plus a whisper effect. OpenAI's TTS
  API does not consume SSML.
- **OpenAI** steers affect through the `instructions` parameter on
  `gpt-4o-mini-tts` (natural language such as "speak in a calm, reassuring
  tone"), plus a separate speed control and a fixed voice choice.
- **ElevenLabs** steers through stability/style voice settings and bracketed
  audio tags such as `[sad]`, `[whispers]`, or `[shouts]` in the script. Exact
  values are creative controls, not a specification, and model generations
  change behavior.
- **Deepgram, Piper, and local runners** vary further; some accept only voice
  and model choice.

So there is no portable "set emotion to anxious" contract. Callcraft's v1 sends
plain text plus voice/model and lets each provider do its default. The planned
layer is a provider-neutral style hint (`calm`, `anxious`, `distressed`,
`urgent`, rate/pitch hints), mapped per adapter with a capability flag, and never
treated as guaranteed. Never send SSML tags to a provider that speaks them
literally. For training, score observable dispatcher behavior, not the emotion
of the synthesized voice.

## Security

- Local providers are the default assumption; cloud speech is explicit opt-in.
- Audio is forwarded to the configured endpoint and never stored or logged.
- Keys are keychain-only and never returned by the API.
- The test audio is synthetic and committed with the repository.

## Known limits

- One utterance per turn; no streaming and no barge-in.
- No diarization, no confidence reporting, no emotion scoring.
- Style/affect mapping is not implemented yet; see the tone section.

Sources: [OpenAI speech steering](https://developers.openai.com/api/docs/guides/text-to-speech.md) ·
[ElevenLabs audio tags](https://runware.ai/docs/models/elevenlabs-v3/guides/directing-with-audio-tags) ·
[ElevenLabs settings](https://neuraplus-ai.github.io/blog/best-settings-for-elevenlabs-ai-voice-quality-improvement-2026) ·
[SSML across providers](https://typecast.ai/learn/best-api-ssml-support/) ·
[Azure SSML](https://docs.azure.cn/en-us/ai-services/speech-service/speech-synthesis-markup-voice)
