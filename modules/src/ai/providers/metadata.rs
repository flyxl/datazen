//! Provider metadata shared by host configuration surfaces.

use crate::ai::AiProviderType;

/// Return the built-in endpoint and protocol defaults displayed for a provider.
/// This describes existing providers only; it does not affect provider registration.
pub fn provider_defaults(provider_type: AiProviderType) -> (&'static str, &'static str) {
    match provider_type {
        AiProviderType::OpenAi => ("https://api.openai.com/v1", "open_ai_compatible"),
        AiProviderType::Anthropic => ("https://api.anthropic.com", "anthropic_compatible"),
        AiProviderType::DeepSeek => ("https://api.deepseek.com", "open_ai_responses"),
        AiProviderType::Ollama => ("http://127.0.0.1:11434/v1", "open_ai_compatible"),
        AiProviderType::Custom => ("", "open_ai_compatible"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_the_existing_provider_types() {
        assert_eq!(
            provider_defaults(AiProviderType::OpenAi),
            ("https://api.openai.com/v1", "open_ai_compatible")
        );
        assert_eq!(
            provider_defaults(AiProviderType::Anthropic),
            ("https://api.anthropic.com", "anthropic_compatible")
        );
        assert_eq!(
            provider_defaults(AiProviderType::DeepSeek),
            ("https://api.deepseek.com", "open_ai_responses")
        );
        assert_eq!(
            provider_defaults(AiProviderType::Ollama),
            ("http://127.0.0.1:11434/v1", "open_ai_compatible")
        );
        assert_eq!(
            provider_defaults(AiProviderType::Custom),
            ("", "open_ai_compatible")
        );
    }
}
