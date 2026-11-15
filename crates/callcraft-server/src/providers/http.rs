//! Shared HTTP helpers for provider adapters.

use std::env;
use std::time::Duration;

/// Join a provider base URL with an API path, tolerating a base that already
/// ends in `/v1`.
pub fn base(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    if base_url.ends_with("/v1") {
        base_url.to_string()
    } else {
        format!("{base_url}/v1")
    }
}

pub fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base(base_url), path)
}

pub fn audio_filename(mime: &str) -> &'static str {
    let mime = mime.to_ascii_lowercase();
    if mime.contains("webm") {
        "turn.webm"
    } else if mime.contains("mp4") || mime.contains("m4a") {
        "turn.mp4"
    } else if mime.contains("wav") {
        "turn.wav"
    } else if mime.contains("ogg") {
        "turn.ogg"
    } else {
        "turn.bin"
    }
}

pub fn request_timeout(env_key: &str, default_ms: u64) -> Duration {
    env::var(env_key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(default_ms))
}

pub fn snippet(text: &str) -> String {
    text.chars().take(200).collect()
}
