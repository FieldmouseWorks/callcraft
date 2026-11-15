//! Speech provider registry and configuration.
//!
//! STT and TTS are separate registries because providers rarely offer both with
//! the same auth and API shape. Local runners stay first-class through the
//! OpenAI-compatible API.

pub mod deepgram;
pub mod elevenlabs;
pub mod openai_compatible;

use std::env;

use serde::{Deserialize, Serialize};

use crate::providers::{AuthKind, SettingsStore, StoredSpeech};

pub const STT_TEST_WAV: &[u8] = include_bytes!("../../../../assets/stt-test.wav");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeechKind {
    Stt,
    Tts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeechApi {
    OpenAiCompatible,
    ElevenLabs,
    Deepgram,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SpeechManifest {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: SpeechKind,
    pub api: SpeechApi,
    pub auth: AuthKind,
    pub default_base_url: Option<&'static str>,
    pub default_model: Option<&'static str>,
    pub default_voice: Option<&'static str>,
    pub docs_url: &'static str,
    pub note: &'static str,
}

pub const STT_PROVIDERS: &[SpeechManifest] = &[
    SpeechManifest {
        id: "openai-compatible",
        label: "OpenAI-compatible STT",
        kind: SpeechKind::Stt,
        api: SpeechApi::OpenAiCompatible,
        auth: AuthKind::OptionalApiKey,
        default_base_url: None,
        default_model: Some("whisper-1"),
        default_voice: None,
        docs_url: "https://platform.openai.com/docs/guides/speech-to-text",
        note: "Any server exposing /v1/audio/transcriptions.",
    },
    SpeechManifest {
        id: "openai",
        label: "OpenAI transcription",
        kind: SpeechKind::Stt,
        api: SpeechApi::OpenAiCompatible,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.openai.com"),
        default_model: Some("whisper-1"),
        default_voice: None,
        docs_url: "https://platform.openai.com/docs/guides/speech-to-text",
        note: "Cloud transcription; API key required.",
    },
    SpeechManifest {
        id: "whisper-cpp",
        label: "whisper.cpp (local)",
        kind: SpeechKind::Stt,
        api: SpeechApi::OpenAiCompatible,
        auth: AuthKind::None,
        default_base_url: Some("http://127.0.0.1:8080"),
        default_model: Some("whisper-1"),
        default_voice: None,
        docs_url: "https://github.com/ggml-org/llama.cpp",
        note: "Local runner exposing the OpenAI transcription route.",
    },
    SpeechManifest {
        id: "elevenlabs",
        label: "ElevenLabs Scribe",
        kind: SpeechKind::Stt,
        api: SpeechApi::ElevenLabs,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.elevenlabs.io"),
        default_model: Some("scribe_v1"),
        default_voice: None,
        docs_url: "https://elevenlabs.io/docs/speech-to-text",
        note: "Cloud transcription; API key required.",
    },
    SpeechManifest {
        id: "deepgram",
        label: "Deepgram",
        kind: SpeechKind::Stt,
        api: SpeechApi::Deepgram,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.deepgram.com"),
        default_model: Some("nova-3"),
        default_voice: None,
        docs_url: "https://developers.deepgram.com/docs",
        note: "Cloud transcription; API key required.",
    },
];

pub const TTS_PROVIDERS: &[SpeechManifest] = &[
    SpeechManifest {
        id: "openai-compatible",
        label: "OpenAI-compatible TTS",
        kind: SpeechKind::Tts,
        api: SpeechApi::OpenAiCompatible,
        auth: AuthKind::OptionalApiKey,
        default_base_url: None,
        default_model: Some("tts-1"),
        default_voice: Some("alloy"),
        docs_url: "https://platform.openai.com/docs/guides/text-to-speech",
        note: "Any server exposing /v1/audio/speech.",
    },
    SpeechManifest {
        id: "openai",
        label: "OpenAI speech",
        kind: SpeechKind::Tts,
        api: SpeechApi::OpenAiCompatible,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.openai.com"),
        default_model: Some("tts-1"),
        default_voice: Some("alloy"),
        docs_url: "https://platform.openai.com/docs/guides/text-to-speech",
        note: "Cloud speech; API key required.",
    },
    SpeechManifest {
        id: "elevenlabs",
        label: "ElevenLabs",
        kind: SpeechKind::Tts,
        api: SpeechApi::ElevenLabs,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.elevenlabs.io"),
        default_model: Some("eleven_multilingual_v2"),
        default_voice: None,
        docs_url: "https://elevenlabs.io/docs/text-to-speech",
        note: "Cloud speech; API key and voice id required.",
    },
    SpeechManifest {
        id: "piper",
        label: "Piper (local, via shim)",
        kind: SpeechKind::Tts,
        api: SpeechApi::OpenAiCompatible,
        auth: AuthKind::None,
        default_base_url: None,
        default_model: Some("tts-1"),
        default_voice: Some("default"),
        docs_url: "https://github.com/OHF-Voice/piper1-gpl",
        note: "Run Piper as a separate process behind an OpenAI-compatible shim; GPL boundary stays outside the core.",
    },
];

pub fn find(kind: SpeechKind, id: &str) -> Option<&'static SpeechManifest> {
    let providers = match kind {
        SpeechKind::Stt => STT_PROVIDERS,
        SpeechKind::Tts => TTS_PROVIDERS,
    };
    providers.iter().find(|provider| provider.id == id)
}

#[derive(Debug, Clone)]
pub struct SttConfig {
    pub provider_id: Option<String>,
    pub api: SpeechApi,
    pub base_url: String,
    pub model: String,
    pub language: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub provider_id: Option<String>,
    pub api: SpeechApi,
    pub base_url: String,
    pub model: String,
    pub voice: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SpeechConfig {
    pub stt: Option<SttConfig>,
    pub tts: Option<TtsConfig>,
}

impl SpeechConfig {
    pub fn from_env() -> Self {
        Self {
            stt: stt_from_env(),
            tts: tts_from_env(),
        }
    }

    pub fn load(store: &SettingsStore) -> Self {
        let env_config = Self::from_env();
        let stored = store.load_speech();
        Self {
            stt: env_config
                .stt
                .or_else(|| stored_stt(stored.as_ref(), store)),
            tts: env_config
                .tts
                .or_else(|| stored_tts(stored.as_ref(), store)),
        }
    }
}

fn stt_from_env() -> Option<SttConfig> {
    let provider = env_string("CALLCRAFT_STT_PROVIDER");
    let url = env_string("CALLCRAFT_STT_URL");
    if provider.is_none() && url.is_none() {
        return None;
    }
    let id = provider.unwrap_or_else(|| "openai-compatible".to_string());
    let manifest = find(SpeechKind::Stt, &id)?;
    let base_url = url
        .or_else(|| manifest.default_base_url.map(str::to_string))
        .unwrap_or_default();
    Some(SttConfig {
        provider_id: Some(id),
        api: manifest.api,
        base_url,
        model: env_string("CALLCRAFT_STT_MODEL")
            .or_else(|| manifest.default_model.map(str::to_string))
            .unwrap_or_else(|| "whisper-1".to_string()),
        language: env_string("CALLCRAFT_STT_LANGUAGE").or_else(|| Some("en".to_string())),
        api_key: env_string("CALLCRAFT_STT_API_KEY")
            .or_else(|| env_string("CALLCRAFT_ADAPTER_TOKEN")),
    })
}

fn tts_from_env() -> Option<TtsConfig> {
    let provider = env_string("CALLCRAFT_TTS_PROVIDER");
    let url = env_string("CALLCRAFT_TTS_URL");
    if provider.is_none() && url.is_none() {
        return None;
    }
    let id = provider.unwrap_or_else(|| "openai-compatible".to_string());
    let manifest = find(SpeechKind::Tts, &id)?;
    let base_url = url
        .or_else(|| manifest.default_base_url.map(str::to_string))
        .unwrap_or_default();
    Some(TtsConfig {
        provider_id: Some(id),
        api: manifest.api,
        base_url,
        model: env_string("CALLCRAFT_TTS_MODEL")
            .or_else(|| manifest.default_model.map(str::to_string))
            .unwrap_or_else(|| "tts-1".to_string()),
        voice: env_string("CALLCRAFT_TTS_VOICE")
            .or_else(|| manifest.default_voice.map(str::to_string)),
        api_key: env_string("CALLCRAFT_TTS_API_KEY")
            .or_else(|| env_string("CALLCRAFT_ADAPTER_TOKEN")),
    })
}

fn stored_stt(stored: Option<&StoredSpeech>, store: &SettingsStore) -> Option<SttConfig> {
    let stored = stored?.stt.as_ref()?;
    let manifest = find(SpeechKind::Stt, &stored.provider_id)?;
    let base_url = if stored.base_url.trim().is_empty() {
        manifest
            .default_base_url
            .map(str::to_string)
            .unwrap_or_default()
    } else {
        stored.base_url.clone()
    };
    let api_key = env_string("CALLCRAFT_STT_API_KEY")
        .or_else(|| env_string("CALLCRAFT_ADAPTER_TOKEN"))
        .or_else(|| store.load_key_ns("stt", &stored.provider_id));
    Some(SttConfig {
        provider_id: Some(stored.provider_id.clone()),
        api: manifest.api,
        base_url,
        model: if stored.model.trim().is_empty() {
            manifest
                .default_model
                .map(str::to_string)
                .unwrap_or_else(|| "whisper-1".to_string())
        } else {
            stored.model.clone()
        },
        language: if stored.language.trim().is_empty() {
            Some("en".to_string())
        } else {
            Some(stored.language.clone())
        },
        api_key,
    })
}

fn stored_tts(stored: Option<&StoredSpeech>, store: &SettingsStore) -> Option<TtsConfig> {
    let stored = stored?.tts.as_ref()?;
    let manifest = find(SpeechKind::Tts, &stored.provider_id)?;
    let base_url = if stored.base_url.trim().is_empty() {
        manifest
            .default_base_url
            .map(str::to_string)
            .unwrap_or_default()
    } else {
        stored.base_url.clone()
    };
    let api_key = env_string("CALLCRAFT_TTS_API_KEY")
        .or_else(|| env_string("CALLCRAFT_ADAPTER_TOKEN"))
        .or_else(|| store.load_key_ns("tts", &stored.provider_id));
    Some(TtsConfig {
        provider_id: Some(stored.provider_id.clone()),
        api: manifest.api,
        base_url,
        model: if stored.model.trim().is_empty() {
            manifest
                .default_model
                .map(str::to_string)
                .unwrap_or_else(|| "tts-1".to_string())
        } else {
            stored.model.clone()
        },
        voice: if stored.voice.trim().is_empty() {
            manifest.default_voice.map(str::to_string)
        } else {
            Some(stored.voice.clone())
        },
        api_key,
    })
}

pub fn stt_from_request(
    provider_id: &str,
    base_url: Option<String>,
    model: Option<String>,
    language: Option<String>,
    api_key: Option<String>,
) -> Result<SttConfig, String> {
    let manifest = find(SpeechKind::Stt, provider_id)
        .ok_or_else(|| format!("unknown STT provider '{provider_id}'"))?;
    let api_key = clean(api_key);
    if manifest.auth == AuthKind::ApiKey && api_key.is_none() {
        return Err(format!("{} requires an API key", manifest.label));
    }
    let base_url = clean(base_url)
        .or_else(|| manifest.default_base_url.map(str::to_string))
        .ok_or_else(|| format!("{} requires a base URL", manifest.label))?;
    Ok(SttConfig {
        provider_id: Some(manifest.id.to_string()),
        api: manifest.api,
        base_url,
        model: clean(model)
            .or_else(|| manifest.default_model.map(str::to_string))
            .unwrap_or_else(|| "whisper-1".to_string()),
        language: clean(language).or_else(|| Some("en".to_string())),
        api_key,
    })
}

pub fn tts_from_request(
    provider_id: &str,
    base_url: Option<String>,
    model: Option<String>,
    voice: Option<String>,
    api_key: Option<String>,
) -> Result<TtsConfig, String> {
    let manifest = find(SpeechKind::Tts, provider_id)
        .ok_or_else(|| format!("unknown TTS provider '{provider_id}'"))?;
    let api_key = clean(api_key);
    if manifest.auth == AuthKind::ApiKey && api_key.is_none() {
        return Err(format!("{} requires an API key", manifest.label));
    }
    let base_url = clean(base_url)
        .or_else(|| manifest.default_base_url.map(str::to_string))
        .ok_or_else(|| format!("{} requires a base URL", manifest.label))?;
    let voice = clean(voice).or_else(|| manifest.default_voice.map(str::to_string));
    if manifest.api == SpeechApi::ElevenLabs && voice.is_none() {
        return Err("ElevenLabs requires a voice id".to_string());
    }
    Ok(TtsConfig {
        provider_id: Some(manifest.id.to_string()),
        api: manifest.api,
        base_url,
        model: clean(model)
            .or_else(|| manifest.default_model.map(str::to_string))
            .unwrap_or_else(|| "tts-1".to_string()),
        voice,
        api_key,
    })
}

#[derive(Debug)]
pub enum SpeechError {
    Transport(String),
    Invalid(String),
}

impl SpeechError {
    pub fn message(&self) -> &str {
        match self {
            SpeechError::Transport(message) | SpeechError::Invalid(message) => message,
        }
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, SpeechError::Invalid(_))
    }
}

pub async fn transcribe(
    client: &reqwest::Client,
    config: &SttConfig,
    mime: &str,
    audio: &[u8],
) -> Result<String, SpeechError> {
    match config.api {
        SpeechApi::OpenAiCompatible => {
            openai_compatible::transcribe(client, config, mime, audio).await
        }
        SpeechApi::ElevenLabs => elevenlabs::transcribe(client, config, mime, audio).await,
        SpeechApi::Deepgram => deepgram::transcribe(client, config, mime, audio).await,
    }
}

pub async fn synthesize(
    client: &reqwest::Client,
    config: &TtsConfig,
    text: &str,
) -> Result<(Vec<u8>, String), SpeechError> {
    match config.api {
        SpeechApi::OpenAiCompatible => openai_compatible::synthesize(client, config, text).await,
        SpeechApi::ElevenLabs => elevenlabs::synthesize(client, config, text).await,
        SpeechApi::Deepgram => Err(SpeechError::Invalid(
            "Deepgram TTS is not supported".to_string(),
        )),
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_string(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}
