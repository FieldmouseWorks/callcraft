use serde::Deserialize;
use serde_json::json;

use super::http;
use super::{CallerConfig, ProviderError};

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    content: String,
}

pub async fn complete(
    client: &reqwest::Client,
    config: &CallerConfig,
    system: &str,
    user: &str,
) -> Result<String, ProviderError> {
    if config.base_url.trim().is_empty() {
        return Err(ProviderError::Invalid(
            "OpenAI-compatible base URL is empty".to_string(),
        ));
    }
    let url = http::endpoint(&config.base_url, "chat/completions");

    let mut request = client.post(url).json(&json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    }));
    if let Some(api_key) = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(api_key);
    }

    let response = request
        .timeout(crate::providers::request_timeout(
            "CALLCRAFT_CALLER_TIMEOUT_MS",
            5000,
        ))
        .send()
        .await
        .map_err(|error| ProviderError::Transport(format!("provider request failed: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::Transport(format!(
            "provider returned {status}: {}",
            http::snippet(&body)
        )));
    }

    let payload: ChatResponse = response
        .json()
        .await
        .map_err(|error| ProviderError::Invalid(format!("provider response: {error}")))?;
    let content = payload
        .choices
        .first()
        .map(|choice| choice.message.content.clone())
        .unwrap_or_default();
    if content.trim().is_empty() {
        return Err(ProviderError::Invalid(
            "provider returned an empty response".to_string(),
        ));
    }
    Ok(content)
}
