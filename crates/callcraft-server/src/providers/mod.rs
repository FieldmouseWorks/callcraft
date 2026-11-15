//! Provider registry, caller configuration, and credential storage.
//!
//! Provider-specific HTTP shapes live in their own modules. The registry is a
//! static list of presets so adding a provider is a module plus a manifest, not
//! a rewrite. API keys go to the OS keychain; non-secret settings go to a local
//! config file.

pub mod anthropic;
pub mod http;
pub mod openai_compatible;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;

use serde::{Deserialize, Serialize};

pub use http::{audio_filename, request_timeout};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    OpenAiCompatible,
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthKind {
    None,
    ApiKey,
    OptionalApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubscriptionOAuth {
    NotApplicable,
    PartnerGated,
    Prohibited,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProviderManifest {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ProviderKind,
    pub auth: AuthKind,
    pub subscription_oauth: SubscriptionOAuth,
    pub default_base_url: Option<&'static str>,
    pub docs_url: &'static str,
    pub note: &'static str,
}

pub const PROVIDERS: &[ProviderManifest] = &[
    ProviderManifest {
        id: "openai",
        label: "OpenAI API",
        kind: ProviderKind::OpenAi,
        auth: AuthKind::ApiKey,
        subscription_oauth: SubscriptionOAuth::PartnerGated,
        default_base_url: Some("https://api.openai.com"),
        docs_url: "https://platform.openai.com/docs",
        note: "API key. Sign in with ChatGPT plan usage is partner-gated.",
    },
    ProviderManifest {
        id: "anthropic",
        label: "Anthropic API",
        kind: ProviderKind::Anthropic,
        auth: AuthKind::ApiKey,
        subscription_oauth: SubscriptionOAuth::Prohibited,
        default_base_url: Some("https://api.anthropic.com"),
        docs_url: "https://docs.anthropic.com",
        note: "API key only. Consumer subscription OAuth is prohibited for third-party apps.",
    },
    ProviderManifest {
        id: "openai-compatible",
        label: "OpenAI-compatible endpoint",
        kind: ProviderKind::OpenAiCompatible,
        auth: AuthKind::OptionalApiKey,
        subscription_oauth: SubscriptionOAuth::NotApplicable,
        default_base_url: None,
        docs_url: "https://platform.openai.com/docs/api-reference/chat",
        note: "Any server exposing /v1/chat/completions, with or without a key.",
    },
    ProviderManifest {
        id: "ollama",
        label: "Ollama (local)",
        kind: ProviderKind::OpenAiCompatible,
        auth: AuthKind::None,
        subscription_oauth: SubscriptionOAuth::NotApplicable,
        default_base_url: Some("http://127.0.0.1:11434/v1"),
        docs_url: "https://ollama.com",
        note: "Local model runner; no key, no cloud data path.",
    },
    ProviderManifest {
        id: "llama-cpp",
        label: "llama.cpp server (local)",
        kind: ProviderKind::OpenAiCompatible,
        auth: AuthKind::None,
        subscription_oauth: SubscriptionOAuth::NotApplicable,
        default_base_url: Some("http://127.0.0.1:8080/v1"),
        docs_url: "https://github.com/ggml-org/llama.cpp",
        note: "Local model runner; no key, no cloud data path.",
    },
    ProviderManifest {
        id: "lm-studio",
        label: "LM Studio (local)",
        kind: ProviderKind::OpenAiCompatible,
        auth: AuthKind::None,
        subscription_oauth: SubscriptionOAuth::NotApplicable,
        default_base_url: Some("http://127.0.0.1:1234/v1"),
        docs_url: "https://lmstudio.ai",
        note: "Local model runner; no key, no cloud data path.",
    },
    ProviderManifest {
        id: "vllm",
        label: "vLLM (local)",
        kind: ProviderKind::OpenAiCompatible,
        auth: AuthKind::None,
        subscription_oauth: SubscriptionOAuth::NotApplicable,
        default_base_url: Some("http://127.0.0.1:8000/v1"),
        docs_url: "https://docs.vllm.ai",
        note: "Local model runner; no key, no cloud data path.",
    },
    ProviderManifest {
        id: "openrouter",
        label: "OpenRouter",
        kind: ProviderKind::OpenAiCompatible,
        auth: AuthKind::ApiKey,
        subscription_oauth: SubscriptionOAuth::NotApplicable,
        default_base_url: Some("https://openrouter.ai/api/v1"),
        docs_url: "https://openrouter.ai/docs",
        note: "One API key fronting many models; cloud data path.",
    },
];

pub fn find(id: &str) -> Option<&'static ProviderManifest> {
    PROVIDERS.iter().find(|provider| provider.id == id)
}

#[derive(Debug, Clone)]
pub struct CallerConfig {
    pub provider_id: Option<String>,
    pub kind: Option<ProviderKind>,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl Default for CallerConfig {
    fn default() -> Self {
        Self {
            provider_id: None,
            kind: None,
            base_url: String::new(),
            model: "default".to_string(),
            api_key: None,
        }
    }
}

impl CallerConfig {
    pub fn is_enabled(&self) -> bool {
        self.kind.is_some()
    }

    /// Environment variables win over stored settings. If only a URL is set,
    /// it is treated as a custom OpenAI-compatible endpoint.
    pub fn from_env() -> Option<Self> {
        let provider_id = env_string("CALLCRAFT_CALLER_PROVIDER");
        let url = env_string("CALLCRAFT_CALLER_URL");
        if provider_id.is_none() && url.is_none() {
            return None;
        }

        let id = provider_id.unwrap_or_else(|| "openai-compatible".to_string());
        let manifest = find(&id);
        let kind = manifest
            .map(|manifest| manifest.kind)
            .unwrap_or(ProviderKind::OpenAiCompatible);
        let base_url = url
            .or_else(|| manifest.and_then(|manifest| manifest.default_base_url.map(str::to_string)))
            .unwrap_or_default();
        let model = env_string("CALLCRAFT_CALLER_MODEL").unwrap_or_else(|| "default".to_string());
        let api_key = env_string("CALLCRAFT_CALLER_API_KEY");

        Some(Self {
            provider_id: Some(id),
            kind: Some(kind),
            base_url,
            model,
            api_key,
        })
    }

    /// Load environment first, then the stored config and its key.
    pub fn load(store: &SettingsStore) -> Self {
        if let Some(provider_id) = env_string("CALLCRAFT_CALLER_PROVIDER")
            && find(&provider_id).is_none()
        {
            return Self::default();
        }
        if let Some(config) = Self::from_env() {
            return config;
        }
        if let Some(stored) = store.load() {
            let Some(manifest) = find(&stored.provider_id) else {
                return Self::default();
            };
            let base_url = if stored.base_url.trim().is_empty() {
                manifest
                    .default_base_url
                    .map(str::to_string)
                    .unwrap_or_default()
            } else {
                stored.base_url
            };
            let env_provider = env_string("CALLCRAFT_CALLER_PROVIDER");
            let api_key = env_string("CALLCRAFT_CALLER_API_KEY")
                .filter(|_| env_provider.as_deref() == Some(stored.provider_id.as_str()))
                .or_else(|| store.load_key(&stored.provider_id));
            return Self {
                provider_id: Some(stored.provider_id),
                kind: Some(manifest.kind),
                base_url,
                model: if stored.model.trim().is_empty() {
                    "default".to_string()
                } else {
                    stored.model
                },
                api_key,
            };
        }
        Self::default()
    }
}

pub fn config_from_request(
    provider_id: &str,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
) -> Result<CallerConfig, String> {
    let manifest = find(provider_id).ok_or_else(|| format!("unknown provider '{provider_id}'"))?;
    let api_key = api_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if manifest.auth == AuthKind::ApiKey && api_key.is_none() {
        return Err(format!("{} requires an API key", manifest.label));
    }
    let base_url = base_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| manifest.default_base_url.map(str::to_string))
        .ok_or_else(|| format!("{} requires a base URL", manifest.label))?;
    ensure_safe_base_url(&base_url)?;
    let model = model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default".to_string());

    Ok(CallerConfig {
        provider_id: Some(manifest.id.to_string()),
        kind: Some(manifest.kind),
        base_url,
        model,
        api_key,
    })
}

#[derive(Debug)]
pub enum ProviderError {
    Transport(String),
    Invalid(String),
}

impl ProviderError {
    pub fn message(&self) -> &str {
        match self {
            ProviderError::Transport(message) | ProviderError::Invalid(message) => message,
        }
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, ProviderError::Invalid(_))
    }
}

/// Reject plaintext HTTP for non-local provider endpoints. Local runners and
/// private-network gateways are allowed; a remote provider must use HTTPS
/// unless an operator explicitly opts out.
/// Compare provider base URLs without trailing slashes.
pub fn same_base(left: &str, right: &str) -> bool {
    let left = left.trim_end_matches('/');
    let right = right.trim_end_matches('/');
    !left.is_empty() && left.eq_ignore_ascii_case(right)
}

pub fn ensure_safe_base_url(base_url: &str) -> Result<(), String> {
    let url = url::Url::parse(base_url).map_err(|error| format!("invalid base URL: {error}"))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            if env_string("CALLCRAFT_ALLOW_INSECURE_ADAPTERS").as_deref() == Some("1") {
                return Ok(());
            }
            let host = url.host_str().unwrap_or_default();
            if is_loopback_or_private(host) {
                Ok(())
            } else {
                Err("non-loopback provider endpoints must use https (set CALLCRAFT_ALLOW_INSECURE_ADAPTERS=1 to override)".to_string())
            }
        }
        other => Err(format!("unsupported provider URL scheme '{other}'")),
    }
}

fn is_loopback_or_private(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let host = host.trim_matches(['[', ']']);
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(address)) => address.is_loopback() || address.is_private(),
        Ok(std::net::IpAddr::V6(address)) => {
            address.is_loopback()
                || (address.segments()[0] & 0xfe00) == 0xfc00
                || (address.segments()[0] & 0xffc0) == 0xfe80
        }
        Err(_) => false,
    }
}

pub async fn complete(
    client: &reqwest::Client,
    config: &CallerConfig,
    system: &str,
    user: &str,
) -> Result<String, ProviderError> {
    ensure_safe_base_url(&config.base_url).map_err(ProviderError::Invalid)?;
    match config.kind {
        Some(ProviderKind::OpenAiCompatible) | Some(ProviderKind::OpenAi) => {
            openai_compatible::complete(client, config, system, user).await
        }
        Some(ProviderKind::Anthropic) => anthropic::complete(client, config, system, user).await,
        None => Err(ProviderError::Invalid(
            "caller provider is not configured".to_string(),
        )),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConfig {
    pub provider_id: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredStt {
    pub provider_id: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTts {
    pub provider_id: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub voice: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredSpeech {
    #[serde(default)]
    pub stt: Option<StoredStt>,
    #[serde(default)]
    pub tts: Option<StoredTts>,
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    config_path: PathBuf,
    keyring_enabled: bool,
}

impl SettingsStore {
    pub fn from_env() -> Self {
        let root = env::var("CALLCRAFT_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_config_dir());
        Self {
            config_path: root.join("callcraft").join("adapters.json"),
            keyring_enabled: env::var("CALLCRAFT_DISABLE_KEYRING").ok().as_deref() != Some("1"),
        }
    }

    pub fn with_path(config_path: PathBuf, keyring_enabled: bool) -> Self {
        Self {
            config_path,
            keyring_enabled,
        }
    }

    pub fn load(&self) -> Option<StoredConfig> {
        let text = fs::read_to_string(&self.config_path).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, config: &StoredConfig) -> Result<(), String> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("config directory: {error}"))?;
        }
        let text = serde_json::to_string_pretty(config)
            .map_err(|error| format!("config serialize: {error}"))?;
        write_private(&self.config_path, &text)
    }

    pub fn load_speech(&self) -> Option<StoredSpeech> {
        let text = fs::read_to_string(self.speech_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save_speech(&self, speech: &StoredSpeech) -> Result<(), String> {
        let text = serde_json::to_string_pretty(speech)
            .map_err(|error| format!("speech config serialize: {error}"))?;
        write_private(&self.speech_path(), &text)
    }

    pub fn clear_speech(&self) -> Result<(), String> {
        remove_if_exists(&self.speech_path())
    }

    fn speech_path(&self) -> PathBuf {
        self.config_path.with_file_name("speech.json")
    }

    pub fn clear(&self) -> Result<(), String> {
        remove_if_exists(&self.config_path)
    }

    pub fn keyring_enabled(&self) -> bool {
        self.keyring_enabled
    }

    pub fn load_key(&self, provider_id: &str) -> Option<String> {
        self.load_key_ns("provider", provider_id)
    }

    pub fn save_key(&self, provider_id: &str, api_key: &str) -> Result<(), String> {
        self.save_key_ns("provider", provider_id, api_key)
    }

    pub fn delete_key(&self, provider_id: &str) {
        self.delete_key_ns("provider", provider_id);
    }

    pub fn load_key_ns(&self, namespace: &str, provider_id: &str) -> Option<String> {
        if !self.keyring_enabled {
            return None;
        }
        keyring_entry(namespace, provider_id)
            .ok()?
            .get_password()
            .ok()
    }

    pub fn save_key_ns(
        &self,
        namespace: &str,
        provider_id: &str,
        api_key: &str,
    ) -> Result<(), String> {
        if !self.keyring_enabled {
            return Err("OS keychain is unavailable or disabled".to_string());
        }
        keyring_entry(namespace, provider_id)?
            .set_password(api_key)
            .map_err(|error| format!("keychain: {error}"))
    }

    pub fn delete_key_ns(&self, namespace: &str, provider_id: &str) {
        if !self.keyring_enabled {
            return;
        }
        if let Ok(entry) = keyring_entry(namespace, provider_id) {
            let _ = entry.delete_credential();
        }
    }
}

fn keyring_entry(namespace: &str, provider_id: &str) -> Result<keyring_core::Entry, String> {
    init_keyring();
    keyring_core::Entry::new("callcraft", &format!("{namespace}:{provider_id}"))
        .map_err(|error| format!("keychain: {error}"))
}

fn write_private(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("config directory: {error}"))?;
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, format!("{text}\n")).map_err(|error| format!("config write: {error}"))?;
    set_private_permissions(&temp_path)?;
    fs::rename(&temp_path, path).map_err(|error| format!("config replace: {error}"))
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("config remove: {error}")),
    }
}

fn init_keyring() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        #[cfg(target_os = "linux")]
        if let Ok(store) = dbus_secret_service_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
        }
        #[cfg(target_os = "windows")]
        if let Ok(store) = windows_native_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
        }
        #[cfg(target_os = "macos")]
        if let Ok(store) = apple_native_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
        }
    });
}

fn default_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Ok(appdata) = env::var("APPDATA") {
        return PathBuf::from(appdata);
    }
    #[cfg(target_os = "macos")]
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg);
        }
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(".config");
        }
    }
    PathBuf::from(".callcraft")
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|error| format!("config permissions: {error}"))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn env_string(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_config_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SettingsStore::with_path(dir.path().join("adapters.json"), false);
        assert!(store.load().is_none());

        let config = StoredConfig {
            provider_id: "ollama".to_string(),
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model: "llama3".to_string(),
        };
        store.save(&config).expect("save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded.provider_id, "ollama");
        assert_eq!(loaded.model, "llama3");
        store.clear().expect("clear");
        assert!(store.load().is_none());
    }

    #[test]
    fn api_key_required_for_key_providers() {
        let error = config_from_request("anthropic", None, Some("x".into()), None)
            .expect_err("missing key");
        assert!(error.contains("API key"));
    }

    #[test]
    fn local_provider_needs_no_key() {
        let config =
            config_from_request("ollama", None, Some("llama3".into()), None).expect("local config");
        assert_eq!(config.kind, Some(ProviderKind::OpenAiCompatible));
        assert_eq!(config.base_url, "http://127.0.0.1:11434/v1");
    }
}
