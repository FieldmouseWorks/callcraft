use serde::Deserialize;
use serde_json::json;

use super::{SpeechError, SttConfig, TtsConfig};

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    #[serde(default)]
    text: String,
}

pub async fn transcribe(
    client: &reqwest::Client,
    config: &SttConfig,
    mime: &str,
    audio: &[u8],
) -> Result<String, SpeechError> {
    if config.base_url.trim().is_empty() {
        return Err(SpeechError::Invalid(
            "speech-to-text base URL is empty".to_string(),
        ));
    }
    let part = reqwest::multipart::Part::bytes(audio.to_vec())
        .file_name("turn.webm")
        .mime_str(mime)
        .map_err(|error| SpeechError::Invalid(format!("audio part: {error}")))?;
    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", config.model.clone());
    if let Some(language) = &config.language {
        form = form.text("language", language.clone());
    }
    let mut request = client
        .post(endpoint(&config.base_url, "audio/transcriptions"))
        .multipart(form);
    if let Some(api_key) = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await.map_err(|error| {
        SpeechError::Transport(format!("speech-to-text request failed: {error}"))
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(SpeechError::Transport(format!(
            "speech-to-text returned {status}: {}",
            snippet(&body)
        )));
    }
    let payload: TranscriptionResponse = response
        .json()
        .await
        .map_err(|error| SpeechError::Invalid(format!("speech-to-text response: {error}")))?;
    if payload.text.trim().is_empty() {
        return Err(SpeechError::Invalid(
            "speech-to-text returned an empty transcript".to_string(),
        ));
    }
    Ok(payload.text)
}

pub async fn synthesize(
    client: &reqwest::Client,
    config: &TtsConfig,
    text: &str,
) -> Result<(Vec<u8>, String), SpeechError> {
    if config.base_url.trim().is_empty() {
        return Err(SpeechError::Invalid(
            "text-to-speech base URL is empty".to_string(),
        ));
    }
    let mut request = client
        .post(endpoint(&config.base_url, "audio/speech"))
        .json(&json!({
            "model": config.model,
            "input": text,
            "voice": config.voice.clone().unwrap_or_else(|| "default".to_string()),
            "response_format": "wav",
        }));
    if let Some(api_key) = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await.map_err(|error| {
        SpeechError::Transport(format!("text-to-speech request failed: {error}"))
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(SpeechError::Transport(format!(
            "text-to-speech returned {status}: {}",
            snippet(&body)
        )));
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/wav")
        .to_string();
    let audio = response
        .bytes()
        .await
        .map_err(|error| SpeechError::Invalid(format!("text-to-speech body: {error}")))?
        .to_vec();
    if audio.is_empty() {
        return Err(SpeechError::Invalid(
            "text-to-speech returned empty audio".to_string(),
        ));
    }
    Ok((audio, mime))
}

fn endpoint(base_url: &str, path: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    if base_url.ends_with("/v1") {
        format!("{base_url}/{path}")
    } else {
        format!("{base_url}/v1/{path}")
    }
}

fn snippet(text: &str) -> String {
    text.chars().take(200).collect()
}
