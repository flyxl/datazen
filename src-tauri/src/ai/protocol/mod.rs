//! Protocol layer — shared implementations for OpenAI and Anthropic wire protocols.
//!
//! Each protocol module provides `complete()` and `stream_complete()` functions.
//! Providers delegate to these via [`ProtocolConfig`].

pub mod anthropic;
pub mod openai_chat;
pub mod openai_responses;

use std::time::Duration;

use async_openai::config::OpenAIConfig;
use reqwest::Client as HttpClient;

use datazen_ai_api::*;

/// Shared configuration for protocol-level calls.
pub struct ProtocolConfig {
    pub http_client: HttpClient,
    pub api_base: String,
    pub api_key: String,
    pub max_tokens: u32,
}

/// Stream chunk read timeout (per-chunk, not total).
pub const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(120);

/// Connect timeout for new HTTP clients.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Build an `async_openai::Client` from a protocol config.
pub(crate) fn build_oai_client(cfg: &ProtocolConfig) -> async_openai::Client<OpenAIConfig> {
    let config = OpenAIConfig::new()
        .with_api_base(normalize_base_url(&cfg.api_base))
        .with_api_key(&cfg.api_key);
    async_openai::Client::build(cfg.http_client.clone(), config)
}

/// Strip known path suffixes so the SDK doesn't double-append.
fn normalize_base_url(endpoint: &str) -> String {
    let base = endpoint.trim_end_matches('/');
    for suffix in ["/chat/completions", "/responses", "/models"] {
        if let Some(stripped) = base.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    base.to_string()
}

/// Convert SDK errors to our AiError enum.
pub(crate) fn map_sdk_error(e: &async_openai::error::OpenAIError) -> AiError {
    let msg = e.to_string();
    if msg.contains("401") || msg.contains("Unauthorized") || msg.contains("invalid_api_key") {
        AiError::InvalidApiKey
    } else if msg.contains("429") || msg.contains("rate_limit") {
        AiError::RateLimited {
            retry_after_secs: 60,
        }
    } else {
        AiError::RequestFailed(msg)
    }
}

/// Convert HTTP status + body to AiError.
pub(crate) fn map_http_error(status: reqwest::StatusCode, body: &str) -> AiError {
    match status.as_u16() {
        401 => AiError::InvalidApiKey,
        429 => AiError::RateLimited {
            retry_after_secs: 60,
        },
        _ => AiError::RequestFailed(format!("HTTP {status}: {}", truncate_str(body, 500))),
    }
}

pub(crate) fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Build messages array for OpenAI-style APIs.
pub(crate) fn to_chat_messages(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                },
                "content": m.content,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_base_url() {
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/responses"),
            "https://api.openai.com/v1"
        );
    }
}
