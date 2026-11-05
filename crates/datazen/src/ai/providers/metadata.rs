//! Provider metadata shared by host configuration surfaces.

use crate::ai::AiProviderType;

/// Return the built-in endpoint and protocol defaults displayed for a provider.
/// Endpoints use the provider's native OpenAI-compatible or Anthropic-compatible
/// base URL (the protocol layer appends `/chat/completions` or `/v1/messages`).
pub fn provider_defaults(provider_type: AiProviderType) -> (&'static str, &'static str) {
    match provider_type {
        AiProviderType::OpenAi => ("https://api.openai.com/v1", "open_ai_compatible"),
        AiProviderType::Anthropic => ("https://api.anthropic.com", "anthropic_compatible"),
        AiProviderType::DeepSeek => ("https://api.deepseek.com", "open_ai_responses"),
        AiProviderType::Ollama => ("http://127.0.0.1:11434/v1", "open_ai_compatible"),
        AiProviderType::Custom => ("", "open_ai_compatible"),
        AiProviderType::Gemini => (
            "https://generativelanguage.googleapis.com/v1beta/openai/",
            "open_ai_compatible",
        ),
        AiProviderType::Qwen => (
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "open_ai_compatible",
        ),
        AiProviderType::MiniMax => ("https://api.minimax.io/v1", "open_ai_compatible"),
        AiProviderType::OpenAiCompatible => ("", "open_ai_compatible"),
        AiProviderType::AnthropicCompatible => ("", "anthropic_compatible"),
    }
}

/// Model preselected when the user picks a provider preset. Compatible/custom
/// endpoints deliberately stay blank: the vendor/model is user-supplied.
pub fn provider_default_model(provider_type: AiProviderType) -> &'static str {
    match provider_type {
        AiProviderType::OpenAi => "gpt-4o",
        AiProviderType::Anthropic => "claude-sonnet-4-20250514",
        AiProviderType::DeepSeek => "deepseek-chat",
        AiProviderType::Ollama => "llama3.2",
        AiProviderType::Custom => "",
        AiProviderType::Gemini => "gemini-2.5-pro",
        AiProviderType::Qwen => "qwen-plus",
        AiProviderType::MiniMax => "MiniMax-M3.1-Flash",
        AiProviderType::OpenAiCompatible => "",
        AiProviderType::AnthropicCompatible => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_all_provider_presets() {
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
        assert_eq!(
            provider_defaults(AiProviderType::Gemini),
            (
                "https://generativelanguage.googleapis.com/v1beta/openai/",
                "open_ai_compatible"
            )
        );
        assert_eq!(
            provider_defaults(AiProviderType::Qwen),
            (
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                "open_ai_compatible"
            )
        );
        assert_eq!(
            provider_defaults(AiProviderType::MiniMax),
            ("https://api.minimax.io/v1", "open_ai_compatible")
        );
        assert_eq!(
            provider_defaults(AiProviderType::OpenAiCompatible),
            ("", "open_ai_compatible")
        );
        assert_eq!(
            provider_defaults(AiProviderType::AnthropicCompatible),
            ("", "anthropic_compatible")
        );
    }

    #[test]
    fn every_preset_has_a_model_default_or_is_explicitly_custom() {
        let model = provider_default_model(AiProviderType::Gemini);
        assert_eq!(model, "gemini-2.5-pro");
        assert_eq!(provider_default_model(AiProviderType::Qwen), "qwen-plus");
        assert_eq!(
            provider_default_model(AiProviderType::MiniMax),
            "MiniMax-M3.1-Flash"
        );
        assert!(provider_default_model(AiProviderType::OpenAiCompatible).is_empty());
        assert!(provider_default_model(AiProviderType::AnthropicCompatible).is_empty());
        assert!(provider_default_model(AiProviderType::Custom).is_empty());
    }
}
