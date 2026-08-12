//! Custom AI provider — user picks protocol + endpoint + API key.
//!
//! Delegates to the shared `protocol` layer; no protocol logic here.

use std::time::Duration;

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use super::protocol::{self, ProtocolConfig, CONNECT_TIMEOUT};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomProtocol {
    OpenAiCompatible,
    OpenAiResponses,
    AnthropicCompatible,
}

pub struct CustomProvider {
    http_client: HttpClient,
    state: RwLock<Option<CustomState>>,
}

struct CustomState {
    api_key: String,
    endpoint: String,
    protocol: CustomProtocol,
    max_tokens: u32,
}

impl CustomProvider {
    pub fn new() -> Self {
        Self {
            http_client: HttpClient::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .unwrap_or_default(),
            state: RwLock::new(None),
        }
    }

    fn parse_protocol(config: &AiProviderConfig) -> CustomProtocol {
        config
            .extra
            .get("protocol")
            .and_then(|v| serde_json::from_value::<CustomProtocol>(v.clone()).ok())
            .unwrap_or(CustomProtocol::OpenAiCompatible)
    }

    fn make_cfg(&self, state: &CustomState) -> ProtocolConfig {
        ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: state.endpoint.clone(),
            api_key: state.api_key.clone(),
            max_tokens: state.max_tokens,
        }
    }
}

// ─── Public API: fetch models from any provider ───

pub async fn fetch_remote_models(
    protocol: CustomProtocol,
    endpoint: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>, AiError> {
    tracing::info!(protocol = ?protocol, %endpoint, "fetch_remote_models: start");

    let http = HttpClient::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let cfg = ProtocolConfig {
        http_client: http,
        api_base: endpoint.into(),
        api_key: api_key.into(),
        max_tokens: 0,
    };

    match protocol {
        CustomProtocol::OpenAiCompatible | CustomProtocol::OpenAiResponses => {
            protocol::openai_chat::fetch_models(&cfg).await
        }
        CustomProtocol::AnthropicCompatible => protocol::anthropic::fetch_models(&cfg).await,
    }
}

// ─── AiProvider implementation ───

#[async_trait]
impl AiProvider for CustomProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Custom
    }

    fn display_name(&self) -> &str {
        "Custom"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        // Custom providers share Chat / Responses / Anthropic-compatible protocols
        // that already implement tool calling in `ai/protocol/`.
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required".into()))?;

        let endpoint = config
            .endpoint
            .as_deref()
            .filter(|e| !e.is_empty())
            .ok_or_else(|| AiError::NotConfigured("Endpoint URL is required".into()))?;

        if config.model.is_empty() {
            return Err(AiError::NotConfigured("Model is required".into()));
        }

        let proto = Self::parse_protocol(config);

        // Try model listing first; fall back to lightweight probe.
        match fetch_remote_models(proto, endpoint, key).await {
            Ok(_) => Ok(()),
            Err(_) => {
                let cfg = ProtocolConfig {
                    http_client: self.http_client.clone(),
                    api_base: endpoint.into(),
                    api_key: key.into(),
                    max_tokens: config.max_tokens,
                };
                match proto {
                    CustomProtocol::OpenAiCompatible => {
                        protocol::openai_chat::probe(&cfg, &config.model).await
                    }
                    CustomProtocol::OpenAiResponses => {
                        protocol::openai_responses::probe(&cfg, &config.model).await
                    }
                    CustomProtocol::AnthropicCompatible => {
                        protocol::anthropic::probe(&cfg, &config.model).await
                    }
                }
            }
        }
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let api_key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required".into()))?;

        let endpoint = config
            .endpoint
            .as_deref()
            .filter(|e| !e.is_empty())
            .ok_or_else(|| AiError::NotConfigured("Endpoint URL is required".into()))?;

        let proto = Self::parse_protocol(config);

        *self.state.write().await = Some(CustomState {
            api_key: api_key.to_string(),
            endpoint: endpoint.to_string(),
            protocol: proto,
            max_tokens: config.max_tokens,
        });

        Ok(())
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError> {
        let guard = self.state.read().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("Custom provider not initialized".into()))?;
        let cfg = self.make_cfg(state);

        match state.protocol {
            CustomProtocol::OpenAiCompatible => {
                protocol::openai_chat::complete(&cfg, request).await
            }
            CustomProtocol::OpenAiResponses => {
                protocol::openai_responses::complete(&cfg, request).await
            }
            CustomProtocol::AnthropicCompatible => {
                protocol::anthropic::complete(&cfg, request).await
            }
        }
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let guard = self.state.read().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("Custom provider not initialized".into()))?;
        let cfg = self.make_cfg(state);

        match state.protocol {
            CustomProtocol::OpenAiCompatible => {
                protocol::openai_chat::stream_complete(&cfg, request, sender).await
            }
            CustomProtocol::OpenAiResponses => {
                protocol::openai_responses::stream_complete(&cfg, request, sender).await
            }
            CustomProtocol::AnthropicCompatible => {
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

    #[test]
    fn test_custom_provider_metadata() {
        let provider = CustomProvider::new();
        assert_eq!(provider.provider_type(), AiProviderType::Custom);
        assert_eq!(provider.display_name(), "Custom");
        assert!(provider.supports_streaming());
    }

    #[test]
    fn test_parse_protocol_default() {
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        assert_eq!(
            CustomProvider::parse_protocol(&config),
            CustomProtocol::OpenAiCompatible
        );
    }

    #[test]
    fn test_parse_protocol_openai() {
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            max_tokens: 200_000,
            extra: serde_json::json!({ "protocol": "open_ai_compatible" }),
        };
        assert_eq!(
            CustomProvider::parse_protocol(&config),
            CustomProtocol::OpenAiCompatible
        );
    }

    #[test]
    fn test_parse_protocol_anthropic() {
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            max_tokens: 200_000,
            extra: serde_json::json!({ "protocol": "anthropic_compatible" }),
        };
        assert_eq!(
            CustomProvider::parse_protocol(&config),
            CustomProtocol::AnthropicCompatible
        );
    }

    #[tokio::test]
    async fn test_validate_requires_fields() {
        let provider = CustomProvider::new();

        let no_key = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: None,
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        assert!(matches!(
            provider.validate_config(&no_key).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));

        let no_endpoint = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: None,
            model: "model".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        assert!(matches!(
            provider.validate_config(&no_endpoint).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));

        let no_model = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: String::new(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        assert!(matches!(
            provider.validate_config(&no_model).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));
    }

    #[tokio::test]
    async fn test_complete_requires_init() {
        let provider = CustomProvider::new();
        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "model".into(),
            messages: vec![],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
        };
        assert!(matches!(
            provider.complete(&req).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));
    }

    #[tokio::test]
    async fn test_reset_clears_state() {
        let provider = CustomProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::Custom,
            api_key: Some("key".into()),
            endpoint: Some("http://example.com".into()),
            model: "model".into(),
            max_tokens: 200_000,
            extra: serde_json::json!({ "protocol": "open_ai_compatible" }),
        };
        provider.initialize(&config).await.unwrap();
        provider.reset().await;

        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "model".into(),
            messages: vec![],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
        };
        assert!(matches!(
            provider.complete(&req).await.unwrap_err(),
            AiError::NotConfigured(_)
        ));
    }

    #[test]
    fn test_protocol_serde() {
        let json = serde_json::to_string(&CustomProtocol::OpenAiCompatible).unwrap();
        assert_eq!(json, "\"open_ai_compatible\"");

        let json = serde_json::to_string(&CustomProtocol::OpenAiResponses).unwrap();
        assert_eq!(json, "\"open_ai_responses\"");

        let json = serde_json::to_string(&CustomProtocol::AnthropicCompatible).unwrap();
        assert_eq!(json, "\"anthropic_compatible\"");

        let p: CustomProtocol = serde_json::from_str("\"open_ai_compatible\"").unwrap();
        assert_eq!(p, CustomProtocol::OpenAiCompatible);

        let p: CustomProtocol = serde_json::from_str("\"open_ai_responses\"").unwrap();
        assert_eq!(p, CustomProtocol::OpenAiResponses);

        let p: CustomProtocol = serde_json::from_str("\"anthropic_compatible\"").unwrap();
        assert_eq!(p, CustomProtocol::AnthropicCompatible);
    }
}
