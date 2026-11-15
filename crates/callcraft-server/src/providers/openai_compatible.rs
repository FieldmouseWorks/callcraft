use serde::Deserialize;
use serde_json::json;

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
    let url = endpoint(&config.base_url, "chat/completions");

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
