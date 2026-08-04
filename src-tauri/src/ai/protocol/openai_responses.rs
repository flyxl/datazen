//! OpenAI Responses API protocol (via async-openai SDK).
//!
//! Used by `CustomProvider(OpenAiResponses)`.

use datazen_ai_api::*;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;

use super::{build_oai_client, map_sdk_error, ProtocolConfig, STREAM_CHUNK_TIMEOUT};

// ─── BYOT types ───

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    response: Option<StreamPayload>,
}

#[derive(Deserialize)]
struct StreamPayload {
    usage: Option<UsageResp>,
}

#[derive(Deserialize)]
struct UsageResp {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct ResponseResp {
    output: Vec<OutputItem>,
    model: String,
    usage: Option<UsageResp>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    item_type: String,
    content: Option<Vec<ContentBlock>>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

// ─── Helpers ───

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

    let input = messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    _ => unreachable!(),
                },
                "content": m.content,
            })
        })
        .collect();

    (instructions, input)
}

// ─── Public API ───

pub async fn complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse, AiError> {
    let oai = build_oai_client(cfg);
    let (instructions, input) = split_instructions(&request.messages);

    let body = serde_json::json!({
        "model": request.model,
        "input": input,
        "instructions": instructions,
        "temperature": request.temperature,
        "max_output_tokens": cfg.max_tokens,
        "stream": false,
        "store": false,
    });

    tracing::info!("openai_responses: request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let resp: ResponseResp = oai
        .responses()
        .create_byot(body)
        .await
        .map_err(|e| map_sdk_error(&e))?;

    let content: String = resp
        .output
        .iter()
        .filter(|item| item.item_type == "message")
        .flat_map(|item| item.content.iter().flatten())
        .filter(|block| block.block_type == "output_text")
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>()
        .join("");

    let reasoning_text: String = resp
        .output
        .iter()
        .filter(|item| item.item_type == "reasoning")
        .flat_map(|item| item.content.iter().flatten())
        .filter(|block| block.block_type == "reasoning_text")
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>()
        .join("");

    let reasoning = if reasoning_text.is_empty() {
        None
    } else {
        Some(reasoning_text)
    };

    let usage = resp
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
        model: resp.model,
        finish_reason: Some("stop".into()),
        usage,
    })
}

pub async fn stream_complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
    sender: mpsc::Sender<Result<StreamChunk, AiError>>,
) -> Result<(), AiError> {
    let oai = build_oai_client(cfg);
    let (instructions, input) = split_instructions(&request.messages);

    let body = serde_json::json!({
        "model": request.model,
        "input": input,
        "instructions": instructions,
        "temperature": request.temperature,
        "max_output_tokens": cfg.max_tokens,
        "stream": true,
        "store": false,
    });

    tracing::info!("openai_responses: stream request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let mut stream = oai
        .responses()
        .create_stream_byot::<serde_json::Value, StreamEvent>(body)
        .await
        .map_err(|e| {
            tracing::error!("openai_responses: stream init error: {}", e);
            map_sdk_error(&e)
        })?;

    let mut chunk_count: u64 = 0;
    tracing::info!("openai_responses: stream started");

    loop {
        let result = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(Some(r)) => r,
            Ok(None) => break,
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

        chunk_count += 1;
        if chunk_count == 1 {
            tracing::info!("openai_responses: first chunk received");
        }

        match result {
            Ok(event) => match event.event_type.as_str() {
                "response.output_text.delta" => {
                    if let Some(text) = event.delta {
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
                "response.reasoning_text.delta" => {
                    if let Some(text) = event.delta {
                        if !text.is_empty()
                            && sender
                                .send(Ok(StreamChunk {
                                    content: String::new(),
                                    reasoning: Some(text),
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
                "response.completed" => {
                    tracing::info!(chunk_count, "openai_responses: stream complete");
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
                            reasoning: None,
                            done: true,
                            usage,
                        }))
                        .await;
                    return Ok(());
                }
                _ => {}
            },
            Err(e) => {
                tracing::error!("openai_responses: stream chunk error: {}", e);
                let _ = sender.send(Err(AiError::RequestFailed(e.to_string()))).await;
                return Ok(());
            }
        }
    }

    tracing::warn!(chunk_count, "openai_responses: stream ended without response.completed");
    let _ = sender
        .send(Ok(StreamChunk { content: String::new(), reasoning: None, done: true, usage: None }))
        .await;

    Ok(())
}

/// Lightweight connectivity probe with minimal token cost.
pub async fn probe(cfg: &ProtocolConfig, model: &str) -> Result<(), AiError> {
    tracing::info!(endpoint = %cfg.api_base, %model, "openai_responses: probe");
    let oai = build_oai_client(cfg);
    let body = serde_json::json!({
        "model": model,
        "input": ".",
        "max_output_tokens": 1,
        "stream": false,
        "temperature": 0,
        "store": false,
    });
    let _resp: serde_json::Value = oai.responses().create_byot(body).await.map_err(|e| map_sdk_error(&e))?;
    tracing::info!("openai_responses: probe success");
    Ok(())
}
