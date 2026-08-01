//! Custom AI provider — supports OpenAI-compatible and Anthropic-compatible protocols.
//!
//! Users configure a protocol, base URL, API key, and model via the Settings UI.
//! The provider delegates to the appropriate protocol handler at runtime.

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomProtocol {
    OpenAiCompatible,
    OpenAiResponses,
    AnthropicCompatible,
}

pub struct CustomProvider {
    client: Client,
    state: RwLock<Option<CustomState>>,
}

struct CustomState {
    api_key: String,
    endpoint: String,
    protocol: CustomProtocol,
}

impl CustomProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            state: RwLock::new(None),
        }
    }

    fn parse_protocol(config: &AiProviderConfig) -> CustomProtocol {
        config
            .extra
            .get("protocol")
            .and_then(|v| serde_json::from_value::<CustomProtocol>(v.clone()).ok())
            .unwrap_or(CustomProtocol::OpenAiCompatible)
    }

    fn build_url(endpoint: &str, path: &str) -> String {
        let base = endpoint.trim_end_matches('/');
        if base.ends_with(path.trim_start_matches('/').trim_end_matches('/')) {
            return base.to_string();
        }
        format!("{base}{path}")
    }
}

// ─── Shared wire types (OpenAI-compatible) ───

#[derive(Serialize)]
struct OaiRequest {
    model: String,
    messages: Vec<OaiMessage>,
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
struct OaiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OaiResponse {
    choices: Vec<OaiChoice>,
    usage: Option<OaiUsage>,
    model: String,
}

#[derive(Deserialize)]
struct OaiChoice {
    message: Option<OaiMessage>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OaiStreamChunk {
    choices: Vec<OaiStreamChoice>,
    #[serde(default)]
    usage: Option<OaiUsage>,
}

#[derive(Deserialize)]
struct OaiStreamChoice {
    delta: Option<OaiDelta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OaiDelta {
    content: Option<String>,
}

// ─── Anthropic-compatible wire types ───

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<OaiMessage>,
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

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: AnthropicMsgStart },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { delta: AnthropicContentDelta },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: AnthropicMsgDeltaBody, usage: AnthropicUsage },
    #[serde(rename = "message_stop")]
    MessageStop {},
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct AnthropicMsgStart {
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct AnthropicContentDelta {
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicMsgDeltaBody {
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

// ─── OpenAI Responses API wire types ───

#[derive(Serialize)]
struct OaiResponsesRequest {
    model: String,
    input: Vec<OaiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    store: Option<bool>,
}

#[derive(Deserialize)]
struct OaiResponsesResponse {
    output: Vec<OaiResponsesOutputItem>,
    model: String,
    usage: Option<OaiResponsesUsage>,
}

#[derive(Deserialize)]
struct OaiResponsesOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    content: Option<Vec<OaiResponsesContentBlock>>,
}

#[derive(Deserialize)]
struct OaiResponsesContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct OaiResponsesUsage {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OaiResponsesStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    response: Option<OaiResponsesStreamResponse>,
}

#[derive(Deserialize)]
struct OaiResponsesStreamResponse {
    usage: Option<OaiResponsesUsage>,
}

// ─── Models listing types ───

#[derive(Deserialize)]
struct OaiModelsResponse {
    data: Vec<OaiModelEntry>,
}

#[derive(Deserialize)]
struct OaiModelEntry {
    id: String,
}

impl From<&ChatMessage> for OaiMessage {
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

fn split_system_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<OaiMessage>) {
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
        .map(|m| OaiMessage {
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

fn split_instructions(messages: &[ChatMessage]) -> (Option<String>, Vec<OaiMessage>) {
    let system_parts: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .collect();

    let instructions = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    let input = messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|m| OaiMessage {
            role: match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                _ => unreachable!(),
            }
            .into(),
            content: m.content.clone(),
        })
        .collect();

    (instructions, input)
}

fn map_oai_error(status: reqwest::StatusCode, body: &str) -> AiError {
    match status.as_u16() {
        401 => AiError::InvalidApiKey,
        429 => AiError::RateLimited { retry_after_secs: 60 },
        _ => AiError::RequestFailed(format!("HTTP {status}: {}", &body[..body.len().min(500)])),
    }
}

// ─── Public API: fetch models from any provider ───

pub async fn fetch_remote_models(
    protocol: CustomProtocol,
    endpoint: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>, AiError> {
    tracing::info!(protocol = ?protocol, %endpoint, "fetch_remote_models: start");
    let client = Client::new();
    let base = endpoint.trim_end_matches('/');

    match protocol {
        CustomProtocol::OpenAiCompatible | CustomProtocol::OpenAiResponses => {
            let url = format!("{base}/models");
            let resp = client
                .get(&url)
                .bearer_auth(api_key)
                .send()
                .await
                .map_err(|e| AiError::RequestFailed(e.to_string()))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(map_oai_error(status, &body));
            }

            let body: OaiModelsResponse = resp
                .json()
                .await
                .map_err(|e| AiError::RequestFailed(format!("Failed to parse models response: {e}")))?;

            let models = body
                .data
                .into_iter()
                .map(|m| ModelInfo {
                    display_name: m.id.clone(),
                    id: m.id,
                    context_window: 0,
                    supports_streaming: true,
                    supports_tools: false,
                })
                .collect();

            Ok(models)
        }
        CustomProtocol::AnthropicCompatible => {
            let url = format!("{base}/v1/models");
            let resp = client
                .get(&url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await
                .map_err(|e| AiError::RequestFailed(e.to_string()))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(map_oai_error(status, &body));
            }

            // Anthropic /v1/models returns {"data": [{"id": "...", "type": "model", ...}]}
            let body: OaiModelsResponse = resp
                .json()
                .await
                .map_err(|e| AiError::RequestFailed(format!("Failed to parse models response: {e}")))?;

            let models = body
                .data
                .into_iter()
                .map(|m| ModelInfo {
                    display_name: m.id.clone(),
                    id: m.id,
                    context_window: 0,
                    supports_streaming: true,
                    supports_tools: false,
                })
                .collect();

            Ok(models)
        }
    }
}

// ─── AiProvider implementation ───

#[async_trait]
impl AiProvider for CustomProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Custom
    }

    fn display_name(&self) -> &str {
        "Custom"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![]
    }

    fn default_model(&self) -> &str {
        ""
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required".into()))?;

        let endpoint = config
            .endpoint
            .as_deref()
            .filter(|e| !e.is_empty())
            .ok_or_else(|| AiError::NotConfigured("Endpoint URL is required".into()))?;

        if config.model.is_empty() {
            return Err(AiError::NotConfigured("Model is required".into()));
        }

        let protocol = Self::parse_protocol(config);
        fetch_remote_models(protocol, endpoint, key).await?;
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let api_key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required".into()))?;

        let endpoint = config
            .endpoint
            .as_deref()
            .filter(|e| !e.is_empty())
            .ok_or_else(|| AiError::NotConfigured("Endpoint URL is required".into()))?;

        let protocol = Self::parse_protocol(config);

        *self.state.write().await = Some(CustomState {
            api_key: api_key.to_string(),
            endpoint: endpoint.to_string(),
            protocol,
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
            .ok_or_else(|| AiError::NotConfigured("Custom provider not initialized".into()))?;

        match state.protocol {
            CustomProtocol::OpenAiCompatible => {
                self.complete_openai(state, request).await
            }
            CustomProtocol::OpenAiResponses => {
                self.complete_responses(state, request).await
            }
            CustomProtocol::AnthropicCompatible => {
                self.complete_anthropic(state, request).await
            }
        }
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let state_guard = self.state.read().await;
        let state = state_guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("Custom provider not initialized".into()))?;

        match state.protocol {
            CustomProtocol::OpenAiCompatible => {
                self.stream_openai(state, request, sender).await
            }
            CustomProtocol::OpenAiResponses => {
                self.stream_responses(state, request, sender).await
            }
            CustomProtocol::AnthropicCompatible => {
                self.stream_anthropic(state, request, sender).await
            }
        }
    }

    async fn reset(&self) {
        *self.state.write().await = None;
    }
}

// ─── Protocol-specific implementations ───

impl CustomProvider {
    async fn complete_openai(
        &self,
        state: &CustomState,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let url = Self::build_url(&state.endpoint, "/chat/completions");
        tracing::debug!(
            %url, model = %request.model, messages = request.messages.len(),
            protocol = "openai_chat", "custom: complete request"
        );

        let body = OaiRequest {
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
        let raw_body = resp.text().await.unwrap_or_default();
        tracing::info!(
            %status,
            body_len = raw_body.len(),
            body_preview = %&raw_body[..raw_body.len().min(500)],
            "custom(openai_chat): raw HTTP response"
        );

        if !status.is_success() {
            return Err(map_oai_error(status, &raw_body));
        }

        let api_resp: OaiResponse = serde_json::from_str(&raw_body)
            .map_err(|e| AiError::RequestFailed(format!("JSON decode: {e}")))?;

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

        tracing::debug!(
            response_len = content.len(),
            finish_reason = ?choice.finish_reason,
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            "custom(openai_chat): complete response"
        );

        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content,
            model: api_resp.model,
            finish_reason: choice.finish_reason.clone(),
            usage,
        })
    }

    async fn complete_responses(
        &self,
        state: &CustomState,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let url = Self::build_url(&state.endpoint, "/responses");
        let (instructions, input) = split_instructions(&request.messages);
        tracing::debug!(
            %url, model = %request.model, input_count = input.len(),
            has_instructions = instructions.is_some(),
            protocol = "openai_responses", "custom: complete request"
        );

        let body = OaiResponsesRequest {
            model: request.model.clone(),
            input,
            instructions,
            temperature: request.temperature,
            max_output_tokens: request.max_tokens,
            stream: Some(false),
            store: Some(false),
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
            return Err(map_oai_error(status, &text));
        }

        let api_resp: OaiResponsesResponse = resp
            .json()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let content = api_resp
            .output
            .iter()
            .filter(|item| item.item_type == "message")
            .flat_map(|item| item.content.iter().flatten())
            .filter(|block| block.block_type == "output_text")
            .filter_map(|block| block.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        let usage = api_resp
            .usage
            .map(|u| TokenUsage {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.total_tokens,
            })
            .unwrap_or_default();

        tracing::debug!(
            response_len = content.len(),
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            "custom(openai_responses): complete response"
        );

        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content,
            model: api_resp.model,
            finish_reason: Some("stop".into()),
            usage,
        })
    }

    async fn complete_anthropic(
        &self,
        state: &CustomState,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let url = Self::build_url(&state.endpoint, "/v1/messages");
        let (system, messages) = split_system_messages(&request.messages);
        tracing::debug!(
            %url, model = %request.model, messages = messages.len(),
            has_system = system.is_some(),
            protocol = "anthropic", "custom: complete request"
        );

        let body = AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system,
            temperature: request.temperature,
            stop_sequences: request.stop.clone(),
            stream: None,
        };

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &state.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_oai_error(status, &text));
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let content = api_resp
            .content
            .iter()
            .filter(|b| b.block_type == "text")
            .filter_map(|b| b.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        let total = api_resp.usage.input_tokens + api_resp.usage.output_tokens;
        tracing::debug!(
            response_len = content.len(),
            input_tokens = api_resp.usage.input_tokens,
            output_tokens = api_resp.usage.output_tokens,
            "custom(anthropic): complete response"
        );

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

    async fn stream_openai(
        &self,
        state: &CustomState,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let url = Self::build_url(&state.endpoint, "/chat/completions");
        tracing::debug!(%url, model = %request.model, protocol = "openai_chat", "custom: stream request");

        let body = OaiRequest {
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
            return Err(map_oai_error(status, &text));
        }

        let mut byte_buf = Vec::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk_result) = futures_util::StreamExt::next(&mut stream).await {
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

                if let Ok(chunk) = serde_json::from_str::<OaiStreamChunk>(data) {
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
                                .send(Ok(StreamChunk { content, done, usage }))
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

    async fn stream_responses(
        &self,
        state: &CustomState,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let url = Self::build_url(&state.endpoint, "/responses");
        let (instructions, input) = split_instructions(&request.messages);
        tracing::debug!(%url, model = %request.model, protocol = "openai_responses", "custom: stream request");

        let body = OaiResponsesRequest {
            model: request.model.clone(),
            input,
            instructions,
            temperature: request.temperature,
            max_output_tokens: request.max_tokens,
            stream: Some(true),
            store: Some(false),
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
            return Err(map_oai_error(status, &text));
        }

        let mut byte_buf = Vec::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk_result) = futures_util::StreamExt::next(&mut stream).await {
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

                let data = if let Some(d) = line.strip_prefix("data: ") {
                    d.trim()
                } else {
                    continue;
                };

                if let Ok(event) = serde_json::from_str::<OaiResponsesStreamEvent>(data) {
                    match event.event_type.as_str() {
                        "response.output_text.delta" => {
                            if let Some(text) = event.delta {
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
                        "response.completed" => {
                            let usage = event
                                .response
                                .and_then(|r| r.usage)
                                .map(|u| TokenUsage {
                                    prompt_tokens: u.input_tokens,
                                    completion_tokens: u.output_tokens,
                                    total_tokens: u.total_tokens,
                                });
                            let _ = sender
                                .send(Ok(StreamChunk {
                                    content: String::new(),
                                    done: true,
                                    usage,
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

    async fn stream_anthropic(
        &self,
        state: &CustomState,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let url = Self::build_url(&state.endpoint, "/v1/messages");
        let (system, messages) = split_system_messages(&request.messages);
        tracing::debug!(%url, model = %request.model, protocol = "anthropic", "custom: stream request");

        let body = AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system,
            temperature: request.temperature,
            stop_sequences: request.stop.clone(),
            stream: Some(true),
        };

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &state.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_oai_error(status, &text));
        }

        let mut byte_buf = Vec::new();
        let mut stream = resp.bytes_stream();
        let mut prompt_tokens = 0u32;
        let mut output_tokens = 0u32;

        while let Some(chunk_result) = futures_util::StreamExt::next(&mut stream).await {
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

                if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                    match event {
                        AnthropicStreamEvent::MessageStart { message } => {
                            if let Some(usage) = message.usage {
                                prompt_tokens = usage.input_tokens;
                            }
                        }
                        AnthropicStreamEvent::ContentBlockDelta { delta } => {
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
                        AnthropicStreamEvent::MessageDelta { usage, .. } => {
                            output_tokens = usage.output_tokens;
                        }
                        AnthropicStreamEvent::MessageStop {} => {
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
    fn test_custom_provider_metadata() {
        let provider = CustomProvider::new();
        assert_eq!(provider.provider_type(), AiProviderType::Custom);
        assert_eq!(provider.display_name(), "Custom");
        assert!(provider.supports_streaming());
        assert!(provider.available_models().is_empty());
    }

    #[test]
    fn test_parse_protocol_default() {
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            extra: serde_json::Value::Null,
        };
        assert_eq!(
            CustomProvider::parse_protocol(&config),
            CustomProtocol::OpenAiCompatible
        );
    }

    #[test]
    fn test_parse_protocol_openai() {
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            extra: serde_json::json!({ "protocol": "open_ai_compatible" }),
        };
        assert_eq!(
            CustomProvider::parse_protocol(&config),
            CustomProtocol::OpenAiCompatible
        );
    }

    #[test]
    fn test_parse_protocol_anthropic() {
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            extra: serde_json::json!({ "protocol": "anthropic_compatible" }),
        };
        assert_eq!(
            CustomProvider::parse_protocol(&config),
            CustomProtocol::AnthropicCompatible
        );
    }

    #[tokio::test]
    async fn test_validate_requires_fields() {
        let provider = CustomProvider::new();

        let no_key = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: None,
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            extra: serde_json::Value::Null,
        };
        assert!(matches!(
            provider.validate_config(&no_key).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));

        let no_endpoint = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: None,
            model: "model".into(),
            extra: serde_json::Value::Null,
        };
        assert!(matches!(
            provider.validate_config(&no_endpoint).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));

        let no_model = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: String::new(),
            extra: serde_json::Value::Null,
        };
        assert!(matches!(
            provider.validate_config(&no_model).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));
    }

    #[tokio::test]
    async fn test_complete_requires_init() {
        let provider = CustomProvider::new();
        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "model".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stop: None,
        };
        assert!(matches!(
            provider.complete(&req).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));
    }

    #[tokio::test]
    async fn test_reset_clears_state() {
        let provider = CustomProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            extra: serde_json::json!({ "protocol": "open_ai_compatible" }),
        };
        provider.initialize(&config).await.unwrap();
        provider.reset().await;

        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "model".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stop: None,
        };
        assert!(matches!(
            provider.complete(&req).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));
    }

    #[test]
    fn test_protocol_serde() {
        let json = serde_json::to_string(&CustomProtocol::OpenAiCompatible).unwrap();
        assert_eq!(json, "\"open_ai_compatible\"");

        let json = serde_json::to_string(&CustomProtocol::OpenAiResponses).unwrap();
        assert_eq!(json, "\"open_ai_responses\"");

        let json = serde_json::to_string(&CustomProtocol::AnthropicCompatible).unwrap();
        assert_eq!(json, "\"anthropic_compatible\"");

        let p: CustomProtocol = serde_json::from_str("\"open_ai_compatible\"").unwrap();
        assert_eq!(p, CustomProtocol::OpenAiCompatible);

        let p: CustomProtocol = serde_json::from_str("\"open_ai_responses\"").unwrap();
        assert_eq!(p, CustomProtocol::OpenAiResponses);

        let p: CustomProtocol = serde_json::from_str("\"anthropic_compatible\"").unwrap();
        assert_eq!(p, CustomProtocol::AnthropicCompatible);
    }
}
