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

pub const STT_TEST_AUDIO: &[u8] = include_bytes!("../../../../assets/stt-test.webm");
pub const STT_TEST_MIME: &str = "audio/webm";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeechKind {
    Stt,
    Tts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SttApi {
    OpenAiCompatible,
    ElevenLabs,
    Deepgram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsApi {
    OpenAiCompatible,
    ElevenLabs,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SttManifest {
    pub id: &'static str,
    pub label: &'static str,
    pub api: SttApi,
    pub auth: AuthKind,
    pub default_base_url: Option<&'static str>,
    pub default_model: Option<&'static str>,
    pub docs_url: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TtsManifest {
    pub id: &'static str,
    pub label: &'static str,
    pub api: TtsApi,
    pub auth: AuthKind,
    pub default_base_url: Option<&'static str>,
    pub default_model: Option<&'static str>,
    pub default_voice: Option<&'static str>,
    pub docs_url: &'static str,
    pub note: &'static str,
}

pub const STT_PROVIDERS: &[SttManifest] = &[
    SttManifest {
        id: "openai-compatible",
        label: "OpenAI-compatible STT",
        api: SttApi::OpenAiCompatible,
        auth: AuthKind::OptionalApiKey,
        default_base_url: None,
        default_model: Some("whisper-1"),
        docs_url: "https://platform.openai.com/docs/guides/speech-to-text",
        note: "Any server exposing /v1/audio/transcriptions.",
    },
    SttManifest {
        id: "openai",
        label: "OpenAI transcription",
        api: SttApi::OpenAiCompatible,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.openai.com"),
        default_model: Some("whisper-1"),
        docs_url: "https://platform.openai.com/docs/guides/speech-to-text",
        note: "Cloud transcription; API key required.",
    },
    SttManifest {
        id: "whisper-cpp",
        label: "whisper.cpp (local)",
        api: SttApi::OpenAiCompatible,
        auth: AuthKind::None,
        default_base_url: Some("http://127.0.0.1:8080"),
        default_model: Some("whisper-1"),
        docs_url: "https://github.com/ggml-org/whisper.cpp",
        note: "Local runner exposing the OpenAI transcription route.",
    },
    SttManifest {
        id: "elevenlabs",
        label: "ElevenLabs Scribe",
        api: SttApi::ElevenLabs,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.elevenlabs.io"),
        default_model: Some("scribe_v1"),
        docs_url: "https://elevenlabs.io/docs/speech-to-text",
        note: "Cloud transcription; API key required.",
    },
    SttManifest {
        id: "deepgram",
        label: "Deepgram",
        api: SttApi::Deepgram,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.deepgram.com"),
        default_model: Some("nova-3"),
        docs_url: "https://developers.deepgram.com/docs",
        note: "Cloud transcription; API key required.",
    },
];

pub const TTS_PROVIDERS: &[TtsManifest] = &[
    TtsManifest {
        id: "openai-compatible",
        label: "OpenAI-compatible TTS",
        api: TtsApi::OpenAiCompatible,
        auth: AuthKind::OptionalApiKey,
        default_base_url: None,
        default_model: Some("tts-1"),
        default_voice: Some("alloy"),
        docs_url: "https://platform.openai.com/docs/guides/text-to-speech",
        note: "Any server exposing /v1/audio/speech.",
    },
    TtsManifest {
        id: "openai",
        label: "OpenAI speech",
        api: TtsApi::OpenAiCompatible,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.openai.com"),
        default_model: Some("tts-1"),
        default_voice: Some("alloy"),
        docs_url: "https://platform.openai.com/docs/guides/text-to-speech",
        note: "Cloud speech; API key required.",
    },
    TtsManifest {
        id: "elevenlabs",
        label: "ElevenLabs",
        api: TtsApi::ElevenLabs,
        auth: AuthKind::ApiKey,
        default_base_url: Some("https://api.elevenlabs.io"),
        default_model: Some("eleven_multilingual_v2"),
        default_voice: None,
        docs_url: "https://elevenlabs.io/docs/text-to-speech",
        note: "Cloud speech; API key and voice id required.",
    },
    TtsManifest {
        id: "piper",
        label: "Piper (local, via shim)",
        api: TtsApi::OpenAiCompatible,
        auth: AuthKind::None,
        default_base_url: None,
        default_model: Some("tts-1"),
        default_voice: Some("default"),
        docs_url: "https://github.com/OHF-Voice/piper1-gpl",
        note: "Run Piper as a separate process behind an OpenAI-compatible shim; GPL boundary stays outside the core.",
    },
];

pub fn find_stt(id: &str) -> Option<&'static SttManifest> {
    STT_PROVIDERS.iter().find(|provider| provider.id == id)
}

pub fn find_tts(id: &str) -> Option<&'static TtsManifest> {
    TTS_PROVIDERS.iter().find(|provider| provider.id == id)
}

#[derive(Debug, Clone)]
pub struct SttConfig {
    pub provider_id: Option<String>,
    pub api: SttApi,
    pub base_url: String,
    pub model: String,
    pub language: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub provider_id: Option<String>,
    pub api: TtsApi,
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
        let stt_env_invalid = env_string("CALLCRAFT_STT_PROVIDER")
            .is_some_and(|provider_id| find_stt(&provider_id).is_none());
        let tts_env_invalid = env_string("CALLCRAFT_TTS_PROVIDER")
            .is_some_and(|provider_id| find_tts(&provider_id).is_none());
        Self {
            stt: if stt_env_invalid {
                None
            } else {
                env_config
                    .stt
                    .or_else(|| stored_stt(stored.as_ref(), store))
            },
            tts: if tts_env_invalid {
                None
            } else {
                env_config
                    .tts
                    .or_else(|| stored_tts(stored.as_ref(), store))
            },
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
    let manifest = find_stt(&id)?;
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
    let manifest = find_tts(&id)?;
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
    let manifest = find_stt(&stored.provider_id)?;
    let base_url = if stored.base_url.trim().is_empty() {
        manifest
            .default_base_url
            .map(str::to_string)
            .unwrap_or_default()
    } else {
        stored.base_url.clone()
    };
    let env_provider = env_string("CALLCRAFT_STT_PROVIDER");
    let env_key = env_string("CALLCRAFT_STT_API_KEY")
        .filter(|_| env_provider.as_deref() == Some(stored.provider_id.as_str()));
    let api_key = env_key
        .or_else(|| {
            env_string("CALLCRAFT_ADAPTER_TOKEN")
                .filter(|_| env_provider.as_deref() == Some(stored.provider_id.as_str()))
        })
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
    let manifest = find_tts(&stored.provider_id)?;
    let base_url = if stored.base_url.trim().is_empty() {
        manifest
            .default_base_url
            .map(str::to_string)
            .unwrap_or_default()
    } else {
        stored.base_url.clone()
    };
    let env_provider = env_string("CALLCRAFT_TTS_PROVIDER");
    let env_key = env_string("CALLCRAFT_TTS_API_KEY")
        .filter(|_| env_provider.as_deref() == Some(stored.provider_id.as_str()));
    let api_key = env_key
        .or_else(|| {
            env_string("CALLCRAFT_ADAPTER_TOKEN")
                .filter(|_| env_provider.as_deref() == Some(stored.provider_id.as_str()))
        })
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
    let manifest =
        find_stt(provider_id).ok_or_else(|| format!("unknown STT provider '{provider_id}'"))?;
    let api_key = clean(api_key);
    if manifest.auth == AuthKind::ApiKey && api_key.is_none() {
        return Err(format!("{} requires an API key", manifest.label));
    }
    let base_url = clean(base_url)
        .or_else(|| manifest.default_base_url.map(str::to_string))
        .ok_or_else(|| format!("{} requires a base URL", manifest.label))?;
    crate::providers::ensure_safe_base_url(&base_url)?;
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
    let manifest =
        find_tts(provider_id).ok_or_else(|| format!("unknown TTS provider '{provider_id}'"))?;
    let api_key = clean(api_key);
    if manifest.auth == AuthKind::ApiKey && api_key.is_none() {
        return Err(format!("{} requires an API key", manifest.label));
    }
    let base_url = clean(base_url)
        .or_else(|| manifest.default_base_url.map(str::to_string))
        .ok_or_else(|| format!("{} requires a base URL", manifest.label))?;
    crate::providers::ensure_safe_base_url(&base_url)?;
    let voice = clean(voice).or_else(|| manifest.default_voice.map(str::to_string));
    if manifest.api == TtsApi::ElevenLabs && voice.is_none() {
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

pub use crate::providers::ProviderError as SpeechError;

pub async fn transcribe(
    client: &reqwest::Client,
    config: &SttConfig,
    mime: &str,
    audio: &[u8],
) -> Result<String, SpeechError> {
    crate::providers::ensure_safe_base_url(&config.base_url).map_err(SpeechError::Invalid)?;
    match config.api {
        SttApi::OpenAiCompatible => {
            openai_compatible::transcribe(client, config, mime, audio).await
        }
        SttApi::ElevenLabs => elevenlabs::transcribe(client, config, mime, audio).await,
        SttApi::Deepgram => deepgram::transcribe(client, config, mime, audio).await,
    }
}

pub async fn synthesize(
    client: &reqwest::Client,
    config: &TtsConfig,
    text: &str,
) -> Result<(Vec<u8>, String), SpeechError> {
    crate::providers::ensure_safe_base_url(&config.base_url).map_err(SpeechError::Invalid)?;
    match config.api {
        TtsApi::OpenAiCompatible => openai_compatible::synthesize(client, config, text).await,
        TtsApi::ElevenLabs => elevenlabs::synthesize(client, config, text).await,
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
