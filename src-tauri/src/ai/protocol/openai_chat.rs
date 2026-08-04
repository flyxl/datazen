//! OpenAI Chat Completions protocol (via async-openai SDK).
//!
//! Used by both `OpenAiProvider` and `CustomProvider(OpenAiCompatible)`.

use datazen_ai_api::*;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;

use super::{build_oai_client, map_sdk_error, to_chat_messages, ProtocolConfig, STREAM_CHUNK_TIMEOUT};

// ─── BYOT types (supports reasoning_content for DeepSeek etc.) ───

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
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct UsageResp {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct ChatResponseResp {
    choices: Vec<ChatChoiceResp>,
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
}

// ─── Public API ───

pub async fn complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse, AiError> {
    let oai = build_oai_client(cfg);

    let body = serde_json::json!({
        "model": request.model,
        "messages": to_chat_messages(&request.messages),
        "temperature": request.temperature,
        "max_tokens": cfg.max_tokens,
        "stop": request.stop,
        "stream": false,
    });

    tracing::info!("openai_chat: request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let resp: ChatResponseResp = oai
        .chat()
        .create_byot(body)
        .await
        .map_err(|e| map_sdk_error(&e))?;

    let choice = resp
        .choices
        .first()
        .ok_or_else(|| AiError::RequestFailed("No choices in response".into()))?;

    let (content, reasoning) = choice
        .message
        .as_ref()
        .map(|m| {
            let c = m.content.clone().unwrap_or_default();
            let r = m.reasoning_content.clone().filter(|s| !s.is_empty());
            (c, r)
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
    })
}

pub async fn stream_complete(
    cfg: &ProtocolConfig,
    request: &CompletionRequest,
    sender: mpsc::Sender<Result<StreamChunk, AiError>>,
) -> Result<(), AiError> {
    let oai = build_oai_client(cfg);

    let body = serde_json::json!({
        "model": request.model,
        "messages": to_chat_messages(&request.messages),
        "temperature": request.temperature,
        "max_tokens": cfg.max_tokens,
        "stop": request.stop,
        "stream": true,
    });

    tracing::info!("openai_chat: stream request\n{}", serde_json::to_string(&body).unwrap_or_default());

    let mut stream = oai
        .chat()
        .create_stream_byot::<serde_json::Value, StreamChunkResp>(body)
        .await
        .map_err(|e| {
            tracing::error!("openai_chat: stream init error: {}", e);
            map_sdk_error(&e)
        })?;

    let mut chunk_count: u64 = 0;
    tracing::info!("openai_chat: stream started");

    loop {
        let result = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(Some(r)) => r,
            Ok(None) => break,
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

        chunk_count += 1;
        if chunk_count == 1 {
            tracing::info!("openai_chat: first chunk received");
        }

        match result {
            Ok(chunk) => {
                if let Some(choice) = chunk.choices.first() {
                    let content = choice
                        .delta
                        .as_ref()
                        .and_then(|d| d.content.clone())
                        .unwrap_or_default();
                    let reasoning = choice
                        .delta
                        .as_ref()
                        .and_then(|d| d.reasoning_content.clone())
                        .filter(|r| !r.is_empty());
                    let done = choice.finish_reason.is_some();
                    let usage = chunk.usage.map(|u| TokenUsage {
                        prompt_tokens: u.prompt_tokens,
                        completion_tokens: u.completion_tokens,
                        total_tokens: u.total_tokens,
                    });

                    if !content.is_empty() || reasoning.is_some() || done {
                        if sender
                            .send(Ok(StreamChunk { content, reasoning, done, usage }))
                            .await
                            .is_err()
                        {
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("openai_chat: stream chunk error: {}", e);
                let _ = sender.send(Err(AiError::RequestFailed(e.to_string()))).await;
                return Ok(());
            }
        }
    }

    tracing::info!(chunk_count, "openai_chat: stream complete");
    let _ = sender
        .send(Ok(StreamChunk { content: String::new(), reasoning: None, done: true, usage: None }))
        .await;

    Ok(())
}

/// Fetch model list via the /models endpoint.
pub async fn fetch_models(cfg: &ProtocolConfig) -> Result<Vec<ModelInfo>, AiError> {
    tracing::info!(endpoint = %cfg.api_base, "openai_chat: fetch_models");
    let oai = build_oai_client(cfg);
    let resp = oai.models().list().await.map_err(|e| map_sdk_error(&e))?;

    Ok(resp
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
    tracing::info!(endpoint = %cfg.api_base, %model, "openai_chat: probe");
    let oai = build_oai_client(cfg);
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "."}],
        "max_tokens": 1,
        "stream": false,
        "temperature": 0,
        "reasoning_effort": "none",
    });
    let _resp: serde_json::Value = oai.chat().create_byot(body).await.map_err(|e| map_sdk_error(&e))?;
    tracing::info!("openai_chat: probe success");
    Ok(())
}
