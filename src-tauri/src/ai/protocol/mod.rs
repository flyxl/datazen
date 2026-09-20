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
    /// Total request timeout for non-streaming calls (connect + read).
    /// Defaults to 120s if not set.
    pub max_request_timeout: Duration,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            http_client: HttpClient::new(),
            api_base: String::new(),
            api_key: String::new(),
            max_tokens: 4096,
            max_request_timeout: Duration::from_secs(120),
        }
    }
}

/// Retry configuration for transient HTTP errors (429 rate-limit).
pub struct RetryConfig {
    /// Maximum number of retries (0 = no retries).
    pub max_retries: u32,
    /// Base delay before first retry (doubled each attempt).
    pub base_delay: Duration,
    /// Maximum delay cap (exponential backoff stops growing at this value).
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        }
    }
}

/// Stream chunk read timeout (per-chunk, not total).
pub const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(120);

/// Connect timeout for new HTTP clients.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Default total request timeout (used when `ProtocolConfig.max_request_timeout` is not set).
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

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
///
/// For 429 responses, the `Retry-After` header value is passed via
/// `retry_after_secs` if available, otherwise a 60s default is used.
/// For 400 responses, the user-facing message is extracted and sanitized
/// so that raw API error payloads are never exposed to the caller.
pub(crate) fn map_http_error(status: reqwest::StatusCode, body: &str) -> datazen_ai_api::AiError {
    match status.as_u16() {
        401 => datazen_ai_api::AiError::InvalidApiKey,
        400 => datazen_ai_api::AiError::RequestFailed(sanitize_400_error(body)),
        429 => datazen_ai_api::AiError::RateLimited {
            retry_after_secs: parse_retry_after(body).unwrap_or(60),
        },
        _ => datazen_ai_api::AiError::RequestFailed(format!(
            "HTTP {status}: provider request failed (response body omitted; {} bytes)",
            body.len()
        )),
    }
}

/// Sanitize a 400 Bad Request response body into a user-facing message.
///
/// Extracts the `error.message` field from standard OpenAI/Anthropic error
/// envelopes. Falls back to a generic message that never exposes the raw
/// response body, API key fragments, or internal server details.
pub(crate) fn sanitize_400_error(body: &str) -> String {
    // BUG-15: Merged duplicate branches — both OpenAI and Anthropic use { "error": { "message": "..." } }.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            let msg = msg.trim();
            if !msg.is_empty() {
                return truncate_user_message(msg);
            }
        }
    }

    "Bad request: the server could not process the request".to_string()
}

/// Truncate a user-facing error message to a reasonable length to prevent
/// extremely long provider error text from being forwarded verbatim.
fn truncate_user_message(msg: &str) -> String {
    const MAX_LEN: usize = 500;
    if msg.len() <= MAX_LEN {
        msg.to_string()
    } else {
        // BUG-04: Find the last char boundary at or before MAX_LEN to avoid UTF-8 panic.
        let end = msg
            .char_indices()
            .take_while(|(i, _)| *i < MAX_LEN)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(MAX_LEN);
        format!("{}…", &msg[..end])
    }
}

/// Extract the `Retry-After` value from a 429 response body.
///
/// Checks for `error.retry_after` (numeric seconds) or a string `retry_after`
/// field. Returns `None` if the header is absent or unparseable.
pub(crate) fn parse_retry_after(body: &str) -> Option<u64> {
    let json = serde_json::from_str::<serde_json::Value>(body).ok()?;
    // Check error.retry_after (numeric)
    if let Some(val) = json.get("error").and_then(|e| e.get("retry_after")) {
        if let Some(secs) = val.as_u64() {
            return Some(secs);
        }
        // Some providers return retry_after as a string
        if let Some(s) = val.as_str() {
            if let Ok(secs) = s.parse::<u64>() {
                return Some(secs);
            }
        }
    }
    // Top-level retry_after
    if let Some(val) = json.get("retry_after") {
        if let Some(secs) = val.as_u64() {
            return Some(secs);
        }
        if let Some(s) = val.as_str() {
            if let Ok(secs) = s.parse::<u64>() {
                return Some(secs);
            }
        }
    }
    None
}

/// Execute an async operation with 429 exponential backoff retry.
///
/// On 429 responses the operation is retried with exponential backoff
/// (base_delay × 2^attempt + random jitter) up to `retry.max_retries` times.
/// The jitter is ±25% of the computed delay.
///
/// Non-retryable errors (including 400, 401, and other non-429 failures)
/// are returned immediately without retry.
pub(crate) async fn retry_with_backoff<F, Fut, T>(
    retry: &RetryConfig,
    mut op: F,
) -> Result<T, datazen_ai_api::AiError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, datazen_ai_api::AiError>>,
{
    let mut attempt: u32 = 0;
    loop {
        match op().await {
            Ok(val) => return Ok(val),
            Err(datazen_ai_api::AiError::RateLimited { retry_after_secs }) => {
                if attempt >= retry.max_retries {
                    tracing::warn!(
                        attempt,
                        max = retry.max_retries,
                        "retry_with_backoff: max retries exhausted"
                    );
                    return Err(datazen_ai_api::AiError::RateLimited { retry_after_secs });
                }
                let delay = compute_backoff_delay(retry, attempt, retry_after_secs);
                tracing::info!(
                    attempt,
                    max = retry.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    retry_after_secs,
                    "retry_with_backoff: 429 received, backing off"
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(other) => return Err(other),
        }
    }
}

/// Compute the backoff delay for a given attempt.
///
/// Uses `max(retry_after_secs, base_delay × 2^attempt)` capped at `max_delay`,
/// then adds ±25% random jitter.
fn compute_backoff_delay(retry: &RetryConfig, attempt: u32, retry_after_secs: u64) -> Duration {
    let base = retry.base_delay.as_secs().max(1);
    let computed = base.saturating_mul(1u64 << attempt.min(10));
    let delay_secs = computed.max(retry_after_secs);
    let max_secs = retry.max_delay.as_secs();
    let capped = delay_secs.min(max_secs);
    // Add ±25% jitter
    let jitter_range = capped.max(1) / 4;
    let jitter = if jitter_range > 0 {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let seed = RandomState::new();
        let h = seed.build_hasher();
        let nanos = h.finish();
        (nanos % (jitter_range * 2 + 1)) as i64 - jitter_range as i64
    } else {
        0
    };
    let final_secs = (capped as i64 + jitter).max(0) as u64;
    Duration::from_secs(final_secs)
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
    tracing::info!(
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
    tracing::info!(
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
    use std::sync::Arc;

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
        // 400 now produces a sanitized user-facing message
        let err = map_http_error(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"model not found: gpt-999"}}"#,
        );
        match err {
            AiError::RequestFailed(message) => {
                assert!(message.contains("model not found"));
                assert!(!message.contains("gpt-999") || message.len() < 200);
            }
            other => panic!("expected request failure, got {other:?}"),
        }
        // 400 with non-JSON body still works
        let err = map_http_error(reqwest::StatusCode::BAD_REQUEST, "raw body here");
        match err {
            AiError::RequestFailed(message) => {
                assert!(message.contains("Bad request"));
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
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::filter::LevelFilter::INFO)
            .with(Capture(events.clone()));
        let prompt_secret = "wiremock-secret-prompt";
        let tool_args_secret = "wiremock-secret-tool-args";
        let http_body_secret = "wiremock-secret-http-body";
        let request = CompletionRequest {
            request_id: "request-1".into(),
            model: "test-model".into(),
            messages: vec![datazen_ai_api::ChatMessage {
                role: datazen_ai_api::MessageRole::User,
                content: prompt_secret.into(),
                reasoning: None,
                tool_calls: Some(vec![datazen_ai_api::ToolCall {
                    id: "call-1".into(),
                    name: "lookup".into(),
                    arguments: tool_args_secret.into(),
                }]),
                tool_call_id: None,
            }],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
            cancel_token: None,
        };
        let body = serde_json::json!({
            "messages": [{"content": http_body_secret}],
            "tools": [{"function": {"arguments": http_body_secret}}]
        });

        tracing::subscriber::with_default(subscriber, || {
            log_request_metadata("test", &request, &body, false);
            log_response_metadata(
                "test",
                &request.request_id,
                reqwest::StatusCode::OK,
                http_body_secret.len(),
            );
            log_http_error(
                "test",
                &request.request_id,
                reqwest::StatusCode::BAD_REQUEST,
                http_body_secret,
            );
        });

        let output = events.lock().unwrap().join("\n");
        assert!(!output.contains(prompt_secret));
        assert!(!output.contains(tool_args_secret));
        assert!(!output.contains(http_body_secret));
        assert!(
            output.contains("request_bytes"),
            "request metadata was not captured: {output}"
        );
        assert!(
            output.contains("response_bytes"),
            "response metadata was not captured: {output}"
        );
        assert!(output.contains("body omitted"));
    }

    #[test]
    fn sanitize_400_openai_envelope() {
        let body = r#"{"error":{"message":"Model 'gpt-999' does not exist","type":"invalid_request_error","param":null,"code":"model_not_found"}}"#;
        let msg = sanitize_400_error(body);
        assert!(msg.contains("Model 'gpt-999' does not exist"));
    }

    #[test]
    fn sanitize_400_anthropic_envelope() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"Invalid model id"}}"#;
        let msg = sanitize_400_error(body);
        assert!(msg.contains("Invalid model id"));
    }

    #[test]
    fn sanitize_400_plain_text_fallback() {
        let msg = sanitize_400_error("some raw error text");
        assert_eq!(msg, "Bad request: the server could not process the request");
    }

    #[test]
    fn sanitize_400_empty_json_fallback() {
        let msg = sanitize_400_error("{}");
        assert_eq!(msg, "Bad request: the server could not process the request");
    }

    #[test]
    fn sanitize_400_truncates_long_message() {
        let long_msg = "x".repeat(1000);
        let body = serde_json::json!({"error": {"message": long_msg}});
        let msg = sanitize_400_error(&body.to_string());
        assert!(msg.len() <= 501); // 500 chars + ellipsis
        assert!(msg.ends_with('…'));
    }

    #[test]
    fn parse_retry_after_numeric() {
        let body = r#"{"error":{"message":"rate limited","retry_after":30}}"#;
        assert_eq!(parse_retry_after(body), Some(30));
    }

    #[test]
    fn parse_retry_after_string() {
        let body = r#"{"error":{"message":"rate limited","retry_after":"5"}}"#;
        assert_eq!(parse_retry_after(body), Some(5));
    }

    #[test]
    fn parse_retry_after_top_level() {
        let body = r#"{"retry_after":10,"message":"slow down"}"#;
        assert_eq!(parse_retry_after(body), Some(10));
    }

    #[test]
    fn parse_retry_after_missing() {
        let body = r#"{"error":{"message":"rate limited"}}"#;
        assert_eq!(parse_retry_after(body), None);
    }

    #[test]
    fn parse_retry_after_invalid_json() {
        assert_eq!(parse_retry_after("not json"), None);
    }

    #[test]
    fn map_http_error_429_uses_retry_after() {
        let body = r#"{"error":{"message":"rate limited","retry_after":42}}"#;
        match map_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body) {
            AiError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 42);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn map_http_error_429_no_retry_after_defaults_to_60() {
        match map_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "{}") {
            AiError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 60);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn compute_backoff_delay_respects_max_delay() {
        let retry = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        };
        // attempt=10 would be 1024s without cap
        let delay = compute_backoff_delay(&retry, 10, 0);
        // With cap at 10s ±25% jitter, delay should be ≤ 12.5s
        assert!(delay <= Duration::from_secs(13));
    }

    #[test]
    fn compute_backoff_delay_respects_retry_after_floor() {
        let retry = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
        };
        // retry_after_secs=30 should be used even if computed backoff is smaller
        let delay = compute_backoff_delay(&retry, 0, 30);
        // 30 ±25% = 22.5..37.5
        assert!(delay >= Duration::from_secs(22));
        assert!(delay <= Duration::from_secs(38));
    }

    #[tokio::test]
    async fn retry_with_backoff_retries_on_429() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let retry = RetryConfig {
            max_retries: 2,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
        };

        let result = retry_with_backoff(&retry, || {
            let cc = call_count_clone.clone();
            async move {
                let n = cc.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(AiError::RateLimited {
                        retry_after_secs: 0,
                    })
                } else {
                    Ok("success")
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_with_backoff_exhausts_retries() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let retry = RetryConfig {
            max_retries: 2,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
        };

        let result: Result<(), AiError> = retry_with_backoff(&retry, || {
            let cc = call_count_clone.clone();
            async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Err(AiError::RateLimited {
                    retry_after_secs: 0,
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AiError::RateLimited { .. }));
        // 1 initial + 2 retries = 3 calls
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_with_backoff_does_not_retry_other_errors() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let retry = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
        };

        let result: Result<(), AiError> = retry_with_backoff(&retry, || {
            let cc = call_count_clone.clone();
            async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Err(AiError::InvalidApiKey)
            }
        })
        .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AiError::InvalidApiKey));
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
