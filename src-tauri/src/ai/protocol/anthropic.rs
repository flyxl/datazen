//! Anthropic Messages API protocol (direct HTTP — no mature SDK).
//!
//! Used by both `AnthropicProvider` and `CustomProvider(AnthropicCompatible)`.

use datazen_ai_api::*;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{map_http_error, ProtocolConfig, STREAM_CHUNK_TIMEOUT};

const API_VERSION: &str = "2023-06-01";

// ─── Wire types ───

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

#[derive(Serialize)]
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

#[derive(Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: StreamMsgStart },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { delta: ContentDelta },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MsgDeltaBody,
        usage: ApiUsage,
    },
    #[serde(rename = "message_stop")]
    MessageStop {},
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct StreamMsgStart {
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ContentDelta {
    text: Option<String>,
}

#[derive(Deserialize)]
struct MsgDeltaBody {
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

// ─── Helpers ───

fn build_url(endpoint: &str) -> String {
    let base = endpoint.trim_end_matches('/');
    if base.ends_with("/v1/messages") {
        base.to_string()
    } else {
        format!("{base}/v1/messages")
    }
}

fn split_system(messages: &[ChatMessage]) -> (Option<String>, Vec<ApiMessage>) {
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

// ─── Public API ───

pub async fn complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse, AiError> {
    let url = build_url(&cfg.api_base);
    let (system, messages) = split_system(&request.messages);

    let body = ApiRequest {
        model: request.model.clone(),
        messages,
        max_tokens: cfg.max_tokens,
        system,
        temperature: request.temperature,
        stop_sequences: request.stop.clone(),
        stream: None,
    };

    tracing::info!(%url, "anthropic: request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let resp = cfg
        .http_client
        .post(&url)
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::RequestFailed(e.to_string()))?;

    let status = resp.status();
    let raw = resp.text().await.unwrap_or_default();
    tracing::info!(%status, "anthropic: response\n{}", raw);
    if !status.is_success() {
        return Err(map_http_error(status, &raw));
    }

    let api_resp: ApiResponse =
        serde_json::from_str(&raw).map_err(|e| AiError::RequestFailed(format!("JSON decode: {e}")))?;

    let content = api_resp
        .content
        .iter()
        .filter(|b| b.block_type == "text")
        .filter_map(|b| b.text.as_deref())
        .collect::<Vec<_>>()
        .join("");

    let reasoning_text: String = api_resp
        .content
        .iter()
        .filter(|b| b.block_type == "thinking")
        .filter_map(|b| b.text.as_deref())
        .collect::<Vec<_>>()
        .join("");
    let reasoning = if reasoning_text.is_empty() {
        None
    } else {
        Some(reasoning_text)
    };

    let total = api_resp.usage.input_tokens + api_resp.usage.output_tokens;

    Ok(CompletionResponse {
        request_id: request.request_id.clone(),
        content,
        reasoning,
        model: api_resp.model,
        finish_reason: api_resp.stop_reason,
        usage: TokenUsage {
            prompt_tokens: api_resp.usage.input_tokens,
            completion_tokens: api_resp.usage.output_tokens,
            total_tokens: total,
        },
    })
}

pub async fn stream_complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
    sender: mpsc::Sender<Result<StreamChunk, AiError>>,
) -> Result<(), AiError> {
    let url = build_url(&cfg.api_base);
    let (system, messages) = split_system(&request.messages);

    let body = ApiRequest {
        model: request.model.clone(),
        messages,
        max_tokens: cfg.max_tokens,
        system,
        temperature: request.temperature,
        stop_sequences: request.stop.clone(),
        stream: Some(true),
    };

    tracing::info!(%url, "anthropic: stream request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let resp = cfg
        .http_client
        .post(&url)
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::RequestFailed(e.to_string()))?;

    let status = resp.status();
    tracing::info!(%status, "anthropic: HTTP response received");
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        tracing::error!(%status, "anthropic: stream HTTP error\n{}", text);
        return Err(map_http_error(status, &text));
    }

    let mut byte_buf = Vec::new();
    let mut stream = resp.bytes_stream();
    let mut prompt_tokens = 0u32;
    let mut output_tokens = 0u32;
    let mut chunk_count: u64 = 0;

    loop {
        let maybe = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(m) => m,
            Err(_) => {
                tracing::error!(chunk_count, "anthropic: stream timed out");
                let _ = sender
                    .send(Err(AiError::RequestFailed(format!(
                        "Stream timed out: no data for {}s",
                        STREAM_CHUNK_TIMEOUT.as_secs()
                    ))))
                    .await;
                return Ok(());
            }
        };
        let chunk_result = match maybe {
            Some(r) => r,
            None => break,
        };
        let chunk_bytes = match chunk_result {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("anthropic: stream read error: {}", e);
                let _ = sender.send(Err(AiError::RequestFailed(e.to_string()))).await;
                return Ok(());
            }
        };

        chunk_count += 1;
        if chunk_count == 1 {
            tracing::info!("anthropic: first chunk received ({} bytes)", chunk_bytes.len());
        }

        byte_buf.extend_from_slice(&chunk_bytes);

        while let Some(line_end) = byte_buf.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&byte_buf[..line_end]).trim().to_string();
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
                            if !text.is_empty()
                                && sender
                                    .send(Ok(StreamChunk {
                                        content: text,
                                        reasoning: None,
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
                    StreamEvent::MessageDelta { usage, .. } => {
                        output_tokens = usage.output_tokens;
                    }
                    StreamEvent::MessageStop {} => {
                        tracing::info!(chunk_count, "anthropic: stream complete");
                        let total = prompt_tokens + output_tokens;
                        let _ = sender
                            .send(Ok(StreamChunk {
                                content: String::new(),
                                reasoning: None,
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

    tracing::warn!(chunk_count, "anthropic: stream ended without message_stop");
    let _ = sender
        .send(Ok(StreamChunk { content: String::new(), reasoning: None, done: true, usage: None }))
        .await;

    Ok(())
}

/// Fetch models from Anthropic /v1/models endpoint.
pub async fn fetch_models(cfg: &ProtocolConfig) -> Result<Vec<ModelInfo>, AiError> {
    let base = cfg.api_base.trim_end_matches('/');
    let url = format!("{base}/v1/models");
    tracing::info!(%url, "anthropic: fetch_models");

    let resp = cfg
        .http_client
        .get(&url)
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", API_VERSION)
        .send()
        .await
        .map_err(|e| AiError::RequestFailed(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_http_error(status, &body));
    }

    #[derive(Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelEntry>,
    }
    #[derive(Deserialize)]
    struct ModelEntry {
        id: String,
    }

    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| AiError::RequestFailed(format!("Failed to parse models: {e}")))?;

    Ok(body
        .data
        .into_iter()
        .map(|m| ModelInfo {
            display_name: m.id.clone(),
            id: m.id,
            context_window: 0,
            supports_streaming: true,
            supports_tools: false,
        })
        .collect())
}

/// Lightweight connectivity probe.
pub async fn probe(cfg: &ProtocolConfig, model: &str) -> Result<(), AiError> {
    let url = build_url(&cfg.api_base);
    tracing::info!(%url, %model, "anthropic: probe");
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "."}],
        "max_tokens": 1,
        "stream": false,
        "temperature": 0,
    });
    let resp = cfg
        .http_client
        .post(&url)
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", API_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::RequestFailed(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(map_http_error(status, &text));
    }
    tracing::info!("anthropic: probe success");
    Ok(())
}
