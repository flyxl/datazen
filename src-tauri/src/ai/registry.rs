//! AI Provider registry — resolves `AiProviderType` to a concrete `AiProvider`.

use datazen_ai_api::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::anthropic::AnthropicProvider;
use super::custom::CustomProvider;
use super::deepseek::DeepSeekProvider;
use super::openai::OpenAiProvider;

pub struct AiProviderRegistry {
    providers: Arc<RwLock<HashMap<AiProviderType, Arc<dyn AiProvider>>>>,
}

impl AiProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
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
            .register(Arc::new(OpenAiProvider::new()))
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
            .register(Arc::new(OpenAiProvider::new()))
            .await;
        registry
            .register(Arc::new(OpenAiProvider::new()))
            .await;

        assert_eq!(registry.available_providers().await.len(), 1);
    }

    #[tokio::test]
    async fn test_available_providers() {
        let registry = AiProviderRegistry::new();
        registry
            .register(Arc::new(OpenAiProvider::new()))
            .await;
        registry
            .register(Arc::new(AnthropicProvider::new()))
            .await;

        let types = registry.available_providers().await;
        assert_eq!(types.len(), 2);
        assert!(types.contains(&AiProviderType::OpenAi));
        assert!(types.contains(&AiProviderType::Anthropic));
    }

    #[tokio::test]
    async fn test_list_providers() {
        let registry = AiProviderRegistry::new();
        registry
            .register(Arc::new(AnthropicProvider::new()))
            .await;

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
}

/// Registers built-in AI providers and discovers plugin providers via `inventory`.
pub async fn init_ai_providers() -> AiProviderRegistry {
    let registry = AiProviderRegistry::new();

    registry
        .register(Arc::new(OpenAiProvider::new()))
        .await;
    registry
        .register(Arc::new(AnthropicProvider::new()))
        .await;
    registry
        .register(Arc::new(DeepSeekProvider::new()))
        .await;
    registry
        .register(Arc::new(CustomProvider::new()))
        .await;

    for factory in iter_ai_provider_factories() {
        let pv = factory.protocol_version();
        if pv < MIN_AI_PROTOCOL_VERSION {
            tracing::error!(
                "AI plugin '{}' protocol version {} is too old (minimum {}). Skipping.",
                factory.provider_id(),
                pv,
                MIN_AI_PROTOCOL_VERSION
            );
            continue;
        }
        if pv > AI_PROTOCOL_VERSION {
            tracing::warn!(
                "AI plugin '{}' protocol version {} is newer than host {}. Loading with possible incompatibility.",
                factory.provider_id(),
                pv,
                AI_PROTOCOL_VERSION
            );
        }
        if pv < AI_PROTOCOL_VERSION {
            tracing::warn!(
                "AI plugin '{}' protocol version {} < host {}. Running in degraded mode \
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
            "Registered AI plugin provider: {} (protocol v{})",
            factory.provider_id(),
            pv
        );
        registry.register(provider).await;
    }

    registry
}
