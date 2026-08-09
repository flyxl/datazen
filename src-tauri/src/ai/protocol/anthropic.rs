//! Anthropic Messages API protocol (direct HTTP).
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
    messages: Vec<serde_json::Value>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
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
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
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
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockMeta,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: ContentDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
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
struct ContentBlockMeta {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(other)]
    Other,
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

fn build_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<serde_json::Value>) {
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

    let api_messages: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|m| match m.role {
            MessageRole::User => serde_json::json!({
                "role": "user",
                "content": m.content,
            }),
            MessageRole::Assistant => {
                let has_tool_calls = m.tool_calls.as_ref().is_some_and(|t| !t.is_empty());
                let has_reasoning = m.reasoning.as_ref().is_some_and(|r| !r.is_empty());

                if has_tool_calls || has_reasoning {
                    let mut blocks = Vec::new();
                    if let Some(reasoning) = &m.reasoning {
                        if !reasoning.is_empty() {
                            blocks.push(serde_json::json!({
                                "type": "thinking",
                                "thinking": reasoning,
                            }));
                        }
                    }
                    if !m.content.is_empty() {
                        blocks.push(serde_json::json!({
                            "type": "text",
                            "text": m.content,
                        }));
                    }
                    if let Some(tool_calls) = &m.tool_calls {
                        for tc in tool_calls {
                            let input: serde_json::Value =
                                serde_json::from_str(&tc.arguments).unwrap_or(serde_json::json!({}));
                            blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": input,
                            }));
                        }
                    }
                    serde_json::json!({
                        "role": "assistant",
                        "content": blocks,
                    })
                } else {
                    serde_json::json!({
                        "role": "assistant",
                        "content": m.content,
                    })
                }
            }
            MessageRole::Tool => serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": m.tool_call_id,
                    "content": m.content,
                }],
            }),
            _ => serde_json::json!({
                "role": "user",
                "content": m.content,
            }),
        })
        .collect();

    (system, api_messages)
}

fn to_anthropic_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        })
        .collect()
}

fn parse_tool_use_blocks(content: &[ContentBlock]) -> Option<Vec<ToolCall>> {
    let calls: Vec<ToolCall> = content
        .iter()
        .filter(|b| b.block_type == "tool_use")
        .filter_map(|b| {
            Some(ToolCall {
                id: b.id.clone()?,
                name: b.name.clone()?,
                arguments: b
                    .input
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_default())
                    .unwrap_or_else(|| "{}".into()),
            })
        })
        .collect();
    if calls.is_empty() { None } else { Some(calls) }
}

// ─── Public API ───

pub async fn complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse, AiError> {
    let url = build_url(&cfg.api_base);
    let (system, messages) = build_messages(&request.messages);

    let tools = request.tools.as_ref().map(|t| to_anthropic_tools(t));

    let body = ApiRequest {
        model: request.model.clone(),
        messages,
        max_tokens: cfg.max_tokens,
        system,
        temperature: request.temperature,
        stop_sequences: request.stop.clone(),
        stream: None,
        tools,
    };

    tracing::debug!(%url, "anthropic: request\n{}", serde_json::to_string(&body).unwrap_or_default());

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
    tracing::debug!(%status, "anthropic: response\n{}", raw);
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
    let reasoning = if reasoning_text.is_empty() { None } else { Some(reasoning_text) };

    let tool_calls = parse_tool_use_blocks(&api_resp.content);
    let finish_reason = if tool_calls.is_some() {
        Some("tool_calls".into())
    } else {
        api_resp.stop_reason
    };

    let total = api_resp.usage.input_tokens + api_resp.usage.output_tokens;

    Ok(CompletionResponse {
        request_id: request.request_id.clone(),
        content,
        reasoning,
        model: api_resp.model,
        finish_reason,
        usage: TokenUsage {
            prompt_tokens: api_resp.usage.input_tokens,
            completion_tokens: api_resp.usage.output_tokens,
            total_tokens: total,
        },
        tool_calls,
        response_id: None,
    })
}

pub async fn stream_complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
    sender: mpsc::Sender<Result<StreamChunk, AiError>>,
) -> Result<(), AiError> {
    let url = build_url(&cfg.api_base);
    let (system, messages) = build_messages(&request.messages);

    let tools = request.tools.as_ref().map(|t| to_anthropic_tools(t));

    let body = ApiRequest {
        model: request.model.clone(),
        messages,
        max_tokens: cfg.max_tokens,
        system,
        temperature: request.temperature,
        stop_sequences: request.stop.clone(),
        stream: Some(true),
        tools,
    };

    tracing::debug!(%url, "anthropic: stream request\n{}", serde_json::to_string(&body).unwrap_or_default());

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

    // Tool call accumulation
    struct PendingToolUse {
        id: String,
        name: String,
        args: String,
    }
    let mut pending_tool_uses: Vec<PendingToolUse> = Vec::new();
    let mut _current_block_type: Option<String> = None;

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
                    StreamEvent::ContentBlockStart { content_block, .. } => {
                        _current_block_type = Some(content_block.block_type.clone());
                        if content_block.block_type == "tool_use" {
                            pending_tool_uses.push(PendingToolUse {
                                id: content_block.id.unwrap_or_default(),
                                name: content_block.name.unwrap_or_default(),
                                args: String::new(),
                            });
                        }
                    }
                    StreamEvent::ContentBlockDelta { delta, .. } => match delta {
                        ContentDelta::TextDelta { text } => {
                            if !text.is_empty()
                                && sender
                                    .send(Ok(StreamChunk {
                                        content: text,
                                        reasoning: None,
                                        done: false,
                                        usage: None,
                                        tool_calls: None,
                                        response_id: None,
                                    }))
                                    .await
                                    .is_err()
                            {
                                return Ok(());
                            }
                        }
                        ContentDelta::ThinkingDelta { thinking } => {
                            if !thinking.is_empty()
                                && sender
                                    .send(Ok(StreamChunk {
                                        content: String::new(),
                                        reasoning: Some(thinking),
                                        done: false,
                                        usage: None,
                                        tool_calls: None,
                                        response_id: None,
                                    }))
                                    .await
                                    .is_err()
                            {
                                return Ok(());
                            }
                        }
                        ContentDelta::InputJsonDelta { partial_json } => {
                            if let Some(last) = pending_tool_uses.last_mut() {
                                last.args.push_str(&partial_json);
                            }
                        }
                        ContentDelta::Other => {}
                    },
                    StreamEvent::ContentBlockStop { .. } => {
                        _current_block_type = None;
                    }
                    StreamEvent::MessageDelta { usage, .. } => {
                        output_tokens = usage.output_tokens;
                    }
                    StreamEvent::MessageStop {} => {
                        tracing::info!(chunk_count, "anthropic: stream complete");
                        let total = prompt_tokens + output_tokens;
                        let tool_calls = if pending_tool_uses.is_empty() {
                            None
                        } else {
                            Some(
                                pending_tool_uses
                                    .drain(..)
                                    .map(|t| ToolCall {
                                        id: t.id,
                                        name: t.name,
                                        arguments: t.args,
                                    })
                                    .collect(),
                            )
                        };
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
                                tool_calls,
                                response_id: None,
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
    let tool_calls = if pending_tool_uses.is_empty() {
        None
    } else {
        Some(
            pending_tool_uses
                .into_iter()
                .map(|t| ToolCall {
                    id: t.id,
                    name: t.name,
                    arguments: t.args,
                })
                .collect(),
        )
    };
    let _ = sender
        .send(Ok(StreamChunk {
            content: String::new(),
            reasoning: None,
            done: true,
            usage: None,
            tool_calls,
            response_id: None,
        }))
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
