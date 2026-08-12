//! In-process mock LLM (`AiProvider`) for `commands/ai` unit tests.
//!
//! Prefer this over HTTP wiremock when testing command orchestration: responses
//! are queued deterministically and request payloads can be asserted.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_ai_api::{
    AiError, AiProvider, AiProviderConfig, AiProviderType, CompletionRequest, CompletionResponse,
    StreamChunk, TokenUsage, ToolCall,
};
use tokio::sync::mpsc;

use super::app_state::TestAppState;

/// Scripted in-process LLM used in place of OpenAI in the registry.
pub struct MockAiProvider {
    supports_tools: AtomicBool,
    validate_ok: AtomicBool,
    initialized: AtomicBool,
    complete_queue: Mutex<VecDeque<Result<CompletionResponse, AiError>>>,
    stream_queue: Mutex<VecDeque<Result<Vec<StreamChunk>, AiError>>>,
    calls: Mutex<Vec<CompletionRequest>>,
}

impl MockAiProvider {
    pub fn new() -> Self {
        Self {
            supports_tools: AtomicBool::new(true),
            validate_ok: AtomicBool::new(true),
            initialized: AtomicBool::new(false),
            complete_queue: Mutex::new(VecDeque::new()),
            stream_queue: Mutex::new(VecDeque::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    pub fn set_supports_tools(&self, v: bool) {
        self.supports_tools.store(v, Ordering::Relaxed);
    }

    pub fn set_validate_ok(&self, v: bool) {
        self.validate_ok.store(v, Ordering::Relaxed);
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    pub fn push_text(&self, content: impl Into<String>) {
        let content = content.into();
        self.complete_queue
            .lock()
            .unwrap()
            .push_back(Ok(CompletionResponse {
                request_id: String::new(),
                content,
                reasoning: None,
                usage: TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 2,
                    total_tokens: 3,
                },
                model: "mock-model".into(),
                finish_reason: Some("stop".into()),
                tool_calls: None,
                response_id: None,
            }));
    }

    pub fn push_error(&self, msg: impl Into<String>) {
        self.complete_queue
            .lock()
            .unwrap()
            .push_back(Err(AiError::RequestFailed(msg.into())));
    }

    /// Queue a non-streaming tool-call response (used when stream queue is empty
    /// and the default `stream_complete` falls back to `complete`).
    #[allow(dead_code)]
    pub fn push_tool_calls(&self, tool_calls: Vec<ToolCall>) {
        self.complete_queue
            .lock()
            .unwrap()
            .push_back(Ok(CompletionResponse {
                request_id: String::new(),
                content: String::new(),
                reasoning: None,
                usage: TokenUsage::default(),
                model: "mock-model".into(),
                finish_reason: Some("tool_calls".into()),
                tool_calls: Some(tool_calls),
                response_id: None,
            }));
    }

    /// Queue an explicit streaming round (chunks delivered via mpsc).
    pub fn push_stream_chunks(&self, chunks: Vec<StreamChunk>) {
        self.stream_queue.lock().unwrap().push_back(Ok(chunks));
    }

    pub fn push_stream_text(&self, content: impl Into<String>) {
        let content = content.into();
        self.push_stream_chunks(vec![
            StreamChunk {
                content: content.clone(),
                reasoning: None,
                done: false,
                usage: None,
                tool_calls: None,
                response_id: None,
            },
            StreamChunk {
                content: String::new(),
                reasoning: None,
                done: true,
                usage: Some(TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 2,
                    total_tokens: 3,
                }),
                tool_calls: None,
                response_id: None,
            },
        ]);
    }

    /// Round 1: tool call via stream; round 2: final text via stream.
    pub fn push_stream_tool_then_text(&self, tool: ToolCall, final_text: impl Into<String>) {
        self.push_stream_chunks(vec![StreamChunk {
            content: String::new(),
            reasoning: None,
            done: true,
            usage: None,
            tool_calls: Some(vec![tool]),
            response_id: None,
        }]);
        self.push_stream_text(final_text);
    }

    #[allow(dead_code)]
    pub fn push_stream_error(&self, msg: impl Into<String>) {
        self.stream_queue
            .lock()
            .unwrap()
            .push_back(Err(AiError::RequestFailed(msg.into())));
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    pub fn last_request(&self) -> Option<CompletionRequest> {
        self.calls.lock().unwrap().last().cloned()
    }

    #[allow(dead_code)]
    pub fn take_calls(&self) -> Vec<CompletionRequest> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

impl Default for MockAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AiProvider for MockAiProvider {
    fn provider_type(&self) -> AiProviderType {
        // Occupy the OpenAI slot so resolve_ai + saved configs keep working.
        AiProviderType::OpenAi
    }

    fn display_name(&self) -> &str {
        "Mock LLM"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        self.supports_tools.load(Ordering::Relaxed)
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        if !self.validate_ok.load(Ordering::Relaxed) {
            return Err(AiError::InvalidApiKey);
        }
        if config
            .api_key
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(AiError::NotConfigured("api_key".into()));
        }
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        self.validate_config(config).await?;
        self.initialized.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError> {
        self.calls.lock().unwrap().push(request.clone());
        let mut queue = self.complete_queue.lock().unwrap();
        let mut response = queue.pop_front().unwrap_or_else(|| {
            Err(AiError::Internal(
                "MockAiProvider complete queue empty".into(),
            ))
        })?;
        if response.request_id.is_empty() {
            response.request_id = request.request_id.clone();
        }
        Ok(response)
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        // Prefer an explicit stream script when present.
        let scripted = {
            let mut q = self.stream_queue.lock().unwrap();
            q.pop_front()
        };

        match scripted {
            Some(Ok(chunks)) => {
                self.calls.lock().unwrap().push(request.clone());
                for chunk in chunks {
                    if sender.send(Ok(chunk)).await.is_err() {
                        break;
                    }
                }
                Ok(())
            }
            Some(Err(e)) => {
                self.calls.lock().unwrap().push(request.clone());
                let _ = sender
                    .send(Err(AiError::RequestFailed(e.to_string())))
                    .await;
                Err(e)
            }
            None => {
                // Fall back to complete → single done chunk (trait default behavior).
                let response = self.complete(request).await?;
                let _ = sender
                    .send(Ok(StreamChunk {
                        content: response.content,
                        reasoning: response.reasoning,
                        done: true,
                        usage: Some(response.usage),
                        tool_calls: response.tool_calls,
                        response_id: response.response_id,
                    }))
                    .await;
                Ok(())
            }
        }
    }

    async fn reset(&self) {
        self.initialized.store(false, Ordering::Relaxed);
        self.complete_queue.lock().unwrap().clear();
        self.stream_queue.lock().unwrap().clear();
        self.calls.lock().unwrap().clear();
    }
}

pub fn mock_ai_config() -> AiProviderConfig {
    AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: Some("mock-api-key".into()),
        model: "mock-model".into(),
        endpoint: Some("mock://llm".into()),
        max_tokens: 4096,
        extra: serde_json::json!({}),
    }
}

impl TestAppState {
    /// Register in-process [`MockAiProvider`] as OpenAI and persist its config.
    pub async fn with_mock_ai() -> (Self, Arc<MockAiProvider>) {
        Self::with_mock_ai_options(super::mock_driver::MockDriverOptions::default()).await
    }

    pub async fn with_mock_ai_tables() -> (Self, Arc<MockAiProvider>) {
        Self::with_mock_ai_options(super::app_state::rich_mock_options()).await
    }

    pub async fn with_mock_ai_options(
        opts: super::mock_driver::MockDriverOptions,
    ) -> (Self, Arc<MockAiProvider>) {
        let test = Self::with_options(opts).await;
        // Register real builtins first, then overwrite OpenAI with the mock.
        test.state.ensure_ai_ready().await;

        let mock = Arc::new(MockAiProvider::new());
        test.state.ai_registry.register(mock.clone()).await;

        let cfg = mock_ai_config();
        mock.initialize(&cfg).await.expect("mock initialize");
        test.state
            .store
            .save_ai_config(&cfg)
            .await
            .expect("save mock ai config");

        (test, mock)
    }
}
