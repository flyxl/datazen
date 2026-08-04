use datazen_ai_api::*;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

// ─── Mock Provider for testing ───

struct MockProvider {
    initialized: RwLock<bool>,
    response_content: String,
}

impl MockProvider {
    fn new(response: &str) -> Self {
        Self {
            initialized: RwLock::new(false),
            response_content: response.to_string(),
        }
    }
}

#[async_trait]
impl AiProvider for MockProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Custom
    }

    fn display_name(&self) -> &str {
        "Mock Provider"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            id: "mock-model".into(),
            display_name: "Mock Model".into(),
            context_window: 4096,
            supports_streaming: true,
            supports_tools: false,
        }]
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        if config.api_key.as_deref() == Some("invalid") {
            return Err(AiError::InvalidApiKey);
        }
        Ok(())
    }

    async fn initialize(&self, _config: &AiProviderConfig) -> Result<(), AiError> {
        *self.initialized.write().await = true;
        Ok(())
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        if !*self.initialized.read().await {
            return Err(AiError::NotConfigured("Mock".into()));
        }

        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content: self.response_content.clone(),
            reasoning: None,
            model: request.model.clone(),
            finish_reason: Some("stop".into()),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            },
        })
    }
}

// ─── Type tests ───

#[test]
fn test_provider_type_display() {
    assert_eq!(AiProviderType::OpenAi.to_string(), "OpenAI");
    assert_eq!(AiProviderType::Anthropic.to_string(), "Anthropic");
    assert_eq!(AiProviderType::Custom.to_string(), "Custom");
}

#[test]
fn test_provider_type_serde() {
    let json = serde_json::to_string(&AiProviderType::OpenAi).unwrap();
    assert_eq!(json, r#""open_ai""#);

    let parsed: AiProviderType = serde_json::from_str(r#""anthropic""#).unwrap();
    assert_eq!(parsed, AiProviderType::Anthropic);
}

#[test]
fn test_ai_config_serde() {
    let config = AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: Some("sk-test".into()),
        endpoint: None,
        model: "gpt-4o".into(),
            max_tokens: 200_000,
        extra: serde_json::Value::Null,
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"providerType\":\"open_ai\""));
    assert!(json.contains("\"model\":\"gpt-4o\""));

    let parsed: AiProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.provider_type, AiProviderType::OpenAi);
    assert_eq!(parsed.model, "gpt-4o");
}

#[test]
fn test_message_role_serde() {
    let msg = ChatMessage {
        role: MessageRole::System,
        content: "You are helpful.".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"system\""));
}

#[test]
fn test_completion_request_serde() {
    let req = CompletionRequest {
        request_id: "req-1".into(),
        model: "gpt-4o".into(),
        messages: vec![
            ChatMessage {
                role: MessageRole::User,
                content: "Hello".into(),
            },
        ],
        temperature: Some(0.7),
        stop: None,
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"temperature\":0.7"));
    assert!(!json.contains("\"maxTokens\""));
    assert!(!json.contains("\"stop\""));
}

#[test]
fn test_ai_error_display() {
    let err = AiError::RequestFailed("connection refused".into());
    assert_eq!(err.to_string(), "API request failed: connection refused");

    let err = AiError::RateLimited { retry_after_secs: 60 };
    assert_eq!(err.to_string(), "Rate limited, retry after 60s");

    let err = AiError::ContextLengthExceeded { used: 130000, limit: 128000 };
    assert_eq!(
        err.to_string(),
        "Context length exceeded: 130000 tokens (max 128000)"
    );
}

#[test]
fn test_sql_generation_context_default() {
    let ctx = SqlGenerationContext::default();
    assert!(ctx.database_type.is_empty());
    assert!(ctx.database_version.is_none());
    assert!(ctx.schema_ddl.is_empty());
    assert!(ctx.current_table.is_none());
    assert!(ctx.recent_queries.is_empty());
}

#[test]
fn test_diagnosis_result_serde() {
    let result = DiagnosisResult {
        explanation: "Column not found".into(),
        suggested_sql: Some("SELECT name FROM users".into()),
        changes: vec!["Changed user_name to name".into()],
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: DiagnosisResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.explanation, "Column not found");
    assert_eq!(parsed.suggested_sql.unwrap(), "SELECT name FROM users");
    assert_eq!(parsed.changes.len(), 1);
}

// ─── Provider trait tests ───

#[tokio::test]
async fn test_mock_provider_validate_config() {
    let provider = MockProvider::new("test");

    let valid_config = AiProviderConfig {
        provider_type: AiProviderType::Custom,
        api_key: Some("sk-valid".into()),
        endpoint: None,
        model: "mock-model".into(),
            max_tokens: 200_000,
        extra: serde_json::Value::Null,
    };
    assert!(provider.validate_config(&valid_config).await.is_ok());

    let invalid_config = AiProviderConfig {
        provider_type: AiProviderType::Custom,
        api_key: Some("invalid".into()),
        endpoint: None,
        model: "mock-model".into(),
            max_tokens: 200_000,
        extra: serde_json::Value::Null,
    };
    let err = provider.validate_config(&invalid_config).await.unwrap_err();
    assert!(matches!(err, AiError::InvalidApiKey));
}

#[tokio::test]
async fn test_mock_provider_complete() {
    let provider = MockProvider::new("SELECT 1");

    let config = AiProviderConfig {
        provider_type: AiProviderType::Custom,
        api_key: Some("sk-test".into()),
        endpoint: None,
        model: "mock-model".into(),
            max_tokens: 200_000,
        extra: serde_json::Value::Null,
    };

    let result = provider
        .complete(&CompletionRequest {
            request_id: "r1".into(),
            model: "mock-model".into(),
            messages: vec![],
            temperature: None,

            stop: None,
        })
        .await;
    assert!(result.is_err());

    provider.initialize(&config).await.unwrap();

    let result = provider
        .complete(&CompletionRequest {
            request_id: "r1".into(),
            model: "mock-model".into(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "test".into(),
            }],
            temperature: None,

            stop: None,
        })
        .await
        .unwrap();

    assert_eq!(result.content, "SELECT 1");
    assert_eq!(result.usage.total_tokens, 30);
}

#[tokio::test]
async fn test_mock_provider_stream_fallback() {
    let provider = MockProvider::new("streaming result");

    let config = AiProviderConfig {
        provider_type: AiProviderType::Custom,
        api_key: Some("sk-test".into()),
        endpoint: None,
        model: "mock-model".into(),
            max_tokens: 200_000,
        extra: serde_json::Value::Null,
    };
    provider.initialize(&config).await.unwrap();

    let (tx, mut rx) = mpsc::channel(32);

    provider
        .stream_complete(
            &CompletionRequest {
                request_id: "r2".into(),
                model: "mock-model".into(),
                messages: vec![],
                temperature: None,
    
                stop: None,
            },
            tx,
        )
        .await
        .unwrap();

    let chunk = rx.recv().await.unwrap().unwrap();
    assert_eq!(chunk.content, "streaming result");
    assert!(chunk.done);
    assert!(chunk.usage.is_some());
    assert_eq!(chunk.usage.unwrap().total_tokens, 30);

    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn test_mock_provider_cancel_not_supported() {
    let provider = MockProvider::new("test");
    let err = provider.cancel("req-1").await.unwrap_err();
    assert!(matches!(err, AiError::NotSupported(_)));
}

#[test]
fn test_provider_metadata() {
    let provider = MockProvider::new("test");
    assert_eq!(provider.provider_type(), AiProviderType::Custom);
    assert_eq!(provider.display_name(), "Mock Provider");
    assert!(provider.supports_streaming());
    assert!(!provider.supports_tools());
    assert_eq!(provider.default_model(), "mock-model");

    let models = provider.available_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "mock-model");
    assert_eq!(models[0].context_window, 4096);
}

// ─── Factory tests ───

struct MockFactory;

impl AiProviderFactory for MockFactory {
    fn create(&self) -> Arc<dyn AiProvider> {
        Arc::new(MockProvider::new("factory result"))
    }

    fn provider_id(&self) -> &'static str {
        "mock"
    }
}

#[test]
fn test_factory_protocol_version() {
    let factory = MockFactory;
    assert_eq!(factory.protocol_version(), AI_PROTOCOL_VERSION);
    assert_eq!(factory.provider_id(), "mock");
}

#[test]
fn test_factory_creates_provider() {
    let factory = MockFactory;
    let provider = factory.create();
    assert_eq!(provider.display_name(), "Mock Provider");
}

#[test]
fn test_token_usage_default() {
    let usage = TokenUsage::default();
    assert_eq!(usage.prompt_tokens, 0);
    assert_eq!(usage.completion_tokens, 0);
    assert_eq!(usage.total_tokens, 0);
}

#[test]
fn test_model_info_serde() {
    let info = ModelInfo {
        id: "gpt-4o".into(),
        display_name: "GPT-4o".into(),
        context_window: 128_000,
        supports_streaming: true,
        supports_tools: true,
    };
    let json = serde_json::to_string(&info).unwrap();
    let parsed: ModelInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "gpt-4o");
    assert_eq!(parsed.context_window, 128_000);
    assert!(parsed.supports_tools);
}
