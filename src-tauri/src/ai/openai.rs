//! OpenAI-compatible AI provider (supports OpenAI API and compatible endpoints).

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";

pub struct OpenAiProvider {
    client: Client,
    state: RwLock<Option<ProviderState>>,
}

struct ProviderState {
    api_key: String,
    endpoint: String,
}

impl OpenAiProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            state: RwLock::new(None),
        }
    }

    fn build_url(endpoint: &str, path: &str) -> String {
        let base = endpoint.trim_end_matches('/');
        format!("{base}{path}")
    }
}

// ─── OpenAI API wire types ───

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
    usage: Option<ApiUsage>,
    model: String,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: Option<ApiMessage>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct ApiStreamChunk {
    choices: Vec<ApiStreamChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ApiStreamChoice {
    delta: Option<ApiDelta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
    #[serde(default)]
    code: Option<String>,
}

impl From<&ChatMessage> for ApiMessage {
    fn from(msg: &ChatMessage) -> Self {
        Self {
            role: match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            }
            .into(),
            content: msg.content.clone(),
        }
    }
}

fn map_api_error(status: reqwest::StatusCode, body: &str) -> AiError {
    if let Ok(err_resp) = serde_json::from_str::<ApiErrorResponse>(body) {
        let msg = err_resp.error.message;
        let code = err_resp.error.code.as_deref().unwrap_or("");

        return match status.as_u16() {
            401 => AiError::InvalidApiKey,
            429 => AiError::RateLimited {
                retry_after_secs: 60,
            },
            _ if code == "context_length_exceeded" => AiError::ContextLengthExceeded {
                used: 0,
                limit: 0,
            },
            _ if code == "model_not_found" => AiError::ModelNotFound(msg),
            _ => AiError::RequestFailed(msg),
        };
    }
    AiError::RequestFailed(format!("HTTP {status}: {body}"))
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::OpenAi
    }

    fn display_name(&self) -> &str {
        "OpenAI"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: "gpt-4o-mini".into(),
                display_name: "GPT-4o mini".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: "gpt-4-turbo".into(),
                display_name: "GPT-4 Turbo".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: "o3-mini".into(),
                display_name: "o3-mini".into(),
                context_window: 200_000,
                supports_streaming: true,
                supports_tools: true,
            },
        ]
    }

    fn default_model(&self) -> &str {
        "gpt-4o-mini"
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required for OpenAI".into()))?;

        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);
        let url = Self::build_url(endpoint, "/models");

        let resp = self
            .client
            .get(&url)
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        if resp.status() == 401 {
            return Err(AiError::InvalidApiKey);
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AiError::RequestFailed(format!("Validation failed: {body}")));
        }
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let api_key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required for OpenAI".into()))?;

        let endpoint = config
            .endpoint
            .as_deref()
            .unwrap_or(DEFAULT_ENDPOINT)
            .to_string();

        *self.state.write().await = Some(ProviderState {
            api_key: api_key.to_string(),
            endpoint,
        });

        Ok(())
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let state_guard = self.state.read().await;
        let state = state_guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("OpenAI provider not initialized".into()))?;

        let url = Self::build_url(&state.endpoint, "/chat/completions");

        let body = ApiRequest {
            model: request.model.clone(),
            messages: request.messages.iter().map(|m| m.into()).collect(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop.clone(),
            stream: Some(false),
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&state.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_api_error(status, &text));
        }

        let api_resp: ApiResponse = resp
            .json()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let choice = api_resp
            .choices
            .first()
            .ok_or_else(|| AiError::RequestFailed("No choices in response".into()))?;

        let content = choice
            .message
            .as_ref()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let usage = api_resp
            .usage
            .map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content,
            model: api_resp.model,
            finish_reason: choice.finish_reason.clone(),
            usage,
        })
    }

    async fn reset(&self) {
        *self.state.write().await = None;
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let state_guard = self.state.read().await;
        let state = state_guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("OpenAI provider not initialized".into()))?;

        let url = Self::build_url(&state.endpoint, "/chat/completions");

        let body = ApiRequest {
            model: request.model.clone(),
            messages: request.messages.iter().map(|m| m.into()).collect(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop.clone(),
            stream: Some(true),
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&state.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_api_error(status, &text));
        }

        let mut byte_buf = Vec::new();
        let mut stream = resp.bytes_stream();

        // Process SSE stream: lines starting with "data: " contain JSON chunks
        while let Some(chunk_result) =
            futures_util::StreamExt::next(&mut stream).await
        {
            let chunk_bytes = match chunk_result {
                Ok(b) => b,
                Err(e) => {
                    let _ = sender.send(Err(AiError::RequestFailed(e.to_string()))).await;
                    return Ok(());
                }
            };

            byte_buf.extend_from_slice(&chunk_bytes);

            // Parse complete SSE lines from buffer
            while let Some(line_end) = byte_buf.iter().position(|&b| b == b'\n') {
                let line = String::from_utf8_lossy(&byte_buf[..line_end])
                    .trim()
                    .to_string();
                byte_buf.drain(..=line_end);

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                let data = if let Some(d) = line.strip_prefix("data: ") {
                    d.trim()
                } else {
                    continue;
                };

                if data == "[DONE]" {
                    let _ = sender
                        .send(Ok(StreamChunk {
                            content: String::new(),
                            done: true,
                            usage: None,
                        }))
                        .await;
                    return Ok(());
                }

                if let Ok(chunk) = serde_json::from_str::<ApiStreamChunk>(data) {
                    if let Some(choice) = chunk.choices.first() {
                        let content = choice
                            .delta
                            .as_ref()
                            .and_then(|d| d.content.clone())
                            .unwrap_or_default();

                        let done = choice.finish_reason.is_some();
                        let usage = chunk.usage.map(|u| TokenUsage {
                            prompt_tokens: u.prompt_tokens,
                            completion_tokens: u.completion_tokens,
                            total_tokens: u.total_tokens,
                        });

                        if !content.is_empty() || done {
                            if sender
                                .send(Ok(StreamChunk {
                                    content,
                                    done,
                                    usage,
                                }))
                                .await
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_metadata() {
        let provider = OpenAiProvider::new();
        assert_eq!(provider.provider_type(), AiProviderType::OpenAi);
        assert_eq!(provider.display_name(), "OpenAI");
        assert!(provider.supports_streaming());
        assert!(provider.supports_tools());
        assert_eq!(provider.default_model(), "gpt-4o-mini");
    }

    #[test]
    fn test_openai_models_list() {
        let provider = OpenAiProvider::new();
        let models = provider.available_models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "gpt-4o"));
        assert!(models.iter().any(|m| m.id == "gpt-4o-mini"));
    }

    #[test]
    fn test_build_url() {
        assert_eq!(
            OpenAiProvider::build_url("https://api.openai.com/v1", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            OpenAiProvider::build_url("https://api.openai.com/v1/", "/models"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn test_message_conversion() {
        let msg = ChatMessage {
            role: MessageRole::System,
            content: "You are helpful.".into(),
        };
        let api_msg: ApiMessage = (&msg).into();
        assert_eq!(api_msg.role, "system");
        assert_eq!(api_msg.content, "You are helpful.");
    }

    #[tokio::test]
    async fn test_validate_config_requires_key() {
        let provider = OpenAiProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: None,
            endpoint: None,
            model: "gpt-4o".into(),
            extra: serde_json::Value::Null,
        };
        let err = provider.validate_config(&config).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn test_complete_requires_init() {
        let provider = OpenAiProvider::new();
        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "gpt-4o".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stop: None,
        };
        let err = provider.complete(&req).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[test]
    fn test_map_api_error_401() {
        let body = r#"{"error":{"message":"Invalid API key","code":"invalid_api_key"}}"#;
        let err = map_api_error(reqwest::StatusCode::UNAUTHORIZED, body);
        assert!(matches!(err, AiError::InvalidApiKey));
    }

    #[test]
    fn test_map_api_error_429() {
        let body = r#"{"error":{"message":"Rate limited","code":"rate_limit_exceeded"}}"#;
        let err = map_api_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body);
        assert!(matches!(err, AiError::RateLimited { .. }));
    }

    #[tokio::test]
    async fn test_reset_clears_state() {
        let provider = OpenAiProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: Some("sk-test".into()),
            endpoint: None,
            model: "gpt-4o".into(),
            extra: serde_json::Value::Null,
        };
        provider.initialize(&config).await.unwrap();

        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "gpt-4o".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stop: None,
        };
        assert!(provider.complete(&req).await.is_err() || provider.complete(&req).await.is_ok());

        provider.reset().await;

        let err = provider.complete(&req).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[test]
    fn test_map_api_error_context_length() {
        let body = r#"{"error":{"message":"Too many tokens","code":"context_length_exceeded"}}"#;
        let err = map_api_error(reqwest::StatusCode::BAD_REQUEST, body);
        assert!(matches!(err, AiError::ContextLengthExceeded { .. }));
    }
}
