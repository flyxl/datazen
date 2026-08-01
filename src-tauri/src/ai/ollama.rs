//! Ollama (local LLM) AI provider.
//! Uses the Ollama HTTP API which is largely OpenAI-compatible for chat completions.

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

const DEFAULT_ENDPOINT: &str = "http://localhost:11434";

pub struct OllamaProvider {
    client: Client,
    state: RwLock<Option<ProviderState>>,
}

struct ProviderState {
    endpoint: String,
    model: String,
}

impl OllamaProvider {
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

// ─── Ollama API wire types ───

#[derive(Serialize)]
struct ApiChatRequest {
    model: String,
    messages: Vec<ApiMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<ApiOptions>,
}

#[derive(Serialize)]
struct ApiOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiChatResponse {
    message: Option<ApiMessage>,
    model: String,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Deserialize)]
struct ApiTagsResponse {
    models: Vec<ApiModelEntry>,
}

#[derive(Deserialize)]
struct ApiModelEntry {
    name: String,
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

#[async_trait]
impl AiProvider for OllamaProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Ollama
    }

    fn display_name(&self) -> &str {
        "Ollama (Local)"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        // Ollama models are dynamically pulled by the user. Return common defaults.
        vec![
            ModelInfo {
                id: "llama3.1".into(),
                display_name: "Llama 3.1".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: false,
            },
            ModelInfo {
                id: "qwen2.5-coder".into(),
                display_name: "Qwen 2.5 Coder".into(),
                context_window: 32_768,
                supports_streaming: true,
                supports_tools: false,
            },
            ModelInfo {
                id: "deepseek-coder-v2".into(),
                display_name: "DeepSeek Coder V2".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: false,
            },
            ModelInfo {
                id: "codellama".into(),
                display_name: "Code Llama".into(),
                context_window: 16_384,
                supports_streaming: true,
                supports_tools: false,
            },
        ]
    }

    fn default_model(&self) -> &str {
        "llama3.1"
    }

    fn supports_tools(&self) -> bool {
        false
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let endpoint = config
            .endpoint
            .as_deref()
            .unwrap_or(DEFAULT_ENDPOINT);

        let url = Self::build_url(endpoint, "/api/tags");
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                AiError::ProviderNotAvailable(format!(
                    "Cannot connect to Ollama at {endpoint}: {e}"
                ))
            })?;

        if !resp.status().is_success() {
            return Err(AiError::ProviderNotAvailable(format!(
                "Ollama returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let endpoint = config
            .endpoint
            .as_deref()
            .unwrap_or(DEFAULT_ENDPOINT)
            .to_string();

        let model = if config.model.is_empty() {
            self.default_model().to_string()
        } else {
            config.model.clone()
        };

        *self.state.write().await = Some(ProviderState { endpoint, model });
        Ok(())
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let state_guard = self.state.read().await;
        let state = state_guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("Ollama provider not initialized".into()))?;

        let url = Self::build_url(&state.endpoint, "/api/chat");
        tracing::debug!(
            %url, model = %request.model, messages = request.messages.len(),
            "ollama: complete request"
        );

        let options = if request.temperature.is_some()
            || request.max_tokens.is_some()
            || request.stop.is_some()
        {
            Some(ApiOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
                stop: request.stop.clone(),
            })
        } else {
            None
        };

        let body = ApiChatRequest {
            model: request.model.clone(),
            messages: request.messages.iter().map(|m| m.into()).collect(),
            stream: false,
            options,
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::RequestFailed(format!("Ollama HTTP {status}: {text}")));
        }

        let api_resp: ApiChatResponse = resp
            .json()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let content = api_resp
            .message
            .map(|m| m.content)
            .unwrap_or_default();

        let prompt_tokens = api_resp.prompt_eval_count.unwrap_or(0);
        let completion_tokens = api_resp.eval_count.unwrap_or(0);

        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content,
            model: api_resp.model,
            finish_reason: Some("stop".into()),
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
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
            .ok_or_else(|| AiError::NotConfigured("Ollama provider not initialized".into()))?;

        let url = Self::build_url(&state.endpoint, "/api/chat");
        tracing::debug!(
            %url, model = %request.model, messages = request.messages.len(),
            "ollama: stream_complete request"
        );

        let options = if request.temperature.is_some()
            || request.max_tokens.is_some()
            || request.stop.is_some()
        {
            Some(ApiOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
                stop: request.stop.clone(),
            })
        } else {
            None
        };

        let body = ApiChatRequest {
            model: request.model.clone(),
            messages: request.messages.iter().map(|m| m.into()).collect(),
            stream: true,
            options,
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::RequestFailed(format!("Ollama HTTP {status}: {text}")));
        }

        // Ollama streams newline-delimited JSON (not SSE)
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

                if line.is_empty() {
                    continue;
                }

                if let Ok(chunk) = serde_json::from_str::<ApiChatResponse>(&line) {
                    if let Some(pt) = chunk.prompt_eval_count {
                        prompt_tokens = pt;
                    }
                    if let Some(ct) = chunk.eval_count {
                        output_tokens = ct;
                    }

                    let content = chunk
                        .message
                        .map(|m| m.content)
                        .unwrap_or_default();

                    let done = chunk.done;
                    let usage = if done {
                        Some(TokenUsage {
                            prompt_tokens,
                            completion_tokens: output_tokens,
                            total_tokens: prompt_tokens + output_tokens,
                        })
                    } else {
                        None
                    };

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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_metadata() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.provider_type(), AiProviderType::Ollama);
        assert_eq!(provider.display_name(), "Ollama (Local)");
        assert!(provider.supports_streaming());
        assert!(!provider.supports_tools());
        assert_eq!(provider.default_model(), "llama3.1");
    }

    #[test]
    fn test_ollama_no_api_key_required() {
        let provider = OllamaProvider::new();
        let models = provider.available_models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "llama3.1"));
    }

    #[tokio::test]
    async fn test_complete_requires_init() {
        let provider = OllamaProvider::new();
        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "llama3.1".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stop: None,
        };
        let err = provider.complete(&req).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[test]
    fn test_build_url() {
        assert_eq!(
            OllamaProvider::build_url("http://localhost:11434", "/api/chat"),
            "http://localhost:11434/api/chat"
        );
    }

    #[test]
    fn test_message_conversion() {
        let msg = ChatMessage {
            role: MessageRole::User,
            content: "Hello".into(),
        };
        let api_msg: ApiMessage = (&msg).into();
        assert_eq!(api_msg.role, "user");
        assert_eq!(api_msg.content, "Hello");
    }
}
