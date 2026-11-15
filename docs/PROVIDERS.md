# Providers and authentication

Callcraft keeps every model behind an out-of-process adapter. The core never
embeds an SDK or a provider runtime. This file covers the caller LLM registry;
speech providers follow the same pattern next.

## Registry

| Provider id | API shape | Auth | Subscription OAuth |
|---|---|---|---|
| `openai` | OpenAI chat completions | API key | Partner-gated (`Sign in with ChatGPT`) |
| `anthropic` | Anthropic Messages | API key | **Prohibited** for third-party apps |
| `openai-compatible` | OpenAI chat completions | Optional API key | Not applicable |
| `ollama` | OpenAI-compatible local | None | Not applicable |
| `llama-cpp` | OpenAI-compatible local | None | Not applicable |
| `lm-studio` | OpenAI-compatible local | None | Not applicable |
| `vllm` | OpenAI-compatible local | None | Not applicable |
| `openrouter` | OpenAI-compatible | API key | Not applicable |

Adding a provider means a module under `crates/callcraft-server/src/providers/`
plus a manifest entry. Presets that share an API shape share the module.

### Anthropic subscription OAuth

Anthropic does not offer a third-party OAuth program. Since early 2026 it has
blocked consumer-plan OAuth tokens in third-party tools and directs builders to
Console API keys. Callcraft deliberately offers an API key only for Anthropic.

### OpenAI subscription OAuth

OpenAI's `Sign in with ChatGPT` plan usage launched in September 2026 with a
limited partner set: an OAuth token against the Responses API, with per-app
caps. It is partner-gated. The registry marks it `partner-gated`; a Responses
API adapter and OAuth flow will be added only if Callcraft is accepted.

## Configuration

Precedence:

1. Environment variables (CLI, container, consortium deployment).
2. Stored non-secret config plus an API key from the OS keychain.
3. No caller (deterministic).

Environment variables:

| Variable | Meaning |
|---|---|
| `CALLCRAFT_CALLER_PROVIDER` | Provider id from the registry. Defaults to `openai-compatible` when only a URL is set. |
| `CALLCRAFT_CALLER_URL` | Base URL override. |
| `CALLCRAFT_CALLER_MODEL` | Model name (default `default`). |
| `CALLCRAFT_CALLER_API_KEY` | API key for environment-based deployments. |

Stored configuration:

- Non-secret settings: `${XDG_CONFIG_HOME:-~/.config}/callcraft/adapters.json`
  on Linux, `%APPDATA%\callcraft\adapters.json` on Windows,
  `~/Library/Application Support/callcraft/adapters.json` on macOS.
  `CALLCRAFT_CONFIG_DIR` overrides the root. The file is written atomically with
  mode `0600` on Unix and contains no secret.
- API keys: OS keychain (Secret Service, Windows Credential Manager, macOS
  Keychain), service `callcraft`, account `provider:<id>`.
  `CALLCRAFT_DISABLE_KEYRING=1` disables keychain access for headless runs.

The Settings tab in the UI lists the registry, tests a candidate config, and
saves it. A key is sent only to the loopback API and then to the OS keychain; it
is never written to the config file or a report.

## API

- `GET /api/providers` — manifests plus the active config (`api_key_set`, never
  the key), and whether the keychain is enabled.
- `POST /api/providers/test` — runs one caller decision against the bundled
  example scenario, validates and renders the decision, and reports model,
  latency, chosen fact ids, style, and rendered caller text. It does not save.
- `POST /api/providers/configure` — validates, stores the non-secret config and
  key (if supplied), and switches the live caller. A blank key keeps an existing
  stored key.
- `POST /api/providers/reset` — switches to the deterministic caller and clears
  the stored config. Provider keys remain in the keychain.

## Security

- Local providers are the public-safety default; cloud endpoints are explicit
  opt-in.
- No telemetry.
- Keys are never returned by the API, logged, or written to reports.
- Cloud providers receive the persona, fact list, transcript, and latest trainee
  line. Agency policy decides whether that data path is allowed.
- The test endpoint uses the embedded, synthetic example scenario only.

## OAuth roadmap

- OpenAI `Sign in with ChatGPT`: partner-gated; requires an official OAuth
  client and the Responses API adapter.
- Anthropic consumer subscription OAuth: not available to third-party apps;
  API keys only.
- Enterprise identity flows (Azure OpenAI with Entra ID, Vertex AI with Google
  ADC, Bedrock with AWS SSO) are legitimate and will be separate provider
  modules, not a generic subscription hack.

Sources: [Anthropic third-party OAuth ban](https://winbuzzer.com/2026/02/19/anthropic-bans-claude-subscription-oauth-in-third-party-apps-xcxwbn/) ·
[Anthropic enforcement reporting](https://gigazine.net/gsc_news/en/20260220-anthropic-third-party-block) ·
[Anthropic CLI auth docs](https://platform.claude.com/docs/en/cli-sdks-libraries/cli/authentication.md) ·
[Sign in with ChatGPT](https://thenewstack.io/sign-in-with-chatgpt/) ·
[WorkOS on plan-usage scope](https://workos.com/blog/sign-in-with-chatgpt-plan-usage-scope)
