//! AI Provider registry — resolves `AiProviderType` to a concrete `AiProvider`.

use datazen_ai_api::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use super::anthropic::AnthropicProvider;
use super::custom::CustomProvider;
use super::deepseek::DeepSeekProvider;
use super::ollama::OllamaProvider;
use super::openai::OpenAiProvider;

pub struct AiProviderRegistry {
    http_client: crate::http::HttpClient,
    providers: Arc<RwLock<HashMap<AiProviderType, Arc<dyn AiProvider>>>>,
    registered: AtomicBool,
    register_lock: Mutex<()>,
}

impl AiProviderRegistry {
    pub fn new() -> Self {
        Self::with_http_client(crate::http::HttpClient::default())
    }

    pub fn with_http_client(http_client: crate::http::HttpClient) -> Self {
        Self {
            http_client,
            providers: Arc::new(RwLock::new(HashMap::new())),
            registered: AtomicBool::new(false),
            register_lock: Mutex::new(()),
        }
    }

    /// Register built-in + external providers.
    /// Safe to call repeatedly; only the first call does work.
    ///
    /// Returns `true` when this call performed that one-time registration. The
    /// caller uses the flag to apply persisted config exactly once, preserving the
    /// pre-split behaviour where registration and config restore were one step.
    pub async fn ensure_registered(&self) -> bool {
        if self.registered.load(Ordering::Acquire) {
            return false;
        }
        let _guard = self.register_lock.lock().await;
        if self.registered.load(Ordering::Acquire) {
            return false;
        }

        let t = std::time::Instant::now();
        register_ai_providers(self).await;
        self.registered.store(true, Ordering::Release);
        tracing::info!("[startup] AI providers ready: {:?}", t.elapsed());
        true
    }

    /// Re-apply a persisted provider config to the matching provider.
    ///
    /// Deliberately separate from [`Self::ensure_registered`]: the caller owns config
    /// persistence (encrypted store, settings import) and hands the loaded config in, so
    /// this crate keeps no dependency on any host storage type.
    pub async fn restore_config(&self, config: &AiProviderConfig) {
        if let Some(provider) = self.get(&config.provider_type).await {
            if let Err(e) = provider.initialize(config).await {
                tracing::warn!("Failed to initialize saved AI provider: {e}");
            }
        }
    }

    pub async fn register(&self, provider: Arc<dyn AiProvider>) {
        let mut providers = self.providers.write().await;
        providers.insert(provider.provider_type(), provider);
    }

    pub async fn get(&self, provider_type: &AiProviderType) -> Option<Arc<dyn AiProvider>> {
        let providers = self.providers.read().await;
        providers.get(provider_type).cloned()
    }

    pub async fn available_providers(&self) -> Vec<AiProviderType> {
        let providers = self.providers.read().await;
        providers.keys().cloned().collect()
    }

    pub async fn list_providers(&self) -> Vec<(AiProviderType, String)> {
        let providers = self.providers.read().await;
        providers
            .iter()
            .map(|(t, p)| (*t, p.display_name().to_string()))
            .collect()
    }

    pub async fn all_providers(&self) -> Vec<Arc<dyn AiProvider>> {
        let providers = self.providers.read().await;
        providers.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_new_is_empty() {
        let registry = AiProviderRegistry::new();
        assert!(registry.available_providers().await.is_empty());
        assert!(registry.all_providers().await.is_empty());
        assert!(registry.list_providers().await.is_empty());
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = AiProviderRegistry::new();
        registry
            .register(Arc::new(OpenAiProvider::with_http_client(
                crate::http::HttpClient::desktop(),
            )))
            .await;

        let provider = registry.get(&AiProviderType::OpenAi).await;
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().display_name(), "OpenAI");

        assert!(registry.get(&AiProviderType::Anthropic).await.is_none());
    }

    #[tokio::test]
    async fn test_register_overwrites() {
        let registry = AiProviderRegistry::new();
        registry
            .register(Arc::new(OpenAiProvider::with_http_client(
                crate::http::HttpClient::desktop(),
            )))
            .await;
        registry
            .register(Arc::new(OpenAiProvider::with_http_client(
                crate::http::HttpClient::desktop(),
            )))
            .await;

        assert_eq!(registry.available_providers().await.len(), 1);
    }

    #[tokio::test]
    async fn test_available_providers() {
        let registry = AiProviderRegistry::new();
        registry
            .register(Arc::new(OpenAiProvider::with_http_client(
                crate::http::HttpClient::desktop(),
            )))
            .await;
        registry.register(Arc::new(AnthropicProvider::new())).await;

        let types = registry.available_providers().await;
        assert_eq!(types.len(), 2);
        assert!(types.contains(&AiProviderType::OpenAi));
        assert!(types.contains(&AiProviderType::Anthropic));
    }

    #[tokio::test]
    async fn test_list_providers() {
        let registry = AiProviderRegistry::new();
        registry.register(Arc::new(AnthropicProvider::new())).await;

        let list = registry.list_providers().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, AiProviderType::Anthropic);
        assert_eq!(list[0].1, "Anthropic (Claude)");
    }

    #[tokio::test]
    async fn test_init_ai_providers_has_builtins() {
        let registry = init_ai_providers().await;

        let types = registry.available_providers().await;
        assert!(types.contains(&AiProviderType::OpenAi));
        assert!(types.contains(&AiProviderType::Anthropic));
        assert!(types.contains(&AiProviderType::DeepSeek));
        assert!(types.len() >= 3);
    }

    #[tokio::test]
    async fn test_all_providers() {
        let registry = init_ai_providers().await;
        let all = registry.all_providers().await;
        assert!(all.len() >= 2);
    }

    #[tokio::test]
    async fn ensure_registered_reports_the_one_time_registration_only_once() {
        let registry = AiProviderRegistry::new();
        assert!(registry.ensure_registered().await);
        assert!(!registry.ensure_registered().await);

        // Built-ins are keyed by provider type, so a repeated registration would be
        // invisible in the map — the flag is the only observable difference.
        let types = registry.available_providers().await;
        assert!(types.len() >= 5);
        assert!(types.contains(&AiProviderType::Ollama));
    }

    #[tokio::test]
    async fn restore_config_initializes_the_matching_provider() {
        use super::super::protocol::test_support::sample_request;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer restored-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": "Restored"}, "finish_reason": "stop"}],
                "model": "gpt-test"
            })))
            .mount(&server)
            .await;

        let registry = AiProviderRegistry::new();
        registry
            .register(Arc::new(OpenAiProvider::with_http_client(
                crate::http::HttpClient::desktop(),
            )))
            .await;
        registry
            .restore_config(&AiProviderConfig {
                provider_type: AiProviderType::OpenAi,
                api_key: Some("restored-key".into()),
                endpoint: Some(format!("{}/v1", server.uri())),
                model: "gpt-test".into(),
                max_tokens: 200_000,
                extra: serde_json::Value::Null,
            })
            .await;

        let provider = registry.get(&AiProviderType::OpenAi).await.unwrap();
        let resp = provider.complete(&sample_request()).await.unwrap();
        assert_eq!(resp.content, "Restored");
    }

    #[tokio::test]
    async fn restore_config_survives_an_invalid_config() {
        use super::super::protocol::test_support::sample_request;

        let registry = AiProviderRegistry::new();
        registry
            .register(Arc::new(OpenAiProvider::with_http_client(
                crate::http::HttpClient::desktop(),
            )))
            .await;
        // No API key -> initialize() fails -> the failure is logged, not propagated,
        // and the provider stays uninitialized (same as the pre-split startup path).
        registry
            .restore_config(&AiProviderConfig {
                provider_type: AiProviderType::OpenAi,
                api_key: None,
                endpoint: None,
                model: "gpt-test".into(),
                max_tokens: 200_000,
                extra: serde_json::Value::Null,
            })
            .await;

        let provider = registry.get(&AiProviderType::OpenAi).await.unwrap();
        let err = provider.complete(&sample_request()).await.unwrap_err();
        assert!(matches!(err, AiError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn restore_config_ignores_a_provider_type_that_is_not_registered() {
        let registry = AiProviderRegistry::new();
        registry
            .restore_config(&AiProviderConfig {
                provider_type: AiProviderType::DeepSeek,
                api_key: Some("sk-test".into()),
                endpoint: None,
                model: "deepseek-chat".into(),
                max_tokens: 200_000,
                extra: serde_json::Value::Null,
            })
            .await;

        assert!(registry.available_providers().await.is_empty());
    }
}
/// Registers built-in AI providers and discovers external providers via `inventory`
/// into an existing registry (used for deferred / in-place startup warm-up).
pub async fn register_ai_providers(registry: &AiProviderRegistry) {
    registry
        .register(Arc::new(OpenAiProvider::with_http_client(
            registry.http_client.clone(),
        )))
        .await;
    registry
        .register(Arc::new(AnthropicProvider::with_http_client(
            registry.http_client.clone(),
        )))
        .await;
    registry
        .register(Arc::new(DeepSeekProvider::with_http_client(
            registry.http_client.clone(),
        )))
        .await;
    registry
        .register(Arc::new(OllamaProvider::with_http_client(
            registry.http_client.clone(),
        )))
        .await;
    registry
        .register(Arc::new(CustomProvider::with_http_client(
            registry.http_client.clone(),
        )))
        .await;

    for factory in iter_ai_provider_factories() {
        let pv = factory.protocol_version();
        if pv < MIN_AI_PROTOCOL_VERSION {
            tracing::error!(
                "AI provider '{}' protocol version {} is too old (minimum {}). Skipping.",
                factory.provider_id(),
                pv,
                MIN_AI_PROTOCOL_VERSION
            );
            continue;
        }
        if pv > AI_PROTOCOL_VERSION {
            tracing::warn!(
                "AI provider '{}' protocol version {} is newer than host {}. Loading with possible incompatibility.",
                factory.provider_id(),
                pv,
                AI_PROTOCOL_VERSION
            );
        }
        if pv < AI_PROTOCOL_VERSION {
            tracing::warn!(
                "AI provider '{}' protocol version {} < host {}. Running in degraded mode \
                 (streaming={}, tools={}).",
                factory.provider_id(),
                pv,
                AI_PROTOCOL_VERSION,
                factory.supports_streaming(),
                factory.supports_tools(),
            );
        }

        let provider = factory.create();
        tracing::info!(
            "Registered AI provider: {} (protocol v{})",
            factory.provider_id(),
            pv
        );
        registry.register(provider).await;
    }
}

/// Registers built-in AI providers and discovers external providers via `inventory`.
pub async fn init_ai_providers() -> AiProviderRegistry {
    let registry = AiProviderRegistry::new();
    register_ai_providers(&registry).await;
    registry
}
