//! OpenAI AI provider — delegates to `protocol::openai_chat`.

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client as HttpClient;
use tokio::sync::{mpsc, RwLock};

use super::protocol::{self, ProtocolConfig, CONNECT_TIMEOUT};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";

pub struct OpenAiProvider {
    http_client: HttpClient,
    state: RwLock<Option<ProviderState>>,
}

struct ProviderState {
    api_key: String,
    endpoint: String,
    max_tokens: u32,
}

impl OpenAiProvider {
    pub fn new() -> Self {
        Self {
            http_client: HttpClient::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .unwrap_or_default(),
            state: RwLock::new(None),
        }
    }

    async fn protocol_config(&self) -> Result<ProtocolConfig, AiError> {
        let guard = self.state.read().await;
        let s = guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("OpenAI provider not initialized".into()))?;
        Ok(ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: s.endpoint.clone(),
            api_key: s.api_key.clone(),
            max_tokens: s.max_tokens,
        })
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::OpenAi
    }

    fn display_name(&self) -> &str {
        "OpenAI"
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required for OpenAI".into()))?;

        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);
        let cfg = ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: endpoint.into(),
            api_key: key.into(),
            max_tokens: config.max_tokens,
        };
        protocol::openai_chat::fetch_models(&cfg).await?;
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let api_key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required for OpenAI".into()))?;

        let endpoint = config
            .endpoint
            .as_deref()
            .unwrap_or(DEFAULT_ENDPOINT)
            .to_string();

        *self.state.write().await = Some(ProviderState {
            api_key: api_key.to_string(),
            endpoint,
            max_tokens: config.max_tokens,
        });

        Ok(())
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError> {
        let cfg = self.protocol_config().await?;
        protocol::openai_chat::complete(&cfg, request).await
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let cfg = self.protocol_config().await?;
        protocol::openai_chat::stream_complete(&cfg, request, sender).await
    }

    async fn reset(&self) {
        *self.state.write().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_metadata() {
        let provider = OpenAiProvider::new();
        assert_eq!(provider.provider_type(), AiProviderType::OpenAi);
        assert_eq!(provider.display_name(), "OpenAI");
        assert!(provider.supports_streaming());
        assert!(provider.supports_tools());
    }

    #[tokio::test]
    async fn test_validate_config_requires_key() {
        let provider = OpenAiProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: None,
            endpoint: None,
            model: "gpt-4o".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        let err = provider.validate_config(&config).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn test_complete_requires_init() {
        let provider = OpenAiProvider::new();
        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "gpt-4o".into(),
            messages: vec![],
            temperature: None,
            stop: None,
        };
        let err = provider.complete(&req).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn test_reset_clears_state() {
        let provider = OpenAiProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: Some("sk-test".into()),
            endpoint: None,
            model: "gpt-4o".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        provider.initialize(&config).await.unwrap();
        provider.reset().await;

        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "gpt-4o".into(),
            messages: vec![],
            temperature: None,
            stop: None,
        };
        let err = provider.complete(&req).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }
}
