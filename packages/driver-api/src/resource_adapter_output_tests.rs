// Included in resource_adapter::tests to reuse its synthetic driver and budget.
#[tokio::test]
async fn execution_forwards_the_exact_command_pair_and_fabricates_no_observation() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");
    let sink = CommandOutputSink::default();
    let input = serde_json::json!({"sql":"select 1", "syntheticOption":{"limit":7}});
    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("query", input.clone()),
            &sink,
        )
        .await
        .expect("typed-output sink accepts the legacy payload");
    assert_eq!(
        *sink.outputs.lock().unwrap(),
        vec![serde_json::json!({
            "kind":"commandResult", "data":{"command":"query", "input":input},
        })]
    );
    assert_eq!(sink.row_chunks.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(completion.completion_status, CompletionStatus::Succeeded);
    assert!(completion.statement_results.is_empty());
    assert!(completion.session_handles.is_empty());
    assert_eq!(completion.context_before, SessionContext::unobserved());
    assert_eq!(completion.context_after, SessionContext::unobserved());
    assert_eq!(
        completion.transaction_observation.state,
        TransactionState::Unknown
    );
    provider
        .close_resource(&handle)
        .await
        .expect("close acquired resource");
}

#[derive(Default)]
struct CommandOutputSink {
    outputs: std::sync::Mutex<Vec<serde_json::Value>>,
    row_chunks: std::sync::atomic::AtomicUsize,
}
#[async_trait]
impl ResultSink for CommandOutputSink {
    async fn write(&self, _: ResultChunk) -> Result<(), ResourceError> {
        self.row_chunks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn write_output(&self, output: serde_json::Value) -> Result<(), ResourceError> {
        self.outputs.lock().unwrap().push(output);
        Ok(())
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        Ok(())
    }
}

/// A row-only sink intentionally retains ResultSink's default output rejection.
struct RowOnlySink;
#[async_trait]
impl ResultSink for RowOnlySink {
    async fn write(&self, _: ResultChunk) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        Ok(())
    }
}
#[tokio::test]
async fn legacy_command_output_rejection_is_not_reported_as_success() {
    let provider = adapter(LegacyFake::new());
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");
    let result = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-rejected"),
            &CommandCall::new("query", serde_json::json!({"sql":"select 1"})),
            &RowOnlySink,
        )
        .await;
    assert!(matches!(result, Err(ResourceError::SinkRejected { .. })));
    provider
        .close_resource(&handle)
        .await
        .expect("close acquired resource after sink rejection");
}
