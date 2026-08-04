//! OpenAI Chat Completions protocol (direct HTTP via reqwest).
//!
//! Used by both `OpenAiProvider` and `CustomProvider(OpenAiCompatible)`.

use std::collections::HashMap;

use datazen_ai_api::*;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;

use super::{map_http_error, normalize_base_url, ProtocolConfig, STREAM_CHUNK_TIMEOUT};

// ─── Wire types ───

#[derive(Deserialize)]
struct ChatResponseResp {
    choices: Vec<ChatChoiceResp>,
    #[serde(default)]
    usage: Option<UsageResp>,
    model: String,
}

#[derive(Deserialize)]
struct ChatChoiceResp {
    message: Option<MessageResp>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct MessageResp {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OaiToolCall>>,
}

#[derive(Clone, Deserialize)]
struct OaiToolCall {
    id: String,
    function: OaiFunction,
}

#[derive(Clone, Deserialize)]
struct OaiFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct StreamChunkResp {
    choices: Vec<StreamChoiceResp>,
    #[serde(default)]
    usage: Option<UsageResp>,
}

#[derive(Deserialize)]
struct StreamChoiceResp {
    delta: Option<DeltaResp>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct DeltaResp {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Clone, Deserialize)]
struct StreamToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunctionDelta>,
}

#[derive(Clone, Deserialize)]
struct StreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct UsageResp {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ─── Helpers ───

fn chat_completions_url(api_base: &str) -> String {
    let base = normalize_base_url(api_base);
    format!("{}/chat/completions", base.trim_end_matches('/'))
}

/// Convert ChatMessage slice to OpenAI wire format, including tool messages.
///
/// When a `ChatMessage` carries `reasoning` (e.g. from DeepSeek thinking mode),
/// we pass it back as `reasoning_content` so the provider can resume the chain.
fn to_oai_messages(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| match m.role {
            MessageRole::Tool => serde_json::json!({
                "role": "tool",
                "tool_call_id": m.tool_call_id,
                "content": m.content,
            }),
            MessageRole::Assistant if m.tool_calls.as_ref().is_some_and(|t| !t.is_empty()) => {
                let tool_calls: Vec<serde_json::Value> = m
                    .tool_calls
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments,
                            }
                        })
                    })
                    .collect();
                let mut msg = serde_json::json!({
                    "role": "assistant",
                    "tool_calls": tool_calls,
                });
                if m.content.is_empty() {
                    msg["content"] = serde_json::Value::Null;
                } else {
                    msg["content"] = serde_json::Value::String(m.content.clone());
                }
                if let Some(reasoning) = &m.reasoning {
                    if !reasoning.is_empty() {
                        msg["reasoning_content"] = serde_json::Value::String(reasoning.clone());
                    }
                }
                msg
            }
            MessageRole::Assistant => {
                let mut msg = serde_json::json!({
                    "role": "assistant",
                    "content": m.content,
                });
                if let Some(reasoning) = &m.reasoning {
                    if !reasoning.is_empty() {
                        msg["reasoning_content"] = serde_json::Value::String(reasoning.clone());
                    }
                }
                msg
            }
            _ => serde_json::json!({
                "role": match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => unreachable!(),
                    MessageRole::Tool => unreachable!(),
                },
                "content": m.content,
            }),
        })
        .collect()
}

fn to_oai_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}

fn build_request_body(cfg: &ProtocolConfig, request: &CompletionRequest, stream: bool) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model,
        "messages": to_oai_messages(&request.messages),
        "temperature": request.temperature,
        "max_tokens": cfg.max_tokens,
        "stop": request.stop,
        "stream": stream,
    });

    if stream {
        body["stream_options"] = serde_json::json!({ "include_usage": true });
    }

    if let Some(tools) = &request.tools {
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(to_oai_tools(tools));
        }
    }

    body
}

fn parse_oai_tool_calls(calls: Option<Vec<OaiToolCall>>) -> Option<Vec<ToolCall>> {
    let calls = calls?;
    if calls.is_empty() {
        return None;
    }
    Some(
        calls
            .into_iter()
            .map(|c| ToolCall {
                id: c.id,
                name: c.function.name,
                arguments: c.function.arguments,
            })
            .collect(),
    )
}

struct AccumulatedToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn accumulate_tool_call_delta(
    acc: &mut HashMap<usize, AccumulatedToolCall>,
    delta: StreamToolCallDelta,
) {
    let entry = acc.entry(delta.index).or_insert_with(|| AccumulatedToolCall {
        id: String::new(),
        name: String::new(),
        arguments: String::new(),
    });
    if let Some(id) = delta.id {
        entry.id = id;
    }
    if let Some(function) = delta.function {
        if let Some(name) = function.name {
            entry.name = name;
        }
        if let Some(args) = function.arguments {
            entry.arguments.push_str(&args);
        }
    }
}

fn finalize_tool_calls(acc: &HashMap<usize, AccumulatedToolCall>) -> Option<Vec<ToolCall>> {
    if acc.is_empty() {
        return None;
    }
    let mut indices: Vec<_> = acc.keys().copied().collect();
    indices.sort_unstable();
    Some(
        indices
            .into_iter()
            .map(|i| {
                let c = &acc[&i];
                ToolCall {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    arguments: c.arguments.clone(),
                }
            })
            .collect(),
    )
}

// ─── Public API ───

pub async fn complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse, AiError> {
    let url = chat_completions_url(&cfg.api_base);
    let body = build_request_body(cfg, request, false);

    tracing::info!("openai_chat: request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let resp = cfg
        .http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::RequestFailed(e.to_string()))?;

    let status = resp.status();
    let raw = resp.text().await.unwrap_or_default();
    tracing::info!(%status, "openai_chat: response\n{}", raw);

    if !status.is_success() {
        return Err(map_http_error(status, &raw));
    }

    let resp: ChatResponseResp = serde_json::from_str(&raw)
        .map_err(|e| AiError::RequestFailed(format!("JSON decode: {e}")))?;

    let choice = resp
        .choices
        .first()
        .ok_or_else(|| AiError::RequestFailed("No choices in response".into()))?;

    let (content, reasoning, tool_calls) = choice
        .message
        .as_ref()
        .map(|m| {
            let c = m.content.clone().unwrap_or_default();
            let r = m
                .reasoning_content
                .clone()
                .filter(|s| !s.is_empty());
            let tc = parse_oai_tool_calls(m.tool_calls.clone());
            (c, r, tc)
        })
        .unwrap_or_default();

    let usage = resp
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
        reasoning,
        model: resp.model,
        finish_reason: choice.finish_reason.clone(),
        usage,
        tool_calls,
        response_id: None,
    })
}

pub async fn stream_complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
    sender: mpsc::Sender<Result<StreamChunk, AiError>>,
) -> Result<(), AiError> {
    let url = chat_completions_url(&cfg.api_base);
    let body = build_request_body(cfg, request, true);

    tracing::info!(
        "openai_chat: stream request\n{}",
        serde_json::to_string(&body).unwrap_or_default()
    );

    let resp = cfg
        .http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("openai_chat: stream init error: {}", e);
            AiError::RequestFailed(e.to_string())
        })?;

    let status = resp.status();
    tracing::info!(%status, "openai_chat: stream started");
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        tracing::error!(%status, "openai_chat: stream HTTP error\n{}", text);
        return Err(map_http_error(status, &text));
    }

    let mut byte_buf = Vec::new();
    let mut stream = resp.bytes_stream();
    let mut chunk_count: u64 = 0;
    let mut tool_calls_acc: HashMap<usize, AccumulatedToolCall> = HashMap::new();
    let mut last_usage: Option<TokenUsage> = None;
    let mut saw_finish = false;

    loop {
        let maybe = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(m) => m,
            Err(_) => {
                tracing::error!(chunk_count, "openai_chat: stream timed out");
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
                tracing::error!("openai_chat: stream read error: {}", e);
                let _ = sender
                    .send(Err(AiError::RequestFailed(e.to_string())))
                    .await;
                return Ok(());
            }
        };

        chunk_count += 1;
        if chunk_count == 1 {
            tracing::info!("openai_chat: first chunk received ({} bytes)", chunk_bytes.len());
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

            if data == "[DONE]" {
                saw_finish = true;
                break;
            }

            let chunk: StreamChunkResp = match serde_json::from_str(data) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("openai_chat: failed to parse SSE chunk: {e}\n{data}");
                    continue;
                }
            };

            if let Some(usage) = chunk.usage {
                last_usage = Some(TokenUsage {
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    total_tokens: usage.total_tokens,
                });
            }

            if let Some(choice) = chunk.choices.first() {
                if let Some(delta) = &choice.delta {
                    if let Some(tool_deltas) = &delta.tool_calls {
                        for td in tool_deltas {
                            accumulate_tool_call_delta(&mut tool_calls_acc, td.clone());
                        }
                    }

                    let content = delta.content.clone().unwrap_or_default();
                    let reasoning = delta
                        .reasoning_content
                        .clone()
                        .filter(|r| !r.is_empty());

                    if !content.is_empty() || reasoning.is_some() {
                        if sender
                            .send(Ok(StreamChunk {
                                content,
                                reasoning,
                                done: false,
                                response_id: None,
                                usage: None,
                                tool_calls: None,
                            }))
                            .await
                            .is_err()
                        {
                            return Ok(());
                        }
                    }
                }

                if choice.finish_reason.is_some() {
                    saw_finish = true;
                }
            }
        }

        if saw_finish {
            break;
        }
    }

    tracing::info!(chunk_count, "openai_chat: stream complete");
    let tool_calls = finalize_tool_calls(&tool_calls_acc);
    let _ = sender
        .send(Ok(StreamChunk {
            content: String::new(),
            reasoning: None,
            done: true,
            response_id: None,
            usage: last_usage,
            tool_calls,
        }))
        .await;

    Ok(())
}

/// Fetch model list via the /models endpoint.
///
/// Uses a custom response type instead of the SDK's `Model` struct because
/// many OpenAI-compatible APIs (DeepSeek, etc.) omit optional fields like
/// `created` that the SDK treats as required.
pub async fn fetch_models(cfg: &ProtocolConfig) -> Result<Vec<ModelInfo>, AiError> {
    let base = normalize_base_url(&cfg.api_base);
    let url = format!("{}/models", base.trim_end_matches('/'));
    tracing::info!(%url, "openai_chat: fetch_models");

    let resp = cfg
        .http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
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

/// Lightweight connectivity probe with minimal token cost.
pub async fn probe(cfg: &ProtocolConfig, model: &str) -> Result<(), AiError> {
    let url = chat_completions_url(&cfg.api_base);
    tracing::info!(endpoint = %cfg.api_base, %model, "openai_chat: probe");

    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "."}],
        "max_tokens": 1,
        "stream": false,
        "temperature": 0,
        "reasoning_effort": "none",
    });

    let resp = cfg
        .http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::RequestFailed(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(map_http_error(status, &text));
    }

    tracing::info!("openai_chat: probe success");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_oai_messages_tool_role() {
        let messages = vec![ChatMessage {
            role: MessageRole::Tool,
            content: r#"{"result": 42}"#.into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: Some("call_abc".into()),
        }];
        let json = to_oai_messages(&messages);
        assert_eq!(json[0]["role"], "tool");
        assert_eq!(json[0]["tool_call_id"], "call_abc");
        assert_eq!(json[0]["content"], r#"{"result": 42}"#);
    }

    #[test]
    fn to_oai_messages_assistant_with_tool_calls() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: String::new(),
            reasoning: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_1".into(),
                name: "get_weather".into(),
                arguments: r#"{"city":"NYC"}"#.into(),
            }]),
            tool_call_id: None,
        }];
        let json = to_oai_messages(&messages);
        assert_eq!(json[0]["role"], "assistant");
        assert!(json[0]["content"].is_null());
        assert_eq!(json[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(json[0]["tool_calls"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn to_oai_messages_assistant_with_reasoning() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: "The answer is 42.".into(),
            reasoning: Some("Let me think about this step by step...".into()),
            tool_calls: None,
            tool_call_id: None,
        }];
        let json = to_oai_messages(&messages);
        assert_eq!(json[0]["role"], "assistant");
        assert_eq!(json[0]["content"], "The answer is 42.");
        assert_eq!(
            json[0]["reasoning_content"],
            "Let me think about this step by step..."
        );
    }

    #[test]
    fn to_oai_messages_assistant_with_tool_calls_and_reasoning() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: "Let me check.".into(),
            reasoning: Some("I need to call a tool.".into()),
            tool_calls: Some(vec![ToolCall {
                id: "call_1".into(),
                name: "list_tables".into(),
                arguments: "{}".into(),
            }]),
            tool_call_id: None,
        }];
        let json = to_oai_messages(&messages);
        assert_eq!(json[0]["role"], "assistant");
        assert_eq!(json[0]["content"], "Let me check.");
        assert_eq!(json[0]["reasoning_content"], "I need to call a tool.");
        assert_eq!(json[0]["tool_calls"][0]["id"], "call_1");
    }

    #[test]
    fn to_oai_messages_assistant_empty_reasoning_not_included() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: "Hello".into(),
            reasoning: Some(String::new()),
            tool_calls: None,
            tool_call_id: None,
        }];
        let json = to_oai_messages(&messages);
        assert_eq!(json[0]["role"], "assistant");
        assert!(json[0].get("reasoning_content").is_none());
    }

    #[test]
    fn accumulate_stream_tool_calls() {
        let mut acc = HashMap::new();
        accumulate_tool_call_delta(
            &mut acc,
            StreamToolCallDelta {
                index: 0,
                id: Some("call_x".into()),
                function: Some(StreamFunctionDelta {
                    name: Some("fn".into()),
                    arguments: Some("{".into()),
                }),
            },
        );
        accumulate_tool_call_delta(
            &mut acc,
            StreamToolCallDelta {
                index: 0,
                id: None,
                function: Some(StreamFunctionDelta {
                    name: None,
                    arguments: Some(r#""a":1}"#.into()),
                }),
            },
        );
        let calls = finalize_tool_calls(&acc).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_x");
        assert_eq!(calls[0].name, "fn");
        assert_eq!(calls[0].arguments, r#"{"a":1}"#);
    }
}
