use serde::Deserialize;
use serde_json::json;

use super::{CallerConfig, ProviderError};

/// Anthropic's current Messages API version. Sampling fields (`temperature`,
/// `top_p`, `top_k`) are omitted because they return 400 on this version.
const ANTHROPIC_VERSION: &str = "2026-10-21";

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<Block>,
}

#[derive(Debug, Deserialize)]
struct Block {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
}

pub async fn complete(
    client: &reqwest::Client,
    config: &CallerConfig,
    system: &str,
    user: &str,
) -> Result<String, ProviderError> {
    let api_key = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| ProviderError::Invalid("Anthropic requires an API key".to_string()))?;
    if config.base_url.trim().is_empty() {
        return Err(ProviderError::Invalid(
            "Anthropic base URL is empty".to_string(),
        ));
    }
    let url = endpoint(&config.base_url);

    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": config.model,
            "max_tokens": 256,
            "system": system,
            "messages": [
                {"role": "user", "content": user}
            ]
        }))
        .send()
        .await
        .map_err(|error| ProviderError::Transport(format!("provider request failed: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::Transport(format!(
            "provider returned {status}: {}",
            snippet(&body)
        )));
    }

    let payload: MessagesResponse = response
        .json()
        .await
        .map_err(|error| ProviderError::Invalid(format!("provider response: {error}")))?;
    let text = payload
        .content
        .iter()
        .filter(|block| block.kind == "text")
        .map(|block| block.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err(ProviderError::Invalid(
            "provider returned an empty response".to_string(),
        ));
    }
    Ok(text)
}

fn endpoint(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    if base_url.ends_with("/v1") {
        format!("{base_url}/messages")
    } else {
        format!("{base_url}/v1/messages")
    }
}

fn snippet(text: &str) -> String {
    text.chars().take(200).collect()
}
