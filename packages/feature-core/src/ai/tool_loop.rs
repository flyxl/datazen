//! Streaming AI tool-loop orchestration; provider streaming and tool I/O are injected.
use super::{
    safety::redact_for_gate,
    tools::{classify_tool, is_readonly_db_tool, ToolKind, ToolLoopGuard},
};
use crate::error::FeatureError as CommandError;
use datazen_ai_api::*;
use std::sync::Arc;
use tokio::sync::mpsc;
pub type StreamCallback = Arc<dyn Fn(&str, Result<StreamChunk, AiError>) + Send + Sync>;
#[async_trait::async_trait]
pub trait AiToolLoopHost: Send + Sync {
    async fn egress_summary(&self) -> String;
    async fn execute_db_tool(&self, call: &ToolCall) -> String;
    async fn execute_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> String;
}
struct StreamRoundResult {
    content: String,
    reasoning: String,
    tool_calls: Option<Vec<ToolCall>>,
    usage: Option<TokenUsage>,
    had_error: bool,
    response_id: Option<String>,
}

pub async fn run_streaming_tool_loop(
    provider: Arc<dyn AiProvider>,
    host: &dyn AiToolLoopHost,
    on_chunk: StreamCallback,
    request_id: &str,
    mut request: CompletionRequest,
    _max_rounds: usize,
    cmd_label: &str,
    gate: &AiSafetyGateConfig,
) -> Result<String, CommandError> {
    let mut guard = ToolLoopGuard::new();

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
        let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);

        let (result_tx, result_rx) = tokio::sync::oneshot::channel::<StreamRoundResult>();
        let on_chunk_c = on_chunk.clone();
        let rid_c = request_id.to_string();

        tokio::spawn(async move {
            let mut full_content = String::new();
            let mut full_reasoning = String::new();
            let mut final_tool_calls: Option<Vec<ToolCall>> = None;
            let mut final_usage = None;
            let mut final_response_id = None;
            let mut had_error = false;

            while let Some(chunk_result) = rx.recv().await {
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

        provider
            .stream_complete(&request, tx)
            .await
            .map_err(CommandError::Ai)?;

        let result = result_rx
            .await
            .map_err(|_| CommandError::Internal("Stream result channel closed".into()))?;

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
                ToolKind::Db(_) => host.execute_db_tool(tc).await,
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
        async fn execute_db_tool(&self, _call: &ToolCall) -> String {
            self.db_tool_calls.fetch_add(1, Ordering::SeqCst);
            "{}".into()
        }
        async fn execute_mcp_tool(&self, _server: &str, _tool: &str, _args: &str) -> String {
            "{}".into()
        }
    }

    async fn collect(
        provider: Arc<dyn AiProvider>,
        host: &StubHost,
        token: CancellationToken,
    ) -> Vec<StreamChunk> {
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
            on_chunk,
            "req-1",
            request(Some(token)),
            6,
            "test",
            &gate(),
        )
        .await;
        assert!(result.is_ok(), "loop must converge: {result:?}");
        let chunks = seen.lock().expect("chunk sink").clone();
        chunks
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
