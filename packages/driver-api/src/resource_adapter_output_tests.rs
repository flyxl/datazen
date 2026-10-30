// Included in resource_adapter::tests to reuse its synthetic driver and budget.

/// A buffered `query` answer must leave the adapter in the chunk protocol: one
/// `statementColumns` + row batch + `statementSummary` per statement. A single
/// `commandResult` carries no `statementIndex`, and every reader that only
/// understands the chunk protocol (the SQL Editor's result assembler) drops it,
/// which is exactly how a successful query rendered an empty result grid.
#[tokio::test]
async fn a_buffered_query_answer_is_split_into_the_chunk_protocol() {
    let answer: MultiQueryResult = serde_json::from_value(serde_json::json!({
        "results": [
            {
                "sql": "CREATE TABLE t (id INT)",
                "columns": [],
                "rows": [],
                "rowsAffected": 0,
                "executionTimeMs": 3,
                "truncated": false,
            },
            {
                "sql": "SELECT id FROM t",
                "columns": [{"name": "id", "dataType": "integer", "nullable": false}],
                "rows": [[1], [2]],
                "rowsAffected": 2,
                "executionTimeMs": 7,
                "truncated": false,
            },
        ],
        "totalTimeMs": 10,
    }))
    .expect("the fixture is a MultiQueryResult");
    let provider = adapter(LegacyFake::new().answering_query_with(answer));
    let budget = Arc::new(CountingBudget::default());
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire succeeds");
    let sink = ChunkRecordingSink::default();
    provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-chunks"),
            &CommandCall::new("query", serde_json::json!({"sql":"SELECT id FROM t"})),
            &sink,
        )
        .await
        .expect("the chunk sink accepts the split answer");
    let chunks = sink.chunks.lock().unwrap().clone();
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.statement_index)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(
        serde_json::to_value(&chunks[0].rows).expect("rows serialize"),
        serde_json::json!([[1], [2]])
    );
    assert_eq!(chunks[0].sql, "SELECT id FROM t");
    let outputs = sink.outputs.lock().unwrap().clone();
    let columns = outputs
        .iter()
        .find(|output| output["kind"] == "statementColumns")
        .expect("the statement with columns publishes them for the grid header");
    assert_eq!(columns["statementIndex"], serde_json::json!(1));
    assert_eq!(columns["columns"][0]["name"], "id");
    let summaries = outputs
        .iter()
        .filter(|output| output["kind"] == "statementSummary")
        .collect::<Vec<_>>();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0]["statementIndex"], serde_json::json!(0));
    assert_eq!(summaries[1]["rowsAffected"], serde_json::json!(2));
    assert!(outputs
        .iter()
        .all(|output| output["kind"] != "commandResult"));
    provider
        .close_resource(&handle)
        .await
        .expect("close acquired resource");
}

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

/// Records both channels, so a test can assert the chunk protocol order.
#[derive(Default)]
struct ChunkRecordingSink {
    outputs: std::sync::Mutex<Vec<serde_json::Value>>,
    chunks: std::sync::Mutex<Vec<ResultChunk>>,
}
#[async_trait]
impl ResultSink for ChunkRecordingSink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.chunks.lock().unwrap().push(chunk);
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
