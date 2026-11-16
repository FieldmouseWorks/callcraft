use serde::Deserialize;

use super::{SpeechError, SttConfig};
use crate::providers::http;

#[derive(Debug, Deserialize)]
struct ListenResponse {
    results: Results,
}

#[derive(Debug, Deserialize)]
struct Results {
    #[serde(default)]
    channels: Vec<Channel>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    #[serde(default)]
    transcript: String,
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
        .ok_or_else(|| SpeechError::Invalid("Deepgram requires an API key".to_string()))?;
    let language = config
        .language
        .as_ref()
        .map(|language| format!("&language={language}"))
        .unwrap_or_default();
    let url = format!(
        "{}/listen?model={}&smart_format=true{}",
        http::base(&config.base_url),
        config.model,
        language
    );
    let response = client
        .post(url)
        .header("Authorization", format!("Token {api_key}"))
        .header("Content-Type", mime)
        .body(audio.to_vec())
        .timeout(crate::providers::request_timeout(
            "CALLCRAFT_STT_TIMEOUT_MS",
            8000,
        ))
        .send()
        .await
        .map_err(|error| SpeechError::Transport(format!("Deepgram request failed: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(SpeechError::Transport(format!(
            "Deepgram returned {status}: {}",
            http::snippet(&body)
        )));
    }
    let payload: ListenResponse = response
        .json()
        .await
        .map_err(|error| SpeechError::Invalid(format!("Deepgram response: {error}")))?;
    let transcript = payload
        .results
        .channels
        .first()
        .and_then(|channel| channel.alternatives.first())
        .map(|alternative| alternative.transcript.clone())
        .unwrap_or_default();
    if transcript.trim().is_empty() {
        return Err(SpeechError::Invalid(
            "Deepgram returned an empty transcript".to_string(),
        ));
    }
    Ok(transcript)
}
