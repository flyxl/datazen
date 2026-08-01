//! Anthropic (Claude) AI provider.

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    client: Client,
    state: RwLock<Option<ProviderState>>,
}

struct ProviderState {
    api_key: String,
    endpoint: String,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            state: RwLock::new(None),
        }
    }

    fn build_url(endpoint: &str, path: &str) -> String {
        let base = endpoint.trim_end_matches('/');
        if base.ends_with(path.trim_start_matches('/').trim_end_matches('/')) {
            return base.to_string();
        }
        format!("{base}{path}")
    }
}

// ─── Anthropic Messages API wire types ───

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
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
    content: Vec<ContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: ApiUsage,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// SSE event types for streaming
#[derive(Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: StreamMessageStart },
    #[serde(rename = "content_block_start")]
    ContentBlockStart { content_block: ContentBlock },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { delta: ContentDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {},
    #[serde(rename = "message_delta")]
    MessageDelta { delta: MessageDeltaBody, usage: ApiUsage },
    #[serde(rename = "message_stop")]
    MessageStop {},
    #[serde(rename = "ping")]
    Ping {},
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
struct StreamMessageStart {
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ContentDelta {
    #[serde(rename = "type")]
    delta_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct MessageDeltaBody {
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
}

fn map_api_error(status: reqwest::StatusCode, body: &str) -> AiError {
    if let Ok(err_resp) = serde_json::from_str::<ApiErrorResponse>(body) {
        let msg = err_resp.error.message;
        return match status.as_u16() {
            401 => AiError::InvalidApiKey,
            429 => AiError::RateLimited {
                retry_after_secs: 60,
            },
            _ if err_resp.error.error_type == "invalid_request_error"
                && msg.contains("token") =>
            {
                AiError::ContextLengthExceeded { used: 0, limit: 0 }
            }
            _ => AiError::RequestFailed(msg),
        };
    }
    AiError::RequestFailed(format!("HTTP {status}: {body}"))
}

/// Extracts system message content and non-system messages for the Anthropic API,
/// which uses a separate `system` parameter.
fn split_system_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<ApiMessage>) {
    let system_parts: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .collect();

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    let api_messages = messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|m| ApiMessage {
            role: match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                _ => unreachable!(),
            }
            .into(),
            content: m.content.clone(),
        })
        .collect();

    (system, api_messages)
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Anthropic
    }

    fn display_name(&self) -> &str {
        "Anthropic (Claude)"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-sonnet-4-20250514".into(),
                display_name: "Claude Sonnet 4".into(),
                context_window: 200_000,
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".into(),
                display_name: "Claude 3.5 Haiku".into(),
                context_window: 200_000,
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".into(),
                display_name: "Claude 3.5 Sonnet".into(),
                context_window: 200_000,
                supports_streaming: true,
                supports_tools: true,
            },
        ]
    }

    fn default_model(&self) -> &str {
        "claude-sonnet-4-20250514"
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                AiError::NotConfigured("API key is required for Anthropic".into())
            })?;
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let api_key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                AiError::NotConfigured("API key is required for Anthropic".into())
            })?;

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
            .ok_or_else(|| AiError::NotConfigured("Anthropic provider not initialized".into()))?;

        let url = Self::build_url(&state.endpoint, "/v1/messages");
        let (system, messages) = split_system_messages(&request.messages);

        let body = ApiRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system,
            temperature: request.temperature,
            stop_sequences: request.stop.clone(),
            stream: None,
        };

        let raw_request = serde_json::to_string(&body).unwrap_or_default();
        tracing::info!(%url, "anthropic: HTTP request body\n{}", raw_request);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &state.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        let raw_resp = resp.text().await.unwrap_or_default();
        tracing::info!(%status, "anthropic: HTTP response body\n{}", raw_resp);
        if !status.is_success() {
            return Err(map_api_error(status, &raw_resp));
        }

        let api_resp: ApiResponse = serde_json::from_str(&raw_resp)
            .map_err(|e| AiError::RequestFailed(format!("JSON decode: {e}")))?;

        let content = api_resp
            .content
            .iter()
            .filter(|b| b.block_type == "text")
            .filter_map(|b| b.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        let total = api_resp.usage.input_tokens + api_resp.usage.output_tokens;

        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content,
            model: api_resp.model,
            finish_reason: api_resp.stop_reason,
            usage: TokenUsage {
                prompt_tokens: api_resp.usage.input_tokens,
                completion_tokens: api_resp.usage.output_tokens,
                total_tokens: total,
            },
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
            .ok_or_else(|| AiError::NotConfigured("Anthropic provider not initialized".into()))?;

        let url = Self::build_url(&state.endpoint, "/v1/messages");
        let (system, messages) = split_system_messages(&request.messages);
        let body = ApiRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system,
            temperature: request.temperature,
            stop_sequences: request.stop.clone(),
            stream: Some(true),
        };

        let raw_request = serde_json::to_string(&body).unwrap_or_default();
        tracing::info!(%url, "anthropic: stream HTTP request body\n{}", raw_request);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &state.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
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
        let mut prompt_tokens = 0u32;
        let mut output_tokens = 0u32;

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

            while let Some(line_end) = byte_buf.iter().position(|&b| b == b'\n') {
                let line = String::from_utf8_lossy(&byte_buf[..line_end])
                    .trim()
                    .to_string();
                byte_buf.drain(..=line_end);

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                let data = match line.strip_prefix("data: ") {
                    Some(d) => d.trim(),
                    None if line.starts_with("event:") => continue,
                    None => continue,
                };

                if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                    match event {
                        StreamEvent::MessageStart { message } => {
                            if let Some(usage) = message.usage {
                                prompt_tokens = usage.input_tokens;
                            }
                        }
                        StreamEvent::ContentBlockDelta { delta } => {
                            if let Some(text) = delta.text {
                                if !text.is_empty() {
                                    if sender
                                        .send(Ok(StreamChunk {
                                            content: text,
                                            done: false,
                                            usage: None,
                                        }))
                                        .await
                                        .is_err()
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                        }
                        StreamEvent::MessageDelta { usage, .. } => {
                            output_tokens = usage.output_tokens;
                        }
                        StreamEvent::MessageStop {} => {
                            let total = prompt_tokens + output_tokens;
                            let _ = sender
                                .send(Ok(StreamChunk {
                                    content: String::new(),
                                    done: true,
                                    usage: Some(TokenUsage {
                                        prompt_tokens,
                                        completion_tokens: output_tokens,
                                        total_tokens: total,
                                    }),
                                }))
                                .await;
                            return Ok(());
                        }
                        _ => {}
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
    fn test_anthropic_provider_metadata() {
        let provider = AnthropicProvider::new();
        assert_eq!(provider.provider_type(), AiProviderType::Anthropic);
        assert_eq!(provider.display_name(), "Anthropic (Claude)");
        assert!(provider.supports_streaming());
        assert!(provider.supports_tools());
    }

    #[test]
    fn test_split_system_messages() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: "You are helpful.".into(),
            },
            ChatMessage {
                role: MessageRole::User,
                content: "Hello".into(),
            },
        ];
        let (system, api_msgs) = split_system_messages(&messages);
        assert_eq!(system.unwrap(), "You are helpful.");
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0].role, "user");
    }

    #[test]
    fn test_split_system_messages_none() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "Hello".into(),
        }];
        let (system, api_msgs) = split_system_messages(&messages);
        assert!(system.is_none());
        assert_eq!(api_msgs.len(), 1);
    }

    #[tokio::test]
    async fn test_validate_config_requires_key() {
        let provider = AnthropicProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::Anthropic,
            api_key: None,
            endpoint: None,
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        let err = provider.validate_config(&config).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[test]
    fn test_map_api_error_401() {
        let body = r#"{"error":{"message":"Invalid API key","type":"authentication_error"}}"#;
        let err = map_api_error(reqwest::StatusCode::UNAUTHORIZED, body);
        assert!(matches!(err, AiError::InvalidApiKey));
    }
}
