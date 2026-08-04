//! OpenAI Responses API protocol (direct HTTP via reqwest).
//!
//! Used by `CustomProvider(OpenAiResponses)`.

use datazen_ai_api::*;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;

use super::{map_http_error, normalize_base_url, ProtocolConfig, STREAM_CHUNK_TIMEOUT};

// ─── Wire types ───

#[derive(Deserialize)]
struct UsageResp {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct ResponseResp {
    #[serde(default)]
    id: Option<String>,
    output: Vec<OutputItem>,
    model: String,
    #[serde(default)]
    usage: Option<UsageResp>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    content: Option<Vec<ContentBlock>>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
}

// SSE event (streaming)
#[derive(Deserialize)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    response: Option<SseResponsePayload>,
    #[serde(default)]
    item: Option<SseItemPayload>,
}

#[derive(Deserialize)]
struct SseResponsePayload {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    usage: Option<UsageResp>,
    #[serde(default)]
    output: Option<Vec<OutputItem>>,
}

#[derive(Deserialize)]
struct SseItemPayload {
    #[serde(rename = "type", default)]
    item_type: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
}

// ─── Helpers ───

fn responses_url(api_base: &str) -> String {
    let base = normalize_base_url(api_base);
    format!("{}/responses", base.trim_end_matches('/'))
}

fn split_instructions(
    messages: &[ChatMessage],
) -> (Option<String>, Vec<serde_json::Value>) {
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

    let input: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .filter_map(|m| match m.role {
            MessageRole::User => Some(serde_json::json!({
                "role": "user",
                "content": m.content,
            })),
            MessageRole::Assistant => {
                let has_tool_calls = m.tool_calls.as_ref().is_some_and(|t| !t.is_empty());
                let has_reasoning = m.reasoning.as_ref().is_some_and(|r| !r.is_empty());

                if has_tool_calls || has_reasoning {
                    let mut items = Vec::new();
                    if let Some(reasoning) = &m.reasoning {
                        if !reasoning.is_empty() {
                            items.push(serde_json::json!({
                                "type": "reasoning",
                                "id": format!("rs_{}", items.len()),
                                "content": [{"type": "reasoning_text", "text": reasoning}],
                            }));
                        }
                    }
                    items.push(serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": m.content}],
                    }));
                    if let Some(tool_calls) = &m.tool_calls {
                        for tc in tool_calls {
                            items.push(serde_json::json!({
                                "type": "function_call",
                                "id": tc.id,
                                "name": tc.name,
                                "arguments": tc.arguments,
                                "call_id": tc.id,
                            }));
                        }
                    }
                    return Some(serde_json::Value::Array(items));
                }
                Some(serde_json::json!({
                    "role": "assistant",
                    "content": m.content,
                }))
            }
            MessageRole::Tool => Some(serde_json::json!({
                "type": "function_call_output",
                "call_id": m.tool_call_id,
                "output": m.content,
            })),
            _ => None,
        })
        .flat_map(|v| {
            if v.is_array() {
                v.as_array().unwrap().clone()
            } else {
                vec![v]
            }
        })
        .collect();

    (instructions, input)
}

fn build_request_body(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
    stream: bool,
) -> serde_json::Value {
    let use_prev_id = request.previous_response_id.is_some();

    let (instructions, input) = if use_prev_id {
        // When using previous_response_id, only send new messages (non-system).
        // The server already has instructions from the stored response — don't re-send them.
        let new_input: Vec<serde_json::Value> = request.messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .filter(|m| m.role == MessageRole::User || m.role == MessageRole::Tool)
            .filter_map(|m| match m.role {
                MessageRole::User => Some(serde_json::json!({
                    "role": "user",
                    "content": m.content,
                })),
                MessageRole::Tool => Some(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": m.tool_call_id,
                    "output": m.content,
                })),
                _ => None,
            })
            .collect();
        (None, new_input)
    } else {
        split_instructions(&request.messages)
    };

    let store = use_prev_id;

    let mut body = serde_json::json!({
        "model": request.model,
        "input": input,
        "temperature": request.temperature,
        "max_output_tokens": cfg.max_tokens,
        "stream": stream,
        "store": store,
    });

    if let Some(instr) = &instructions {
        body["instructions"] = serde_json::Value::String(instr.clone());
    }

    if let Some(prev_id) = &request.previous_response_id {
        body["previous_response_id"] = serde_json::Value::String(prev_id.clone());
    }

    if let Some(tools) = &request.tools {
        if !tools.is_empty() {
            let oai_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(oai_tools);
        }
    }

    body
}

fn parse_output_tool_calls(output: &[OutputItem]) -> Option<Vec<ToolCall>> {
    let calls: Vec<ToolCall> = output
        .iter()
        .filter(|item| item.item_type == "function_call")
        .filter_map(|item| {
            let id = item.id.as_deref().or(item.call_id.as_deref())?;
            let name = item.name.as_deref()?;
            let arguments = item.arguments.as_deref().unwrap_or("{}");
            Some(ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: arguments.to_string(),
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
    let url = responses_url(&cfg.api_base);
    let body = build_request_body(cfg, request, false);

    tracing::info!("openai_responses: request\n{}", serde_json::to_string(&body).unwrap_or_default());

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
    tracing::info!(%status, "openai_responses: response\n{}", raw);

    if !status.is_success() {
        return Err(map_http_error(status, &raw));
    }

    let api_resp: ResponseResp = serde_json::from_str(&raw)
        .map_err(|e| AiError::RequestFailed(format!("JSON decode: {e}")))?;

    let content: String = api_resp
        .output
        .iter()
        .filter(|item| item.item_type == "message")
        .flat_map(|item| item.content.iter().flatten())
        .filter(|block| block.block_type == "output_text")
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>()
        .join("");

    let reasoning_text: String = api_resp
        .output
        .iter()
        .filter(|item| item.item_type == "reasoning")
        .flat_map(|item| item.content.iter().flatten())
        .filter(|block| block.block_type == "reasoning_text")
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>()
        .join("");

    let reasoning = if reasoning_text.is_empty() { None } else { Some(reasoning_text) };

    let tool_calls = parse_output_tool_calls(&api_resp.output);
    let finish_reason = if tool_calls.is_some() {
        Some("tool_calls".into())
    } else {
        Some("stop".into())
    };

    let usage = api_resp
        .usage
        .map(|u| TokenUsage {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            total_tokens: u.total_tokens,
        })
        .unwrap_or_default();

    Ok(CompletionResponse {
        request_id: request.request_id.clone(),
        content,
        reasoning,
        model: api_resp.model,
        finish_reason,
        usage,
        tool_calls,
        response_id: api_resp.id,
    })
}

pub async fn stream_complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
    sender: mpsc::Sender<Result<StreamChunk, AiError>>,
) -> Result<(), AiError> {
    let url = responses_url(&cfg.api_base);
    let body = build_request_body(cfg, request, true);

    tracing::info!("openai_responses: stream request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let resp = cfg
        .http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("openai_responses: stream init error: {}", e);
            AiError::RequestFailed(e.to_string())
        })?;

    let status = resp.status();
    tracing::info!(%status, "openai_responses: stream started");
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        tracing::error!(%status, "openai_responses: stream HTTP error\n{}", text);
        return Err(map_http_error(status, &text));
    }

    let mut byte_buf = Vec::new();
    let mut stream = resp.bytes_stream();
    let mut chunk_count: u64 = 0;
    let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
    let mut current_fc_id = String::new();
    let mut current_fc_name = String::new();
    let mut current_fc_args = String::new();
    let mut response_id: Option<String> = None;

    loop {
        let maybe = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(m) => m,
            Err(_) => {
                tracing::error!(chunk_count, "openai_responses: stream timed out");
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
                tracing::error!("openai_responses: stream read error: {}", e);
                let _ = sender.send(Err(AiError::RequestFailed(e.to_string()))).await;
                return Ok(());
            }
        };

        chunk_count += 1;
        if chunk_count == 1 {
            tracing::info!("openai_responses: first chunk received ({} bytes)", chunk_bytes.len());
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

            let event: SseEvent = match serde_json::from_str(data) {
                Ok(e) => e,
                Err(_) => continue,
            };

            match event.event_type.as_str() {
                "response.output_text.delta" => {
                    if let Some(text) = event.delta {
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
                }
                "response.reasoning_text.delta" => {
                    if let Some(text) = event.delta {
                        if !text.is_empty()
                            && sender
                                .send(Ok(StreamChunk {
                                    content: String::new(),
                                    reasoning: Some(text),
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
                }
                "response.created" | "response.in_progress" => {
                    if let Some(r) = &event.response {
                        if let Some(id) = &r.id {
                            response_id = Some(id.clone());
                        }
                    }
                }
                "response.function_call_arguments.delta" => {
                    if let Some(args) = event.delta {
                        current_fc_args.push_str(&args);
                    }
                }
                "response.function_call_arguments.done" => {
                    pending_tool_calls.push(ToolCall {
                        id: std::mem::take(&mut current_fc_id),
                        name: std::mem::take(&mut current_fc_name),
                        arguments: std::mem::take(&mut current_fc_args),
                    });
                }
                "response.output_item.added" => {
                    if let Some(item) = &event.item {
                        if item.item_type.as_deref() == Some("function_call") {
                            current_fc_id = item.id.clone().or(item.call_id.clone()).unwrap_or_default();
                            current_fc_name = item.name.clone().unwrap_or_default();
                            current_fc_args.clear();
                        }
                    }
                }
                "response.completed" | "response.incomplete" => {
                    tracing::info!(chunk_count, "openai_responses: stream complete");
                    let usage = event
                        .response
                        .and_then(|r| {
                            if let Some(id) = &r.id {
                                response_id = Some(id.clone());
                            }
                            if let Some(output) = &r.output {
                                let extra = parse_output_tool_calls(output);
                                if let Some(extra) = extra {
                                    for tc in extra {
                                        if !pending_tool_calls.iter().any(|p| p.id == tc.id) {
                                            pending_tool_calls.push(tc);
                                        }
                                    }
                                }
                            }
                            r.usage
                        })
                        .map(|u| TokenUsage {
                            prompt_tokens: u.input_tokens,
                            completion_tokens: u.output_tokens,
                            total_tokens: u.total_tokens,
                        });
                    let tool_calls = if pending_tool_calls.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut pending_tool_calls))
                    };
                    let _ = sender
                        .send(Ok(StreamChunk {
                            content: String::new(),
                            reasoning: None,
                            done: true,
                            usage,
                            tool_calls,
                            response_id: response_id.take(),
                        }))
                        .await;
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    tracing::warn!(chunk_count, "openai_responses: stream ended without response.completed");
    let tool_calls = if pending_tool_calls.is_empty() {
        None
    } else {
        Some(pending_tool_calls)
    };
    let _ = sender
        .send(Ok(StreamChunk {
            content: String::new(),
            reasoning: None,
            done: true,
            usage: None,
            tool_calls,
            response_id: response_id.take(),
        }))
        .await;

    Ok(())
}

/// Lightweight connectivity probe with minimal token cost.
pub async fn probe(cfg: &ProtocolConfig, model: &str) -> Result<(), AiError> {
    let url = responses_url(&cfg.api_base);
    tracing::info!(endpoint = %cfg.api_base, %model, "openai_responses: probe");

    let body = serde_json::json!({
        "model": model,
        "input": ".",
        "max_output_tokens": 1,
        "stream": false,
        "temperature": 0,
        "store": false,
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

    tracing::info!("openai_responses: probe success");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_instructions_assistant_with_reasoning_no_tool_calls() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: "Hello".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: "The answer is 42.".into(),
                reasoning: Some("Let me reason...".into()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let (_, input) = split_instructions(&messages);
        assert_eq!(input.len(), 3);
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[1]["type"], "reasoning");
        assert_eq!(input[1]["content"][0]["type"], "reasoning_text");
        assert_eq!(input[1]["content"][0]["text"], "Let me reason...");
        assert_eq!(input[2]["type"], "message");
        assert_eq!(input[2]["content"][0]["text"], "The answer is 42.");
    }

    #[test]
    fn split_instructions_assistant_with_reasoning_and_tool_calls() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: "Checking...".into(),
            reasoning: Some("Need to explore DB.".into()),
            tool_calls: Some(vec![ToolCall {
                id: "tc_1".into(),
                name: "list_tables".into(),
                arguments: "{}".into(),
            }]),
            tool_call_id: None,
        }];
        let (_, input) = split_instructions(&messages);
        assert_eq!(input[0]["type"], "reasoning");
        assert_eq!(input[1]["type"], "message");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["name"], "list_tables");
    }

    #[test]
    fn split_instructions_assistant_no_reasoning() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: "Simple answer".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        let (_, input) = split_instructions(&messages);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "assistant");
        assert_eq!(input[0]["content"], "Simple answer");
        assert!(input[0].get("type").is_none());
    }

    #[test]
    fn split_instructions_tool_result() {
        let messages = vec![ChatMessage {
            role: MessageRole::Tool,
            content: "[\"db1\", \"db2\"]".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: Some("tc_1".into()),
        }];
        let (_, input) = split_instructions(&messages);
        assert_eq!(input[0]["type"], "function_call_output");
        assert_eq!(input[0]["call_id"], "tc_1");
    }
}
