//! Anthropic Messages API protocol (direct HTTP).
//!
//! Used by both `AnthropicProvider` and `CustomProvider(AnthropicCompatible)`.

use datazen_ai_api::*;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    log_http_error, log_request_metadata, log_response_metadata, map_http_error, ProtocolConfig,
    STREAM_CHUNK_TIMEOUT,
};

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
                            let input: serde_json::Value = serde_json::from_str(&tc.arguments)
                                .unwrap_or(serde_json::json!({}));
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
    if calls.is_empty() {
        None
    } else {
        Some(calls)
    }
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

    log_request_metadata("anthropic", request, &body, false);

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
    log_response_metadata("anthropic", &request.request_id, status, raw.len());
    if !status.is_success() {
        return Err(map_http_error(status, &raw));
    }

    let api_resp: ApiResponse = serde_json::from_str(&raw)
        .map_err(|e| AiError::RequestFailed(format!("JSON decode: {e}")))?;

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

    log_request_metadata("anthropic", request, &body, true);

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
    tracing::info!(
        request_id = %request.request_id,
        %status,
        "anthropic: HTTP response received"
    );
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        log_http_error("anthropic", &request.request_id, status, &text);
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
                tracing::error!(
                    request_id = %request.request_id,
                    "anthropic: stream read failed"
                );
                let _ = sender
                    .send(Err(AiError::RequestFailed(e.to_string())))
                    .await;
                return Ok(());
            }
        };

        chunk_count += 1;
        if chunk_count == 1 {
            tracing::info!(
                "anthropic: first chunk received ({} bytes)",
                chunk_bytes.len()
            );
        }

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
    tracing::info!("anthropic: fetch_models");

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
    tracing::info!(%model, "anthropic: probe");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::protocol::test_support::{
        collect_stream, protocol_config_anthropic, sample_request,
    };
    use datazen_ai_api::AiError;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn build_url_appends_messages_path() {
        assert_eq!(
            build_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_url("https://api.anthropic.com/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_url("https://api.anthropic.com/"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn build_messages_splits_system_and_roles() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: "You are helpful".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: "Hello".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: "Hi there".into(),
                reasoning: Some("thinking".into()),
                tool_calls: Some(vec![ToolCall {
                    id: "tu_1".into(),
                    name: "lookup".into(),
                    arguments: r#"{"q":"x"}"#.into(),
                }]),
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::Tool,
                content: "result".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: Some("tu_1".into()),
            },
        ];
        let (system, api_messages) = build_messages(&messages);
        assert_eq!(system.as_deref(), Some("You are helpful"));
        assert_eq!(api_messages.len(), 3);
        assert_eq!(api_messages[0]["role"], "user");
        assert_eq!(api_messages[1]["role"], "assistant");
        assert!(api_messages[1]["content"].is_array());
        assert_eq!(api_messages[2]["role"], "user");
        assert_eq!(api_messages[2]["content"][0]["type"], "tool_result");
    }

    #[test]
    fn parse_tool_use_blocks_extracts_calls() {
        let blocks = vec![
            ContentBlock {
                block_type: "text".into(),
                text: Some("done".into()),
                id: None,
                name: None,
                input: None,
            },
            ContentBlock {
                block_type: "tool_use".into(),
                text: None,
                id: Some("id1".into()),
                name: Some("fn".into()),
                input: Some(serde_json::json!({"a": 1})),
            },
        ];
        let calls = parse_tool_use_blocks(&blocks).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "fn");
        assert!(calls[0].arguments.contains("\"a\":1"));
    }

    #[tokio::test]
    async fn complete_success_parses_text_tools_and_reasoning() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [
                    {"type": "thinking", "text": "hmm"},
                    {"type": "text", "text": "Answer"},
                    {"type": "tool_use", "id": "tu_1", "name": "search", "input": {"q": "x"}}
                ],
                "model": "claude-test",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 10, "output_tokens": 5}
            })))
            .mount(&server)
            .await;

        let cfg = protocol_config_anthropic(&server.uri());
        let resp = complete(&cfg, &sample_request()).await.unwrap();
        assert_eq!(resp.content, "Answer");
        assert_eq!(resp.reasoning.as_deref(), Some("hmm"));
        assert_eq!(resp.finish_reason.as_deref(), Some("tool_calls"));
        assert!(resp.tool_calls.is_some());
        assert_eq!(resp.usage.total_tokens, 15);
    }

    #[tokio::test]
    async fn complete_maps_http_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let cfg = protocol_config_anthropic(&server.uri());
        let err = complete(&cfg, &sample_request()).await.unwrap_err();
        assert!(matches!(err, AiError::InvalidApiKey));
    }

    #[tokio::test]
    async fn stream_complete_emits_text_thinking_tools_and_done() {
        let server = MockServer::start().await;
        let sse = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"think\"}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"fn\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"a\\\":1}\"}}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"input_tokens\":12,\"output_tokens\":3}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;

        let cfg = protocol_config_anthropic(&server.uri());
        let req = sample_request();
        let chunks = collect_stream(|tx| stream_complete(&cfg, &req, tx)).await;
        let ok: Vec<_> = chunks.into_iter().filter_map(Result::ok).collect();
        assert!(ok.iter().any(|c| c.content == "Hel" && !c.done));
        assert!(ok.iter().any(|c| c.reasoning.as_deref() == Some("think")));
        let done = ok.iter().find(|c| c.done).expect("done chunk");
        assert_eq!(done.usage.as_ref().unwrap().total_tokens, 15);
        assert_eq!(done.tool_calls.as_ref().unwrap()[0].arguments, r#"{"a":1}"#);
    }

    #[tokio::test]
    async fn fetch_models_returns_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "claude-sonnet"}]
            })))
            .mount(&server)
            .await;

        let cfg = protocol_config_anthropic(&server.uri());
        let models = fetch_models(&cfg).await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "claude-sonnet");
    }

    #[tokio::test]
    async fn probe_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{"type": "text", "text": "."}],
                "model": "claude-test",
                "usage": {"input_tokens": 1, "output_tokens": 1}
            })))
            .mount(&server)
            .await;

        let cfg = protocol_config_anthropic(&server.uri());
        probe(&cfg, "claude-test").await.unwrap();
    }
}
