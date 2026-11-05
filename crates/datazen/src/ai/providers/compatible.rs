//! Generic OpenAI-compatible / Anthropic-compatible provider presets.
//!
//! The wire formats live in [`super::protocol`]. This file only binds a preset's
//! identity, default endpoint and protocol to that shared transport, so adding
//! a vendor that speaks a compatible API never duplicates request logic.

use std::time::Duration;

use crate::ai::http::HttpClient;
use crate::ai::*;
use async_trait::async_trait;
use tokio::sync::{mpsc, RwLock};

use super::metadata::provider_defaults;
use super::protocol::{self, ProtocolConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompatibleKind {
    OpenAiChat,
    Anthropic,
}

pub struct CompatibleProvider {
    http_client: HttpClient,
    provider_type: AiProviderType,
    display_name: &'static str,
    default_endpoint: &'static str,
    kind: CompatibleKind,
    state: RwLock<Option<CompatibleState>>,
}

struct CompatibleState {
    api_key: String,
    endpoint: String,
    max_tokens: u32,
}

impl CompatibleProvider {
    /// Build one of the generic presets, or `None` when the type has a dedicated
    /// vendor adapter.
    pub fn new(provider_type: AiProviderType) -> Option<Self> {
        Self::with_http_client(provider_type, HttpClient::default())
    }

    pub fn with_http_client(
        provider_type: AiProviderType,
        http_client: HttpClient,
    ) -> Option<Self> {
        let (display_name, default_endpoint, kind) = match provider_type {
            AiProviderType::Gemini => (
                "Google Gemini",
                provider_defaults(provider_type).0,
                CompatibleKind::OpenAiChat,
            ),
            AiProviderType::Qwen => (
                "Qwen",
                provider_defaults(provider_type).0,
                CompatibleKind::OpenAiChat,
            ),
            AiProviderType::MiniMax => (
                "MiniMax",
                provider_defaults(provider_type).0,
                CompatibleKind::OpenAiChat,
            ),
            AiProviderType::OpenAiCompatible => (
                "OpenAI Compatible",
                provider_defaults(provider_type).0,
                CompatibleKind::OpenAiChat,
            ),
            AiProviderType::AnthropicCompatible => (
                "Anthropic Compatible",
                provider_defaults(provider_type).0,
                CompatibleKind::Anthropic,
            ),
            _ => return None,
        };
        Some(Self {
            http_client,
            provider_type,
            display_name,
            default_endpoint,
            kind,
            state: RwLock::new(None),
        })
    }

    fn endpoint(&self, config: &AiProviderConfig) -> Result<String, AiError> {
        if let Some(endpoint) = config
            .endpoint
            .as_deref()
            .filter(|endpoint| !endpoint.trim().is_empty())
        {
            return Ok(endpoint.to_string());
        }
        if self.default_endpoint.is_empty() {
            return Err(AiError::NotConfigured(
                "Endpoint URL is required for this provider".into(),
            ));
        }
        Ok(self.default_endpoint.to_string())
    }

    fn api_key<'a>(&self, config: &'a AiProviderConfig) -> Result<&'a str, AiError> {
        config
            .api_key
            .as_deref()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                AiError::NotConfigured(format!("API key is required for {}", self.display_name))
            })
    }

    fn protocol_config(state: &CompatibleState, http_client: HttpClient) -> ProtocolConfig {
        ProtocolConfig {
            http_client,
            api_base: state.endpoint.clone(),
            api_key: state.api_key.clone(),
            max_tokens: state.max_tokens,
            max_request_timeout: Duration::from_secs(120),
        }
    }

    async fn initialized_config(&self) -> Result<ProtocolConfig, AiError> {
        let guard = self.state.read().await;
        let state = guard.as_ref().ok_or_else(|| {
            AiError::NotConfigured(format!("{} provider not initialized", self.display_name))
        })?;
        Ok(Self::protocol_config(state, self.http_client.clone()))
    }
}

#[async_trait]
impl AiProvider for CompatibleProvider {
    fn provider_type(&self) -> AiProviderType {
        self.provider_type
    }

    fn display_name(&self) -> &str {
        self.display_name
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let key = self.api_key(config)?;
        if config.model.trim().is_empty() {
            return Err(AiError::NotConfigured(format!(
                "Model is required for {}",
                self.display_name
            )));
        }
        let cfg = ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: self.endpoint(config)?,
            api_key: key.into(),
            max_tokens: config.max_tokens,
            max_request_timeout: Duration::from_secs(120),
        };
        match self.kind {
            CompatibleKind::OpenAiChat => {
                protocol::openai_chat::fetch_models(&cfg).await?;
            }
            CompatibleKind::Anthropic => {
                protocol::anthropic::fetch_models(&cfg).await?;
            }
        }
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let api_key = self.api_key(config)?.to_string();
        let endpoint = self.endpoint(config)?;
        *self.state.write().await = Some(CompatibleState {
            api_key,
            endpoint,
            max_tokens: config.max_tokens,
        });
        Ok(())
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError> {
        let cfg = self.initialized_config().await?;
        match self.kind {
            CompatibleKind::OpenAiChat => protocol::openai_chat::complete(&cfg, request).await,
            CompatibleKind::Anthropic => protocol::anthropic::complete(&cfg, request).await,
        }
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let cfg = self.initialized_config().await?;
        match self.kind {
            CompatibleKind::OpenAiChat => {
                protocol::openai_chat::stream_complete(&cfg, request, sender).await
            }
            CompatibleKind::Anthropic => {
                protocol::anthropic::stream_complete(&cfg, request, sender).await
            }
        }
    }

    async fn reset(&self) {
        *self.state.write().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::protocol::test_support::sample_request;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config(provider_type: AiProviderType, endpoint: String, key: &str) -> AiProviderConfig {
        AiProviderConfig {
            provider_type,
            api_key: Some(key.into()),
            endpoint: Some(endpoint),
            model: "test-model".into(),
            max_tokens: 4_096,
            extra: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn each_preset_uses_its_protocol_and_bearer_auth() {
        for provider_type in [
            AiProviderType::Gemini,
            AiProviderType::Qwen,
            AiProviderType::MiniMax,
            AiProviderType::OpenAiCompatible,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .and(header("Authorization", "Bearer preset-key"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
                    "model": "test-model"
                })))
                .mount(&server)
                .await;

            let provider =
                CompatibleProvider::with_http_client(provider_type, HttpClient::desktop()).unwrap();
            provider
                .initialize(&config(
                    provider_type,
                    format!("{}/v1", server.uri()),
                    "preset-key",
                ))
                .await
                .unwrap();
            let response = provider.complete(&sample_request()).await.unwrap();
            assert_eq!(response.content, "ok");
            assert_eq!(provider.provider_type(), provider_type);
        }
    }

    #[tokio::test]
    async fn anthropic_compatible_uses_x_api_key_messages() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "anthropic-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg_1",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "ok"}],
                "model": "test-model",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 1, "output_tokens": 1}
            })))
            .mount(&server)
            .await;

        let provider = CompatibleProvider::with_http_client(
            AiProviderType::AnthropicCompatible,
            HttpClient::desktop(),
        )
        .unwrap();
        provider
            .initialize(&config(
                AiProviderType::AnthropicCompatible,
                server.uri(),
                "anthropic-key",
            ))
            .await
            .unwrap();
        let response = provider.complete(&sample_request()).await.unwrap();
        assert_eq!(response.content, "ok");
    }

    #[test]
    fn preset_metadata_and_required_endpoint_are_stable() {
        let gemini = CompatibleProvider::new(AiProviderType::Gemini).unwrap();
        assert_eq!(gemini.display_name(), "Google Gemini");
        assert!(gemini.supports_tools());
        let compatible = CompatibleProvider::new(AiProviderType::OpenAiCompatible).unwrap();
        assert!(compatible.default_endpoint.is_empty());
        assert!(CompatibleProvider::new(AiProviderType::OpenAi).is_none());
    }

    #[test]
    fn provider_source_never_logs_the_api_key() {
        let source = include_str!("compatible.rs");
        for line in source.lines().filter(|line| line.contains("tracing::")) {
            assert!(
                !line.contains("api_key"),
                "API keys must never be logged: {line}"
            );
        }
    }
}
