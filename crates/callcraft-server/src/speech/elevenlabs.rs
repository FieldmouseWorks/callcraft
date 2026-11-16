use serde::Deserialize;
use serde_json::json;

use super::{SpeechError, SttConfig, TtsConfig};
use crate::providers::http;

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
    let api_key = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| SpeechError::Invalid("ElevenLabs requires an API key".to_string()))?;
    let part = reqwest::multipart::Part::bytes(audio.to_vec())
        .file_name(crate::providers::audio_filename(mime))
        .mime_str(mime)
        .map_err(|error| SpeechError::Invalid(format!("audio part: {error}")))?;
    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model_id", config.model.clone());
    if let Some(language) = &config.language {
        form = form.text("language_code", language.clone());
    }
    let response = client
        .post(http::endpoint(&config.base_url, "speech-to-text"))
        .header("xi-api-key", api_key)
        .multipart(form)
        .timeout(crate::providers::request_timeout(
            "CALLCRAFT_STT_TIMEOUT_MS",
            8000,
        ))
        .send()
        .await
        .map_err(|error| {
            SpeechError::Transport(format!("ElevenLabs STT request failed: {error}"))
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(SpeechError::Transport(format!(
            "ElevenLabs STT returned {status}: {}",
            http::snippet(&body)
        )));
    }
    let payload: TranscriptionResponse = response
        .json()
        .await
        .map_err(|error| SpeechError::Invalid(format!("ElevenLabs STT response: {error}")))?;
    if payload.text.trim().is_empty() {
        return Err(SpeechError::Invalid(
            "ElevenLabs STT returned an empty transcript".to_string(),
        ));
    }
    Ok(payload.text)
}

pub async fn synthesize(
    client: &reqwest::Client,
    config: &TtsConfig,
    text: &str,
) -> Result<(Vec<u8>, String), SpeechError> {
    let api_key = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| SpeechError::Invalid("ElevenLabs requires an API key".to_string()))?;
    let Some(voice) = config
        .voice
        .as_deref()
        .filter(|voice| !voice.trim().is_empty())
    else {
        return Err(SpeechError::Invalid(
            "ElevenLabs requires a voice id".to_string(),
        ));
    };
    let response = client
        .post(http::endpoint(
            &config.base_url,
            &format!("text-to-speech/{voice}"),
        ))
        .header("xi-api-key", api_key)
        .header("accept", "audio/mpeg")
        .json(&json!({
            "text": text,
            "model_id": config.model,
        }))
        .timeout(crate::providers::request_timeout(
            "CALLCRAFT_TTS_TIMEOUT_MS",
            8000,
        ))
        .send()
        .await
        .map_err(|error| {
            SpeechError::Transport(format!("ElevenLabs TTS request failed: {error}"))
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(SpeechError::Transport(format!(
            "ElevenLabs TTS returned {status}: {}",
            http::snippet(&body)
        )));
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/mpeg")
        .to_string();
    let audio = response
        .bytes()
        .await
        .map_err(|error| SpeechError::Invalid(format!("ElevenLabs TTS body: {error}")))?
        .to_vec();
    if audio.is_empty() {
        return Err(SpeechError::Invalid(
            "ElevenLabs TTS returned empty audio".to_string(),
        ));
    }
    Ok((audio, mime))
}
