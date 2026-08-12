//! DeepSeek AI provider — delegates to `protocol::openai_responses`.

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client as HttpClient;
use tokio::sync::{mpsc, RwLock};

use super::protocol::{self, ProtocolConfig, CONNECT_TIMEOUT};

const DEFAULT_ENDPOINT: &str = "https://api.deepseek.com";

pub struct DeepSeekProvider {
    http_client: HttpClient,
    config: RwLock<Option<AiProviderConfig>>,
}

impl DeepSeekProvider {
    pub fn new() -> Self {
        Self {
            http_client: HttpClient::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .unwrap_or_default(),
            config: RwLock::new(None),
        }
    }

    async fn protocol_config(&self) -> Result<ProtocolConfig, AiError> {
        let guard = self.config.read().await;
        let config = guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("DeepSeek provider not initialized".into()))?;

        let api_key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required for DeepSeek".into()))?;

        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);

        Ok(ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: endpoint.into(),
            api_key: api_key.into(),
            max_tokens: config.max_tokens,
        })
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError> {
        let cfg = self.protocol_config().await?;
        protocol::openai_chat::fetch_models(&cfg).await
    }
}

#[async_trait]
impl AiProvider for DeepSeekProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::DeepSeek
    }

    fn display_name(&self) -> &str {
        "DeepSeek"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let key = config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required for DeepSeek".into()))?;

        if config.model.is_empty() {
            return Err(AiError::NotConfigured(
                "Model is required for DeepSeek".into(),
            ));
        }

        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);
        let cfg = ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: endpoint.into(),
            api_key: key.into(),
            max_tokens: config.max_tokens,
        };
        protocol::openai_responses::probe(&cfg, &config.model).await
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AiError::NotConfigured("API key is required for DeepSeek".into()))?;

        *self.config.write().await = Some(config.clone());
        Ok(())
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError> {
        let cfg = self.protocol_config().await?;
        protocol::openai_responses::complete(&cfg, request).await
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let cfg = self.protocol_config().await?;
        protocol::openai_responses::stream_complete(&cfg, request, sender).await
    }

    async fn reset(&self) {
        *self.config.write().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deepseek_provider_metadata() {
        let provider = DeepSeekProvider::new();
        assert_eq!(provider.provider_type(), AiProviderType::DeepSeek);
        assert_eq!(provider.display_name(), "DeepSeek");
        assert!(provider.supports_streaming());
        assert!(provider.supports_tools());
    }

    #[tokio::test]
    async fn test_validate_config_requires_key() {
        let provider = DeepSeekProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::DeepSeek,
            api_key: None,
            endpoint: None,
            model: "deepseek-chat".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        let err = provider.validate_config(&config).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn test_validate_config_requires_model() {
        let provider = DeepSeekProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::DeepSeek,
            api_key: Some("sk-test".into()),
            endpoint: None,
            model: String::new(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        let err = provider.validate_config(&config).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn test_complete_requires_init() {
        let provider = DeepSeekProvider::new();
        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "deepseek-chat".into(),
            messages: vec![],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
        };
        let err = provider.complete(&req).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn test_reset_clears_state() {
        let provider = DeepSeekProvider::new();
        let config = AiProviderConfig {
            provider_type: AiProviderType::DeepSeek,
            api_key: Some("sk-test".into()),
            endpoint: None,
            model: "deepseek-chat".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        provider.initialize(&config).await.unwrap();
        provider.reset().await;

        let req = CompletionRequest {
            request_id: "r1".into(),
            model: "deepseek-chat".into(),
            messages: vec![],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
        };
        let err = provider.complete(&req).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }
}
