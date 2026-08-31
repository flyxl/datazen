//! Protocol layer — shared implementations for OpenAI and Anthropic wire protocols.
//!
//! Each protocol module provides `complete()` and `stream_complete()` functions.
//! Providers delegate to these via [`ProtocolConfig`].

pub mod anthropic;
pub mod openai_chat;
pub mod openai_responses;

use std::time::Duration;

use datazen_ai_api::CompletionRequest;
use reqwest::Client as HttpClient;
use serde::Serialize;

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
            "HTTP {status}: provider request failed (response body omitted; {} bytes)",
            body.len()
        )),
    }
}

/// Log only non-sensitive request metadata. The serialized body is used to
/// measure its size, never formatted into the event.
pub(crate) fn log_request_metadata<T: Serialize>(
    protocol: &str,
    request: &CompletionRequest,
    body: &T,
    streaming: bool,
) {
    let request_bytes = serde_json::to_vec(body).map_or(0, |payload| payload.len());
    tracing::debug!(
        protocol,
        request_id = %request.request_id,
        message_count = request.messages.len(),
        tool_count = request.tools.as_ref().map_or(0, |tools| tools.len()),
        request_bytes,
        streaming,
        "AI provider request prepared"
    );
}

/// Log response metadata without exposing response content or tool arguments.
pub(crate) fn log_response_metadata(
    protocol: &str,
    request_id: &str,
    status: reqwest::StatusCode,
    response_bytes: usize,
) {
    tracing::debug!(
        protocol,
        request_id,
        %status,
        response_bytes,
        "AI provider response received"
    );
}

/// Log an HTTP failure without exposing a provider-generated error body.
pub(crate) fn log_http_error(
    protocol: &str,
    request_id: &str,
    status: reqwest::StatusCode,
    body: &str,
) {
    tracing::error!(
        protocol,
        request_id,
        %status,
        response_bytes = body.len(),
        "AI provider HTTP error (response body omitted)"
    );
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ai_api::AiError;

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

    #[test]
    fn map_http_error_status_codes() {
        assert!(matches!(
            map_http_error(reqwest::StatusCode::UNAUTHORIZED, "bad key"),
            AiError::InvalidApiKey
        ));
        assert!(matches!(
            map_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "slow down"),
            AiError::RateLimited { .. }
        ));
        let err = map_http_error(
            reqwest::StatusCode::BAD_REQUEST,
            "provider-secret-should-not-be-returned",
        );
        match err {
            AiError::RequestFailed(message) => {
                assert!(message.contains("HTTP 400"));
                assert!(message.contains("response body omitted"));
                assert!(message.contains("bytes"));
                assert!(!message.contains("provider-secret-should-not-be-returned"));
            }
            other => panic!("expected request failure, got {other:?}"),
        }
    }

    #[test]
    fn metadata_logs_never_render_request_or_error_payloads() {
        use std::fmt::Write as _;
        use std::sync::{Arc, Mutex};
        use tracing::{Event, Subscriber};
        use tracing_subscriber::layer::{Context, Layer};
        use tracing_subscriber::prelude::*;

        #[derive(Clone, Default)]
        struct Capture(Arc<Mutex<Vec<String>>>);

        struct Visitor(String);

        impl tracing::field::Visit for Visitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let _ = write!(self.0, "{}={value:?};", field.name());
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                let _ = write!(self.0, "{}={value};", field.name());
            }

            fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
                let _ = write!(self.0, "{}={value};", field.name());
            }

            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                let _ = write!(self.0, "{}={value};", field.name());
            }

            fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
                let _ = write!(self.0, "{}={value};", field.name());
            }
        }

        impl<S> Layer<S> for Capture
        where
            S: Subscriber,
        {
            fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
                let mut visitor = Visitor(String::new());
                event.record(&mut visitor);
                self.0.lock().unwrap().push(visitor.0);
            }
        }

        let events = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(Capture(events.clone()));
        let secret = "wiremock-secret-prompt-and-tool-args";
        let request = CompletionRequest {
            request_id: "request-1".into(),
            model: "test-model".into(),
            messages: vec![datazen_ai_api::ChatMessage {
                role: datazen_ai_api::MessageRole::User,
                content: secret.into(),
                reasoning: None,
                tool_calls: Some(vec![datazen_ai_api::ToolCall {
                    id: "call-1".into(),
                    name: "lookup".into(),
                    arguments: secret.into(),
                }]),
                tool_call_id: None,
            }],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
        };
        let body = serde_json::json!({
            "messages": [{"content": secret}],
            "tools": [{"function": {"arguments": secret}}]
        });

        tracing::subscriber::with_default(subscriber, || {
            log_request_metadata("test", &request, &body, false);
            log_response_metadata(
                "test",
                &request.request_id,
                reqwest::StatusCode::OK,
                secret.len(),
            );
            log_http_error(
                "test",
                &request.request_id,
                reqwest::StatusCode::BAD_REQUEST,
                secret,
            );
        });

        let output = events.lock().unwrap().join("\n");
        assert!(!output.contains(secret));
        assert!(output.contains("request_bytes"));
        assert!(output.contains("response_bytes"));
        assert!(output.contains("body omitted"));
    }
}
