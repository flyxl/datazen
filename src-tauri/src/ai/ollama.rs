//! Ollama provider — OpenAI-compatible local endpoint (default `http://127.0.0.1:11434/v1`).

use async_trait::async_trait;
use datazen_ai_api::*;
use reqwest::Client as HttpClient;
use tokio::sync::{mpsc, RwLock};

use super::protocol::{self, ProtocolConfig, CONNECT_TIMEOUT};

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434/v1";

pub struct OllamaProvider {
    http_client: HttpClient,
    state: RwLock<Option<ProviderState>>,
}

struct ProviderState {
    api_key: String,
    endpoint: String,
    max_tokens: u32,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            http_client: HttpClient::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .unwrap_or_default(),
            state: RwLock::new(None),
        }
    }

    fn resolve_key(config: &AiProviderConfig) -> String {
        config
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .unwrap_or("ollama")
            .to_string()
    }

    async fn protocol_config(&self) -> Result<ProtocolConfig, AiError> {
        let guard = self.state.read().await;
        let s = guard
            .as_ref()
            .ok_or_else(|| AiError::NotConfigured("Ollama provider not initialized".into()))?;
        Ok(ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: s.endpoint.clone(),
            api_key: s.api_key.clone(),
            max_tokens: s.max_tokens,
        })
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Ollama
    }

    fn display_name(&self) -> &str {
        "Ollama"
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        if config.model.is_empty() {
            return Err(AiError::NotConfigured(
                "Model is required for Ollama (e.g. llama3.2)".into(),
            ));
        }
        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);
        let cfg = ProtocolConfig {
            http_client: self.http_client.clone(),
            api_base: endpoint.into(),
            api_key: Self::resolve_key(config),
            max_tokens: config.max_tokens,
        };
        protocol::openai_chat::fetch_models(&cfg).await?;
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        if config.model.is_empty() {
            return Err(AiError::NotConfigured(
                "Model is required for Ollama".into(),
            ));
        }
        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| DEFAULT_ENDPOINT.into());
        *self.state.write().await = Some(ProviderState {
            api_key: Self::resolve_key(config),
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
    fn provider_type_is_ollama() {
        assert_eq!(
            OllamaProvider::new().provider_type(),
            AiProviderType::Ollama
        );
    }

    #[test]
    fn resolve_key_defaults() {
        let cfg = AiProviderConfig {
            provider_type: AiProviderType::Ollama,
            api_key: None,
            endpoint: None,
            model: "llama3.2".into(),
            max_tokens: 4096,
            extra: serde_json::Value::Null,
        };
        assert_eq!(OllamaProvider::resolve_key(&cfg), "ollama");
    }
}
