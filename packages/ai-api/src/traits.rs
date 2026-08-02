//! Core AI provider trait.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::*;

/// Core trait for AI providers.
///
/// Mirrors the `DatabaseDriver` trait pattern: `Send + Sync`, `async_trait`,
/// optional methods with defaults. Each provider is a singleton managed by
/// `AiProviderRegistry`; it may hold internal HTTP clients or rate limiters.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Provider type identifier.
    fn provider_type(&self) -> AiProviderType;

    /// Human-readable provider name.
    fn display_name(&self) -> &str;

    /// Models this provider offers.
    fn available_models(&self) -> Vec<ModelInfo>;

    /// Default model ID.
    fn default_model(&self) -> &str;

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        false
    }

    /// Validate provider configuration (API key, endpoint, etc.) without
    /// making a full completion request.
    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError>;

    /// Initialize the provider with a saved configuration. Creates internal
    /// HTTP clients, caches model lists, etc. Analogous to `DatabaseDriver::connect`.
    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError>;

    /// Non-streaming completion request.
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError>;

    /// Streaming completion. Sends chunks through the `mpsc::Sender`.
    /// Default implementation falls back to a single non-streaming call.
    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let response = self.complete(request).await?;
        let _ = sender
            .send(Ok(StreamChunk {
                content: response.content,
                reasoning: None,
                done: true,
                usage: Some(response.usage),
            }))
            .await;
        Ok(())
    }

    /// Cancel a running request. Optional capability.
    async fn cancel(&self, _request_id: &str) -> Result<(), AiError> {
        Err(AiError::NotSupported("cancel".into()))
    }

    /// Reset internal state (clear HTTP clients, cached keys, etc.).
    /// Called when the user deletes their AI configuration.
    async fn reset(&self) {}
}
