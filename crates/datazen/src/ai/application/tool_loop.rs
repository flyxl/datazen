//! Streaming AI tool-loop orchestration; provider streaming and tool I/O are injected.
use super::error::AiUseCaseError as CommandError;
use super::retry::RetryPolicy;
use super::tools::{classify_tool, is_readonly_db_tool, ToolKind, ToolLoopGuard, ToolLoopLimits};
use crate::ai::domain::safety::redact_for_gate;
use crate::ai::domain::traits::AiProvider;
use crate::ai::domain::types::{
    AiError, AiSafetyGateConfig, ChatMessage, CompletionRequest, MessageRole, StreamChunk, ToolCall,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
pub type StreamCallback = Arc<dyn Fn(&str, Result<StreamChunk, AiError>) + Send + Sync>;

/// Per-request policy for database tools.
///
/// `allowed_connection_id` is required for read-only query/sampling: it
/// confines them to the chat's active connection, and `None` fails closed
/// (no bound connection means no query permission). `max_sample_rows` is the
/// safety-gate default for `sample_rows` when the model omits a limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiToolContext {
    pub allowed_connection_id: Option<String>,
    pub max_sample_rows: u32,
}

impl Default for AiToolContext {
    fn default() -> Self {
        Self {
            allowed_connection_id: None,
            max_sample_rows: 100,
        }
    }
}

#[async_trait::async_trait]
pub trait AiToolLoopHost: Send + Sync {
    async fn egress_summary(&self) -> String;
    async fn execute_db_tool(&self, ctx: &AiToolContext, call: &ToolCall) -> String;
    async fn execute_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> String;
}
struct StreamRoundResult {
    content: String,
    reasoning: String,
    tool_calls: Option<Vec<ToolCall>>,
    usage: Option<crate::ai::domain::types::TokenUsage>,
    had_error: bool,
    response_id: Option<String>,
}

pub async fn run_streaming_tool_loop(
    provider: Arc<dyn AiProvider>,
    host: &dyn AiToolLoopHost,
    tool_ctx: &AiToolContext,
    on_chunk: StreamCallback,
    request_id: &str,
    mut request: CompletionRequest,
    limits: ToolLoopLimits,
    cmd_label: &str,
    gate: &AiSafetyGateConfig,
) -> Result<String, CommandError> {
    let mut guard = ToolLoopGuard::with_limits(limits);
    let retry_policy = RetryPolicy::from_gate(gate);

    // ── FR-13: Send egress summary before AI reply begins ──
    {
        let summary = host.egress_summary().await;
        on_chunk(
            request_id,
            Ok(StreamChunk {
                content: String::new(),
                reasoning: None,
                done: false,
                cancelled: false,
                usage: None,
                tool_calls: None,
                response_id: None,
                egress_summary: Some(summary),
            }),
        );
    }

    loop {
        // ── Cancel check (Phase C) ──
        if let Some(ref token) = request.cancel_token {
            if token.is_cancelled() {
                tracing::info!(%request_id, "{cmd_label}: cancelled before round {}", guard.round);
                on_chunk(
                    request_id,
                    Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        // Cancellation is not a normal completion: never report it as one.
                        cancelled: true,
                        usage: None,
                        tool_calls: None,
                        response_id: None,
                        egress_summary: None,
                    }),
                );
                return Ok(request_id.to_string());
            }
        }

        // ── Guard: round / token / time budget ──
        if let Err(msg) = guard.check_round() {
            tracing::warn!(%request_id, "{msg}");
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: format!("[Tool loop stopped: {msg}]"),
                    reasoning: None,
                    done: true,
                    cancelled: false,
                    usage: None,
                    tool_calls: None,
                    response_id: None,
                    egress_summary: None,
                }),
            );
            return Ok(request_id.to_string());
        }
        let mut retries_done = 0u32;
        let result = loop {
            let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);

            let (result_tx, result_rx) = tokio::sync::oneshot::channel::<StreamRoundResult>();
            let on_chunk_c = on_chunk.clone();
            let rid_c = request_id.to_string();
            let saw_output = Arc::new(AtomicBool::new(false));
            let saw_output_task = saw_output.clone();

            tokio::spawn(async move {
                let mut full_content = String::new();
                let mut full_reasoning = String::new();
                let mut final_tool_calls: Option<Vec<ToolCall>> = None;
                let mut final_usage = None;
                let mut final_response_id = None;
                let mut had_error = false;

                while let Some(chunk_result) = rx.recv().await {
                    saw_output_task.store(true, Ordering::SeqCst);
                    match chunk_result {
                        Ok(chunk) => {
                            if chunk.done {
                                let content = chunk.content;
                                let reasoning = chunk.reasoning;
                                full_content.push_str(&content);
                                if let Some(r) = &reasoning {
                                    full_reasoning.push_str(r);
                                }
                                final_tool_calls = chunk.tool_calls;
                                final_usage = chunk.usage;
                                final_response_id = chunk.response_id;
                                if !content.is_empty() || reasoning.is_some() {
                                    on_chunk_c(
                                        &rid_c,
                                        Ok(StreamChunk {
                                            content,
                                            reasoning,
                                            done: false,
                                            cancelled: false,
                                            usage: None,
                                            tool_calls: None,
                                            response_id: None,
                                            egress_summary: None,
                                        }),
                                    );
                                }
                            } else {
                                full_content.push_str(&chunk.content);
                                if let Some(r) = &chunk.reasoning {
                                    full_reasoning.push_str(r);
                                }
                                on_chunk_c(&rid_c, Ok(chunk));
                            }
                        }
                        Err(e) => {
                            on_chunk_c(&rid_c, Err(e));
                            had_error = true;
                            break;
                        }
                    }
                }

                let _ = result_tx.send(StreamRoundResult {
                    content: full_content,
                    reasoning: full_reasoning,
                    tool_calls: final_tool_calls,
                    usage: final_usage,
                    had_error,
                    response_id: final_response_id,
                });
            });

            let stream_error = provider.stream_complete(&request, tx).await.err();

            let result = result_rx
                .await
                .map_err(|_| CommandError::Internal("Stream result channel closed".into()))?;

            let Some(error) = stream_error else {
                break result;
            };

            // Once any chunk reached the caller a retry would replay output
            // (and possibly duplicate tool calls). Only pre-first-chunk start
            // failures are safe to retry.
            if saw_output.load(Ordering::SeqCst) {
                if result.had_error {
                    return Ok(request_id.to_string());
                }
                return Err(CommandError::Ai(error));
            }

            if !retry_policy.should_retry(retries_done, &error) {
                return Err(CommandError::Ai(error));
            }

            tracing::warn!(
                %request_id,
                retry = retries_done + 1,
                max = retry_policy.max_retries,
                error = %error,
                "{cmd_label}: transient provider start failure, retrying"
            );
            if !retry_policy
                .wait_before_retry(retries_done, request.cancel_token.as_ref())
                .await
            {
                tracing::info!(%request_id, "{cmd_label}: cancelled during retry backoff");
                on_chunk(
                    request_id,
                    Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        cancelled: true,
                        usage: None,
                        tool_calls: None,
                        response_id: None,
                        egress_summary: None,
                    }),
                );
                return Ok(request_id.to_string());
            }
            retries_done += 1;
        };

        if result.had_error {
            return Ok(request_id.to_string());
        }

        // Cancellation can land while the provider stream is still running. The
        // terminal state must say so instead of continuing into tool dispatch.
        if request
            .cancel_token
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            tracing::info!(%request_id, "{cmd_label}: cancelled after stream round {}", guard.round);
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    cancelled: true,
                    usage: result.usage,
                    tool_calls: None,
                    response_id: result.response_id,
                    egress_summary: None,
                }),
            );
            return Ok(request_id.to_string());
        }

        // Accumulate token usage for the guard's budget check.
        if let Some(ref usage) = result.usage {
            guard.accumulate_usage(usage);
        }

        let all_tcs = match result.tool_calls {
            Some(tcs) if !tcs.is_empty() => tcs,
            _ => {
                on_chunk(
                    request_id,
                    Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        cancelled: false,
                        usage: result.usage,
                        tool_calls: result.tool_calls,
                        response_id: result.response_id,
                        egress_summary: None,
                    }),
                );
                return Ok(request_id.to_string());
            }
        };

        let classified: Vec<(ToolCall, ToolKind)> = all_tcs
            .into_iter()
            .map(|tc| {
                let kind = classify_tool(&tc.name);
                (tc, kind)
            })
            .collect();

        let executable_count = classified
            .iter()
            .filter(|(_, kind)| matches!(kind, ToolKind::Db(_) | ToolKind::Mcp { .. }))
            .count();

        let has_ask_questions = classified
            .iter()
            .any(|(_, kind)| matches!(kind, ToolKind::AskQuestions));
        let all_ask_questions = classified
            .iter()
            .all(|(_, kind)| matches!(kind, ToolKind::AskQuestions));

        if executable_count == 0 && all_ask_questions {
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    cancelled: false,
                    usage: result.usage,
                    tool_calls: Some(classified.iter().map(|(tc, _)| tc.clone()).collect()),
                    response_id: result.response_id,
                    egress_summary: None,
                }),
            );
            return Ok(request_id.to_string());
        }

        tracing::info!(
            %request_id,
            round = guard.round,
            executable_count,
            has_ask_questions,
            tool_names = ?classified.iter().map(|(t, _)| t.name.as_str()).collect::<Vec<_>>(),
            response_id = ?result.response_id,
            "{cmd_label}: executing tools (round {})", guard.round
        );
        tracing::debug!(
            %request_id,
            round = guard.round,
            tool_count = classified.len(),
            "{cmd_label}: tools selected"
        );

        request.messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: result.content,
            reasoning: if result.reasoning.is_empty() {
                None
            } else {
                Some(result.reasoning)
            },
            tool_calls: Some(classified.iter().map(|(tc, _)| tc.clone()).collect()),
            tool_call_id: None,
        });

        let mcp_tool_calls: Vec<ToolCall> = classified
            .iter()
            .filter(|(_, kind)| matches!(kind, ToolKind::Mcp { .. }))
            .map(|(tc, _)| tc.clone())
            .collect();
        if !mcp_tool_calls.is_empty() {
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: false,
                    cancelled: false,
                    usage: None,
                    tool_calls: Some(mcp_tool_calls),
                    response_id: None,
                    egress_summary: None,
                }),
            );
        }

        for (tc, kind) in &classified {
            match kind {
                ToolKind::AskQuestions => continue,
                ToolKind::Unknown => {
                    // Unknown tool: send error chunk and terminate loop.
                    tracing::warn!(
                        %request_id,
                        tool_name = %tc.name,
                        "unknown tool requested by provider"
                    );
                    let err_msg = format!(
                        "Tool '{}' is not available. The provider requested a tool that is not registered.",
                        tc.name
                    );
                    request.messages.push(ChatMessage {
                        role: MessageRole::Tool,
                        content: err_msg.clone(),
                        reasoning: None,
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                    });
                    on_chunk(
                        request_id,
                        Ok(StreamChunk {
                            content: err_msg,
                            reasoning: None,
                            done: true,
                            cancelled: false,
                            usage: result.usage,
                            tool_calls: None,
                            response_id: result.response_id,
                            egress_summary: None,
                        }),
                    );
                    return Ok(request_id.to_string());
                }
                _ => {}
            }
        }

        // Separate read-only DB tools for potential future parallel execution.
        // Tools execute sequentially through the host port.
        let (parallel_db_tcs, sequential_tcs): (Vec<_>, Vec<_>) = classified
            .iter()
            .filter(|(_, kind)| !matches!(kind, ToolKind::AskQuestions))
            .partition(|(_, kind)| matches!(kind, ToolKind::Db(name) if is_readonly_db_tool(name)));

        // Execute all tools sequentially.
        for (tc, kind) in parallel_db_tcs.iter().chain(sequential_tcs.iter()) {
            // A tool may be long-running; a cancel that lands mid-round must
            // stop the remaining dispatch and still report cancellation.
            if request
                .cancel_token
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
            {
                tracing::info!(%request_id, "{cmd_label}: cancelled before tool {}", tc.name);
                on_chunk(
                    request_id,
                    Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        cancelled: true,
                        usage: result.usage,
                        tool_calls: None,
                        response_id: result.response_id,
                        egress_summary: None,
                    }),
                );
                return Ok(request_id.to_string());
            }
            let tool_result = match kind {
                ToolKind::Db(_) => host.execute_db_tool(tool_ctx, tc).await,
                ToolKind::Mcp {
                    server_id,
                    tool_name,
                } => {
                    host.execute_mcp_tool(server_id, tool_name, &tc.arguments)
                        .await
                }
                _ => continue,
            };
            let redacted = redact_for_gate(&tool_result, gate);
            request.messages.push(guard.truncate_and_wrap(tc, redacted));
        }

        if has_ask_questions {
            let ask_tool_calls: Vec<ToolCall> = classified
                .iter()
                .filter(|(_, kind)| matches!(kind, ToolKind::AskQuestions))
                .map(|(tc, _)| tc.clone())
                .collect();
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    cancelled: false,
                    usage: result.usage,
                    tool_calls: Some(ask_tool_calls),
                    response_id: result.response_id,
                    egress_summary: None,
                }),
            );
            return Ok(request_id.to_string());
        }
    }
    // The loop always returns internally (cancel, guard, no-tools, ask_questions,
    // or after tool execution). This point is unreachable.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::domain::types::{AiProviderConfig, AiProviderType, CompletionResponse};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    fn gate() -> AiSafetyGateConfig {
        serde_json::from_str("{}").expect("safety gate defaults")
    }

    fn request(token: Option<CancellationToken>) -> CompletionRequest {
        CompletionRequest {
            request_id: "req-1".into(),
            model: "test-model".into(),
            messages: vec![],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
            cancel_token: token,
        }
    }

    /// Provider that cancels the request while its stream is still open. This is
    /// the window where a naive loop would report a normal completion.
    struct MidStreamCancelProvider {
        token: Option<CancellationToken>,
    }

    #[async_trait::async_trait]
    impl AiProvider for MidStreamCancelProvider {
        fn provider_type(&self) -> AiProviderType {
            AiProviderType::Custom
        }
        fn display_name(&self) -> &str {
            "test-mid-stream-cancel"
        }
        async fn validate_config(&self, _config: &AiProviderConfig) -> Result<(), AiError> {
            Ok(())
        }
        async fn initialize(&self, _config: &AiProviderConfig) -> Result<(), AiError> {
            Ok(())
        }
        async fn complete(
            &self,
            _request: &CompletionRequest,
        ) -> Result<CompletionResponse, AiError> {
            Err(AiError::NotConfigured("complete is not used".into()))
        }
        async fn stream_complete(
            &self,
            _request: &CompletionRequest,
            sender: mpsc::Sender<Result<StreamChunk, AiError>>,
        ) -> Result<(), AiError> {
            if let Some(token) = &self.token {
                token.cancel();
            }
            let _ = sender
                .send(Ok(StreamChunk {
                    content: "partial".into(),
                    reasoning: None,
                    done: true,
                    cancelled: false,
                    usage: None,
                    tool_calls: None,
                    response_id: None,
                    egress_summary: None,
                }))
                .await;
            Ok(())
        }
    }

    #[derive(Default)]
    struct StubHost {
        db_tool_calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl AiToolLoopHost for StubHost {
        async fn egress_summary(&self) -> String {
            String::new()
        }
        async fn execute_db_tool(&self, _ctx: &AiToolContext, _call: &ToolCall) -> String {
            self.db_tool_calls.fetch_add(1, Ordering::SeqCst);
            "{}".into()
        }
        async fn execute_mcp_tool(&self, _server: &str, _tool: &str, _args: &str) -> String {
            "{}".into()
        }
    }

    async fn run_loop(
        provider: Arc<dyn AiProvider>,
        host: &StubHost,
        token: CancellationToken,
        gate_config: AiSafetyGateConfig,
    ) -> (Result<String, CommandError>, Vec<StreamChunk>) {
        run_loop_with_limits(
            provider,
            host,
            token,
            gate_config,
            ToolLoopLimits::default(),
        )
        .await
    }

    async fn run_loop_with_limits(
        provider: Arc<dyn AiProvider>,
        host: &StubHost,
        token: CancellationToken,
        gate_config: AiSafetyGateConfig,
        limits: ToolLoopLimits,
    ) -> (Result<String, CommandError>, Vec<StreamChunk>) {
        let seen: Arc<Mutex<Vec<StreamChunk>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let on_chunk: StreamCallback = Arc::new(move |_id, chunk| {
            if let Ok(chunk) = chunk {
                sink.lock().expect("chunk sink").push(chunk);
            }
        });
        let result = run_streaming_tool_loop(
            provider,
            host,
            &AiToolContext::default(),
            on_chunk,
            "req-1",
            request(Some(token)),
            limits,
            "test",
            &gate_config,
        )
        .await;
        let chunks = seen.lock().expect("chunk sink").clone();
        (result, chunks)
    }

    async fn collect(
        provider: Arc<dyn AiProvider>,
        host: &StubHost,
        token: CancellationToken,
    ) -> Vec<StreamChunk> {
        let (result, chunks) = run_loop(provider, host, token, gate()).await;
        assert!(result.is_ok(), "loop must converge: {result:?}");
        chunks
    }

    struct ToolCallingProvider {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AiProvider for ToolCallingProvider {
        fn provider_type(&self) -> AiProviderType {
            AiProviderType::Custom
        }
        fn display_name(&self) -> &str {
            "test-tool-calling"
        }
        async fn validate_config(&self, _config: &AiProviderConfig) -> Result<(), AiError> {
            Ok(())
        }
        async fn initialize(&self, _config: &AiProviderConfig) -> Result<(), AiError> {
            Ok(())
        }
        async fn complete(
            &self,
            _request: &CompletionRequest,
        ) -> Result<CompletionResponse, AiError> {
            Err(AiError::NotConfigured("complete is not used".into()))
        }
        async fn stream_complete(
            &self,
            _request: &CompletionRequest,
            sender: mpsc::Sender<Result<StreamChunk, AiError>>,
        ) -> Result<(), AiError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _ = sender
                .send(Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    cancelled: false,
                    usage: None,
                    tool_calls: Some(vec![ToolCall {
                        id: format!("call-{}", self.calls.load(Ordering::SeqCst)),
                        name: "list_connections".into(),
                        arguments: "{}".into(),
                    }]),
                    response_id: None,
                    egress_summary: None,
                }))
                .await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn caller_max_rounds_stops_the_second_round() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(ToolCallingProvider {
            calls: calls.clone(),
        });
        let host = StubHost::default();
        let token = CancellationToken::new();

        let (result, chunks) = run_loop_with_limits(
            provider,
            &host,
            token,
            gate_with_retries(0),
            ToolLoopLimits {
                max_rounds: 1,
                ..ToolLoopLimits::default()
            },
        )
        .await;

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second round must stop");
        assert_eq!(host.db_tool_calls.load(Ordering::SeqCst), 1);
        assert!(
            chunks
                .iter()
                .any(|chunk| chunk.content.contains("max rounds (1)")),
            "guard stop must be visible: {chunks:?}"
        );
    }

    struct FlakyStartProvider {
        calls: Arc<AtomicUsize>,
        fail_times: usize,
        emit_before_error: bool,
    }

    #[async_trait::async_trait]
    impl AiProvider for FlakyStartProvider {
        fn provider_type(&self) -> AiProviderType {
            AiProviderType::Custom
        }
        fn display_name(&self) -> &str {
            "test-flaky-start"
        }
        async fn validate_config(&self, _config: &AiProviderConfig) -> Result<(), AiError> {
            Ok(())
        }
        async fn initialize(&self, _config: &AiProviderConfig) -> Result<(), AiError> {
            Ok(())
        }
        async fn complete(
            &self,
            _request: &CompletionRequest,
        ) -> Result<CompletionResponse, AiError> {
            Err(AiError::NotConfigured("complete is not used".into()))
        }
        async fn stream_complete(
            &self,
            _request: &CompletionRequest,
            sender: mpsc::Sender<Result<StreamChunk, AiError>>,
        ) -> Result<(), AiError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.emit_before_error && call == 1 {
                let _ = sender
                    .send(Ok(StreamChunk {
                        content: "partial".into(),
                        reasoning: None,
                        done: false,
                        cancelled: false,
                        usage: None,
                        tool_calls: None,
                        response_id: None,
                        egress_summary: None,
                    }))
                    .await;
                return Err(AiError::RateLimited {
                    retry_after_secs: 0,
                });
            }
            if call <= self.fail_times {
                return Err(AiError::RateLimited {
                    retry_after_secs: 0,
                });
            }
            let _ = sender
                .send(Ok(StreamChunk {
                    content: "ok".into(),
                    reasoning: None,
                    done: true,
                    cancelled: false,
                    usage: None,
                    tool_calls: None,
                    response_id: None,
                    egress_summary: None,
                }))
                .await;
            Ok(())
        }
    }

    fn gate_with_retries(retries: u32) -> AiSafetyGateConfig {
        let mut gate = gate();
        gate.api_retries = retries;
        gate
    }

    #[tokio::test]
    async fn retries_a_transient_start_failure_then_streams() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(FlakyStartProvider {
            calls: calls.clone(),
            fail_times: 1,
            emit_before_error: false,
        });
        let host = StubHost::default();
        let token = CancellationToken::new();

        let (result, chunks) = run_loop(provider, &host, token, gate_with_retries(1)).await;

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "one retry");
        assert!(chunks.iter().any(|chunk| chunk.content == "ok"));
    }

    #[tokio::test]
    async fn zero_retries_fails_immediately() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(FlakyStartProvider {
            calls: calls.clone(),
            fail_times: 5,
            emit_before_error: false,
        });
        let host = StubHost::default();
        let token = CancellationToken::new();

        let (result, _chunks) = run_loop(provider, &host, token, gate_with_retries(0)).await;

        assert!(result.is_err(), "no retry means the error surfaces");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn does_not_retry_after_the_first_stream_chunk() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(FlakyStartProvider {
            calls: calls.clone(),
            fail_times: 1,
            emit_before_error: true,
        });
        let host = StubHost::default();
        let token = CancellationToken::new();

        let (result, chunks) = run_loop(provider, &host, token, gate_with_retries(2)).await;

        assert!(
            result.is_err(),
            "stream errors after output are not replayed"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no replay after output");
        assert!(chunks.iter().any(|chunk| chunk.content == "partial"));
    }

    #[tokio::test]
    async fn pre_cancelled_request_reports_cancelled_not_done() {
        let token = CancellationToken::new();
        token.cancel();
        let provider = Arc::new(MidStreamCancelProvider { token: None });
        let host = StubHost::default();

        let chunks = collect(provider, &host, token).await;
        let last = chunks.last().expect("terminal chunk");
        assert!(last.done, "terminal chunk must be done");
        assert!(
            last.cancelled,
            "cancellation must never be reported as a normal completion"
        );
        assert_eq!(
            host.db_tool_calls.load(Ordering::SeqCst),
            0,
            "no tool may be dispatched after cancellation"
        );
    }

    #[tokio::test]
    async fn cancel_landing_mid_stream_stops_tool_dispatch_and_reports_cancelled() {
        let token = CancellationToken::new();
        let provider = Arc::new(MidStreamCancelProvider {
            token: Some(token.clone()),
        });
        let host = StubHost::default();

        let chunks = collect(provider, &host, token).await;
        let last = chunks.last().expect("terminal chunk");
        assert!(last.done);
        assert!(last.cancelled);
        assert_eq!(host.db_tool_calls.load(Ordering::SeqCst), 0);
    }
}
