//! WireMock helpers for AI command integration tests.

use datazen_ai_api::{AiProviderConfig, AiProviderType};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::app_state::TestAppState;

/// Running mock HTTP server + helpers for OpenAI-compatible responses.
pub struct WiremockAi {
    pub server: MockServer,
}

impl WiremockAi {
    pub async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    pub fn endpoint_v1(&self) -> String {
        format!("{}/v1", self.server.uri())
    }

    pub async fn mount_models_list(&self) {
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("Authorization", "Bearer test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "gpt-test", "object": "model"}]
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn mount_chat_completion(&self, content: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "content": content },
                    "finish_reason": "stop"
                }],
                "model": "gpt-test",
                "usage": { "prompt_tokens": 3, "completion_tokens": 5, "total_tokens": 8 }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn mount_chat_completion_text_once(&self, content: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "content": content },
                    "finish_reason": "stop"
                }],
                "model": "gpt-test",
                "usage": { "prompt_tokens": 3, "completion_tokens": 5, "total_tokens": 8 }
            })))
            .up_to_n_times(1)
            .mount(&self.server)
            .await;
    }

    pub async fn mount_chat_completion_text(&self, content: &str) {
        self.mount_chat_completion(serde_json::Value::String(content.into()))
            .await;
    }

    /// SSE stream ending with a single text response (no tool calls).
    pub async fn mount_chat_stream_text(&self, content: &str) {
        let sse = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}\n\ndata: {{\"choices\":[{{\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":1,\"completion_tokens\":2,\"total_tokens\":3}}}}\n\ndata: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-api-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&self.server)
            .await;
    }

    /// SSE stream that requests a DB tool then returns final SQL text.
    #[allow(dead_code)]
    pub async fn mount_chat_stream_with_db_tool_then_text(&self, final_text: &str) {
        let sse = format!(
            r#"data: {{"choices":[{{"delta":{{"tool_calls":[{{"index":0,"id":"call_1","function":{{"name":"list_connections","arguments":"{{}}"}}}}]}}}}]}}

data: {{"choices":[{{"finish_reason":"tool_calls"}}]}}

data: {{"choices":[{{"delta":{{"content":"{final_text}"}}}}]}}

data: {{"choices":[{{"finish_reason":"stop"}}],"usage":{{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}}}}

data: [DONE]

"#
        );
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-api-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .expect(2)
            .mount(&self.server)
            .await;
    }
}

impl TestAppState {
    /// Mock postgres driver + OpenAI provider pointed at wiremock.
    pub async fn with_wiremock_ai() -> (Self, WiremockAi) {
        Self::with_wiremock_ai_options(super::mock_driver::MockDriverOptions::default()).await
    }

    pub async fn with_wiremock_ai_tables() -> (Self, WiremockAi) {
        Self::with_wiremock_ai_options(super::app_state::rich_mock_options()).await
    }

    pub async fn with_wiremock_ai_options(
        opts: super::mock_driver::MockDriverOptions,
    ) -> (Self, WiremockAi) {
        use crate::ai::register_ai_providers;

        let mock = WiremockAi::start().await;
        mock.mount_models_list().await;

        let test = Self::with_options(opts).await;
        register_ai_providers(&test.state.ai_registry).await;

        let cfg = openai_wiremock_config(&mock);
        let provider = test
            .state
            .ai_registry
            .get(&AiProviderType::OpenAi)
            .await
            .expect("openai provider");
        provider.initialize(&cfg).await.expect("initialize");
        test.state
            .store
            .save_ai_config(&cfg)
            .await
            .expect("save ai cfg");

        (test, mock)
    }
}

pub fn openai_wiremock_config(mock: &WiremockAi) -> AiProviderConfig {
    AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: Some("test-api-key".into()),
        model: "gpt-test".into(),
        endpoint: Some(mock.endpoint_v1()),
        max_tokens: 4096,
        extra: serde_json::json!({}),
    }
}
