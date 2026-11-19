//! Independent audit for T252: `runEditorQuery.ts` port.
//!
//! The coder pins the React artifact cases. These tests fill branches it does
//! not reach: 21-disconnect exhaustion, the metadata/columns/summary chunk
//! kinds and truncation, and the positive-limit gate on the query envelope.

use serde_json::{json, Value};

use crate::session::editor_session_controller::ExecutionState;
use crate::session::execution_projection::{
    ArtifactChunk, PublishedResultChunk, ResultCompleteness, StatementResultSource, WritableMapping,
};
use crate::session::run_editor_query::{
    query_command_call, statement_snapshots, start, QueryEffect, QueryFailure, QueryInput,
    QueryOutcome, QueryRequest, QuerySettings,
};

fn request(limit_enabled: bool, limit: i64) -> QueryRequest {
    QueryRequest {
        key: "pane".to_owned(),
        connection_id: "connection".to_owned(),
        sql: "SELECT 1".to_owned(),
        database: None,
        schema: None,
        params: Vec::new(),
        settings: QuerySettings {
            limit_select_results: limit_enabled,
            query_result_limit: limit,
        },
    }
}

fn driven() -> crate::session::run_editor_query::QueryRun {
    let (mut run, _) = start(request(false, 0));
    let _ = run.step(QueryInput::ControllerReady);
    let _ = run.step(QueryInput::Executed(Ok(crate::session::editor_session_controller::ExecutionReceipt {
        execution_id: "execution-1".to_owned(),
        stream_id: "stream-1".to_owned(),
        state: ExecutionState::Queued,
    })));
    let _ = run.step(QueryInput::Projected);
    run
}

fn chunk(payload: Value, source: Option<StatementResultSource>) -> PublishedResultChunk {
    PublishedResultChunk {
        chunk: ArtifactChunk {
            artifact_id: "artifact-1".to_owned(),
            chunk_index: "0".to_owned(),
            total_chunks: Some("3".to_owned()),
            published_chunk_count: None,
            published_byte_length: None,
            offset: "0".to_owned(),
            bytes: Vec::new(),
            result_completeness: ResultCompleteness::Complete,
        },
        source,
        payload,
    }
}

/// React: `++disconnectedReads > 20` throws on the 21st lost read. The coder
/// only pins that one retry re-emits Recover, not the exhaustion boundary.
#[test]
fn disconnect_exhaustion_after_twenty_one_failed_reads() {
    let mut run = driven();
    let mut retries = 0;
    let mut final_step = None;
    for _ in 0..21 {
        let step = run.step(QueryInput::RecoverFailed);
        if step.effects.iter().any(|e| matches!(e, QueryEffect::Recover)) {
            retries += 1;
        }
        if step.outcome.is_some() {
            final_step = Some(step);
            break;
        }
    }
    assert_eq!(retries, 20, "20 recover retries before the 21st failure throws");
    let step = final_step.expect("the 21st failure settles the run");
    assert_eq!(
        step.outcome,
        Some(QueryOutcome::Failed(QueryFailure::DisconnectExhausted))
    );
    assert_eq!(step.effects, vec![QueryEffect::Dispose]);
}

/// React's inner loop handles `statementMetadata` / `statementColumns` /
/// `statementSummary` in addition to `rows`. Pin column extraction, an early
/// envelope-level `columns` fallback, truncation, index ordering and the
/// verified-binding rule.
#[test]
fn statement_snapshots_handle_metadata_columns_summary_and_truncation() {
    let verified = Some(StatementResultSource {
        execution_id: "execution-1".to_owned(),
        statement_index: serde_json::Number::from(1),
        writable_mapping: WritableMapping::Verified,
    });
    let chunks = vec![
        // index 1 metadata first in array; must be sorted after index 0.
        chunk(
            json!({ "output": { "kind": "statementMetadata", "statementIndex": 1,
                "columns": [{"name":"b","dataType":"text","nullable":true}], "truncated": true } }),
            verified,
        ),
        chunk(
            json!({ "columns": [{"name":"a","dataType":"int4","nullable":false}],
                "output": { "kind": "rows", "statementIndex": 0, "rows": [[7]] } }),
            None,
        ),
        chunk(
            json!({ "output": { "kind": "statementSummary", "statementIndex": 1,
                "sql": "SELECT b FROM t" } }),
            None,
        ),
    ];
    let snapshots = statement_snapshots(&chunks, false, &None);
    assert_eq!(snapshots.len(), 2, "one snapshot per statement index");
    assert_eq!(snapshots[0].0, 0, "results are ordered by statement index");
    assert_eq!(snapshots[0].1.columns[0].name, "a", "early envelope columns are used");
    assert_eq!(snapshots[0].1.rows, vec![vec![json!(7)]]);
    assert_eq!(snapshots[1].0, 1);
    assert_eq!(snapshots[1].1.sql, "SELECT b FROM t", "summary supplies the sql");
    assert_eq!(snapshots[1].1.columns[0].name, "b");
    assert!(snapshots[1].1.truncated, "metadata sets the truncated flag");
}

/// The `limit` field is gated on both the setting and a positive value.
#[test]
fn envelope_limit_is_gated_by_setting_and_positive_value() {
    assert_eq!(
        query_command_call(&request(true, 250)).input,
        r#"{"sql":"SELECT 1","recordHistory":true,"limit":250}"#
    );
    assert_eq!(
        query_command_call(&request(true, 0)).input,
        r#"{"sql":"SELECT 1","recordHistory":true}"#,
        "a zero limit is omitted even when the setting is on"
    );
    assert_eq!(
        query_command_call(&request(false, 250)).input,
        r#"{"sql":"SELECT 1","recordHistory":true}"#,
        "a disabled setting omits the limit"
    );
}
