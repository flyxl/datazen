//! Shared helpers for protocol HTTP unit tests (wiremock).

use datazen_ai_api::*;
use reqwest::Client as HttpClient;
use tokio::sync::mpsc;

use super::{ProtocolConfig, CONNECT_TIMEOUT};

pub fn test_http_client() -> HttpClient {
    HttpClient::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap()
}

pub fn protocol_config(base_uri: &str) -> ProtocolConfig {
    protocol_config_with_base(format!("{}/v1", base_uri.trim_end_matches('/')))
}

/// Anthropic bases omit `/v1`; `build_url` appends `/v1/messages`.
pub fn protocol_config_anthropic(base_uri: &str) -> ProtocolConfig {
    protocol_config_with_base(base_uri.trim_end_matches('/').to_string())
}

fn protocol_config_with_base(api_base: String) -> ProtocolConfig {
    ProtocolConfig {
        http_client: test_http_client(),
        api_base,
        api_key: "test-api-key".to_string(),
        max_tokens: 256,
    }
}

pub fn sample_request() -> CompletionRequest {
    CompletionRequest {
        request_id: "req-1".into(),
        model: "test-model".into(),
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: "Hi".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        temperature: Some(0.5),
        stop: None,
        tools: None,
        previous_response_id: None,
    }
}

pub async fn collect_stream<F, Fut>(f: F) -> Vec<Result<StreamChunk, AiError>>
where
    F: FnOnce(mpsc::Sender<Result<StreamChunk, AiError>>) -> Fut,
    Fut: std::future::Future<Output = Result<(), AiError>>,
{
    let (tx, mut rx) = mpsc::channel(32);
    let stream = f(tx);
    let (_, chunks) = tokio::join!(stream, async {
        let mut out = Vec::new();
        while let Some(chunk) = rx.recv().await {
            out.push(chunk);
        }
        out
    });
    chunks
}
