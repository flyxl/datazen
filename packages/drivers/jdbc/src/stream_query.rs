//! Streaming query over Agent `query.execute` / `query.fetch` batches.

use datazen_driver_api::{ColumnInfo, DriverError, QueryStreamCallback, QueryStreamEvent, Value};
use std::sync::Arc;
use std::time::Instant;

use crate::agent_process::AgentProcessManager;
use crate::protocol::methods;

pub(crate) async fn stream_query(
    agent: &Arc<AgentProcessManager>,
    session_id: &str,
    sql: &str,
    limit: Option<u32>,
    on_event: QueryStreamCallback,
    parse_columns: fn(&serde_json::Value) -> Vec<ColumnInfo>,
    parse_rows: fn(&serde_json::Value) -> Vec<Vec<Option<Value>>>,
    map_err: fn(String) -> DriverError,
) -> Result<(), DriverError> {
    let max_rows = limit.unwrap_or(10_000);
    let start = Instant::now();
    let result = agent
        .rpc(
            methods::QUERY_EXECUTE,
            serde_json::json!({
                "sessionId": session_id,
                "sql": sql,
                "maxRows": max_rows,
                "fetchSize": 500,
            }),
        )
        .await
        .map_err(map_err)?;

    let columns = parse_columns(result.get("columns").unwrap_or(&serde_json::Value::Null));
    on_event(QueryStreamEvent::StatementStart {
        index: 0,
        sql: sql.to_string(),
        columns,
    });

    let mut total_rows: u32 = 0;
    let rows = parse_rows(result.get("rows").unwrap_or(&serde_json::Value::Null));
    if !rows.is_empty() {
        total_rows += rows.len() as u32;
        on_event(QueryStreamEvent::Rows { index: 0, rows });
    }
    let mut has_more = result
        .get("hasMore")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cursor_id = result
        .get("cursorId")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    if let Some(ref cid) = cursor_id {
        while has_more && total_rows < max_rows {
            let remain = max_rows - total_rows;
            let batch = agent
                .rpc(
                    methods::QUERY_FETCH,
                    serde_json::json!({
                        "sessionId": session_id,
                        "cursorId": cid,
                        "maxRows": remain,
                    }),
                )
                .await
                .map_err(map_err)?;
            let more = parse_rows(batch.get("rows").unwrap_or(&serde_json::Value::Null));
            has_more = batch
                .get("hasMore")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if more.is_empty() {
                break;
            }
            total_rows += more.len() as u32;
            on_event(QueryStreamEvent::Rows {
                index: 0,
                rows: more,
            });
        }
        let _ = agent
            .rpc(
                methods::QUERY_CLOSE,
                serde_json::json!({
                    "sessionId": session_id,
                    "cursorId": cid,
                }),
            )
            .await;
    }

    let elapsed = start.elapsed().as_millis() as u64;
    on_event(QueryStreamEvent::StatementEnd {
        index: 0,
        rows_affected: result.get("updateCount").and_then(|v| v.as_u64()),
        execution_time_ms: elapsed,
        truncated: has_more || total_rows >= max_rows,
    });
    on_event(QueryStreamEvent::Done {
        total_time_ms: elapsed,
    });
    Ok(())
}

pub(crate) async fn cancel_session_query(
    agent: &Arc<AgentProcessManager>,
    session_id: &str,
) -> Result<(), DriverError> {
    let _ = agent
        .rpc(
            methods::QUERY_CANCEL,
            serde_json::json!({ "sessionId": session_id }),
        )
        .await;
    Ok(())
}
