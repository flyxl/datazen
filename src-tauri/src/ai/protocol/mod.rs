//! Protocol layer — shared implementations for OpenAI and Anthropic wire protocols.
//!
//! Each protocol module provides `complete()` and `stream_complete()` functions.
//! Providers delegate to these via [`ProtocolConfig`].

pub mod anthropic;
pub mod openai_chat;
pub mod openai_responses;

use std::time::Duration;

use reqwest::Client as HttpClient;

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

/// Strip known path suffixes so protocol modules don't double-append.
pub(crate) fn normalize_base_url(endpoint: &str) -> String {
    let base = endpoint.trim_end_matches('/');
    for suffix in ["/chat/completions", "/responses", "/models", "/v1/messages"] {
        if let Some(stripped) = base.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    base.to_string()
}

/// Convert HTTP status + body to AiError.
pub(crate) fn map_http_error(status: reqwest::StatusCode, body: &str) -> datazen_ai_api::AiError {
    match status.as_u16() {
        401 => datazen_ai_api::AiError::InvalidApiKey,
        429 => datazen_ai_api::AiError::RateLimited {
            retry_after_secs: 60,
        },
        _ => datazen_ai_api::AiError::RequestFailed(format!(
            "HTTP {status}: {}",
            truncate_str(body, 500)
        )),
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
        assert_eq!(
            normalize_base_url("https://api.anthropic.com/v1/messages"),
            "https://api.anthropic.com"
        );
    }
}
