//! Streaming query result transfer.
//!
//! Batch size is a **transfer chunk size**, not a SQL row cap. SQL `LIMIT`
//! (the "limit SELECT results" setting) is passed separately as `limit` and
//! must not be inferred from [`QUERY_STREAM_BATCH_SIZE`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::types::{ColumnInfo, MultiQueryResult, StatementResult, Value};

/// Default number of rows per `Rows` event. Independent of SQL LIMIT.
pub const QUERY_STREAM_BATCH_SIZE: usize = 500;

pub type QueryStreamCallback = Arc<dyn Fn(QueryStreamEvent) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum QueryStreamEvent {
    #[serde(rename_all = "camelCase")]
    StatementStart {
        index: usize,
        sql: String,
        columns: Vec<ColumnInfo>,
    },
    #[serde(rename_all = "camelCase")]
    Rows {
        index: usize,
        rows: Vec<Vec<Option<Value>>>,
    },
    #[serde(rename_all = "camelCase")]
    StatementEnd {
        index: usize,
        rows_affected: Option<u64>,
        execution_time_ms: u64,
        truncated: bool,
    },
    #[serde(rename_all = "camelCase")]
    Done { total_time_ms: u64 },
}

/// Emit an already-materialized [`MultiQueryResult`] as stream events,
/// draining row vectors as chunks are sent so the default (non-streaming)
/// driver path can still avoid a giant single IPC payload.
pub fn emit_multi_query_as_stream(result: MultiQueryResult, on_event: &QueryStreamCallback) {
    for (index, stmt) in result.results.into_iter().enumerate() {
        emit_statement_as_stream(index, stmt, on_event);
    }
    on_event(QueryStreamEvent::Done {
        total_time_ms: result.total_time_ms,
    });
}

/// Append `LIMIT n+1` to a top-level SELECT/WITH that has no LIMIT.
/// The extra row lets callers detect truncation. Independent of batch size.
pub fn append_select_limit(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };
    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    let is_select = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !is_select {
        return (stmt.to_string(), None);
    }
    if upper.split_whitespace().any(|w| w == "LIMIT") {
        return (stmt.to_string(), Some(lim));
    }
    (format!("{trimmed} LIMIT {}", lim + 1), Some(lim))
}

/// Emit already-decoded rows through [`QueryRowBatcher`] without holding a
/// second full `MultiQueryResult` copy.
pub fn stream_decoded_rows(
    on_event: &QueryStreamCallback,
    index: usize,
    sql: String,
    columns: Vec<ColumnInfo>,
    rows: impl IntoIterator<Item = Vec<Option<Value>>>,
    limit: Option<u32>,
    execution_time_ms: u64,
    rows_affected: Option<u64>,
) {
    let mut batcher = QueryRowBatcher::new(Arc::clone(on_event), index, sql, limit);
    batcher.start(columns);
    for row in rows {
        if !batcher.push(row) {
            break;
        }
    }
    batcher.finish(execution_time_ms, rows_affected);
}

pub fn emit_execute_statement(
    on_event: &QueryStreamCallback,
    index: usize,
    sql: String,
    rows_affected: u64,
    execution_time_ms: u64,
) {
    on_event(QueryStreamEvent::StatementStart {
        index,
        sql,
        columns: Vec::new(),
    });
    on_event(QueryStreamEvent::StatementEnd {
        index,
        rows_affected: Some(rows_affected),
        execution_time_ms,
        truncated: false,
    });
}

fn emit_statement_as_stream(
    index: usize,
    mut stmt: StatementResult,
    on_event: &QueryStreamCallback,
) {
    on_event(QueryStreamEvent::StatementStart {
        index,
        sql: std::mem::take(&mut stmt.sql),
        columns: std::mem::take(&mut stmt.columns),
    });
    let mut rows = stmt.rows;
    while !rows.is_empty() {
        let end = QUERY_STREAM_BATCH_SIZE.min(rows.len());
        let chunk: Vec<_> = rows.drain(..end).collect();
        on_event(QueryStreamEvent::Rows { index, rows: chunk });
    }
    on_event(QueryStreamEvent::StatementEnd {
        index,
        rows_affected: stmt.rows_affected,
        execution_time_ms: stmt.execution_time_ms,
        truncated: stmt.truncated,
    });
}

/// Incremental row emitter used by drivers that stream from the wire.
///
/// `limit` is the SQL result cap (from "limit SELECT results"). `None` means
/// stream every row. Batching is always applied regardless of `limit`.
pub struct QueryRowBatcher {
    on_event: QueryStreamCallback,
    index: usize,
    sql: String,
    limit: Option<u32>,
    batch_size: usize,
    batch: Vec<Vec<Option<Value>>>,
    accepted: u32,
    truncated: bool,
    started: bool,
}

impl QueryRowBatcher {
    pub fn new(
        on_event: QueryStreamCallback,
        index: usize,
        sql: String,
        limit: Option<u32>,
    ) -> Self {
        Self {
            on_event,
            index,
            sql,
            limit,
            batch_size: QUERY_STREAM_BATCH_SIZE,
            batch: Vec::with_capacity(QUERY_STREAM_BATCH_SIZE),
            accepted: 0,
            truncated: false,
            started: false,
        }
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn start(&mut self, columns: Vec<ColumnInfo>) {
        if self.started {
            return;
        }
        self.started = true;
        (self.on_event)(QueryStreamEvent::StatementStart {
            index: self.index,
            sql: self.sql.clone(),
            columns,
        });
    }

    /// Push a decoded row. Returns `false` when the SQL result limit has been
    /// exceeded; the extra row is **not** emitted and the caller should stop fetching.
    pub fn push(&mut self, row: Vec<Option<Value>>) -> bool {
        if let Some(lim) = self.limit {
            if self.accepted >= lim {
                self.truncated = true;
                return false;
            }
        }
        self.accepted += 1;
        self.batch.push(row);
        if self.batch.len() >= self.batch_size {
            self.flush();
        }
        true
    }

    fn flush(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        let rows = std::mem::take(&mut self.batch);
        (self.on_event)(QueryStreamEvent::Rows {
            index: self.index,
            rows,
        });
    }

    pub fn finish(mut self, execution_time_ms: u64, rows_affected: Option<u64>) {
        if !self.started {
            (self.on_event)(QueryStreamEvent::StatementStart {
                index: self.index,
                sql: std::mem::take(&mut self.sql),
                columns: Vec::new(),
            });
        }
        self.flush();
        (self.on_event)(QueryStreamEvent::StatementEnd {
            index: self.index,
            rows_affected: rows_affected.or(Some(u64::from(self.accepted))),
            execution_time_ms,
            truncated: self.truncated,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ColumnInfo;
    use std::sync::Mutex;

    fn collect() -> (QueryStreamCallback, Arc<Mutex<Vec<QueryStreamEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_cb = Arc::clone(&events);
        let cb: QueryStreamCallback = Arc::new(move |ev| {
            events_cb.lock().unwrap().push(ev);
        });
        (cb, events)
    }

    fn col(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "int".into(),
            nullable: true,
        }
    }

    fn row(n: i64) -> Vec<Option<Value>> {
        vec![Some(Value::Integer(n))]
    }

    #[test]
    fn event_wire_format_uses_type_tag_and_camel_case() {
        let ev = QueryStreamEvent::Done { total_time_ms: 12 };
        let value = serde_json::to_value(&ev).unwrap();
        assert_eq!(value["type"], "done");
        assert_eq!(value["totalTimeMs"], 12);
        assert!(value.get("total_time_ms").is_none());

        let start = QueryStreamEvent::StatementStart {
            index: 0,
            sql: "SELECT 1".into(),
            columns: vec![],
        };
        let value = serde_json::to_value(&start).unwrap();
        assert_eq!(value["type"], "statementStart");

        let end = QueryStreamEvent::StatementEnd {
            index: 1,
            rows_affected: Some(3),
            execution_time_ms: 9,
            truncated: false,
        };
        let value = serde_json::to_value(&end).unwrap();
        assert_eq!(value["type"], "statementEnd");
        assert_eq!(value["rowsAffected"], 3);
        assert_eq!(value["executionTimeMs"], 9);
        assert!(value.get("rows_affected").is_none());
    }

    #[test]
    fn emit_multi_query_chunks_independently_of_sql_limit() {
        let row_count = QUERY_STREAM_BATCH_SIZE + 3;
        let stmt = StatementResult {
            sql: "SELECT * FROM t".into(),
            columns: vec![col("id")],
            rows: (0..row_count as i64).map(row).collect(),
            rows_affected: Some(row_count as u64),
            execution_time_ms: 8,
            truncated: false,
        };
        let (cb, events) = collect();
        emit_multi_query_as_stream(
            MultiQueryResult {
                results: vec![stmt],
                total_time_ms: 8,
            },
            &cb,
        );
        let events = events.lock().unwrap();
        let row_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .collect();
        assert_eq!(row_events, vec![QUERY_STREAM_BATCH_SIZE, 3]);
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
        let end = events.iter().find_map(|e| match e {
            QueryStreamEvent::StatementEnd { truncated, .. } => Some(*truncated),
            _ => None,
        });
        assert_eq!(end, Some(false), "chunking must not set truncated");
    }

    #[test]
    fn batcher_sql_limit_is_independent_of_batch_size() {
        let (cb, events) = collect();
        let mut batcher =
            QueryRowBatcher::new(Arc::clone(&cb), 0, "SELECT 1".into(), Some(5)).with_batch_size(2);
        batcher.start(vec![col("id")]);
        for i in 0..20 {
            if !batcher.push(row(i)) {
                break;
            }
        }
        batcher.finish(4, None);

        let events = events.lock().unwrap();
        let total_rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(total_rows, 5);
        let end = events.iter().find_map(|e| match e {
            QueryStreamEvent::StatementEnd {
                truncated,
                rows_affected,
                ..
            } => Some((*truncated, *rows_affected)),
            _ => None,
        });
        assert_eq!(end, Some((true, Some(5))));
    }

    #[test]
    fn batcher_without_sql_limit_emits_all_rows() {
        let (cb, events) = collect();
        let mut batcher =
            QueryRowBatcher::new(Arc::clone(&cb), 0, "SELECT 1".into(), None).with_batch_size(100);
        batcher.start(vec![col("id")]);
        for i in 0..50 {
            assert!(batcher.push(row(i)));
        }
        batcher.finish(1, None);
        let events = events.lock().unwrap();
        let total_rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(total_rows, 50);
        let truncated = events.iter().find_map(|e| match e {
            QueryStreamEvent::StatementEnd { truncated, .. } => Some(*truncated),
            _ => None,
        });
        assert_eq!(truncated, Some(false));
    }

    #[test]
    fn batcher_empty_result_still_emits_start_and_end() {
        let (cb, events) = collect();
        let batcher = QueryRowBatcher::new(Arc::clone(&cb), 0, "SELECT 1".into(), None);
        batcher.finish(2, Some(0));
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            QueryStreamEvent::StatementStart { columns, .. } if columns.is_empty()
        ));
        assert!(matches!(
            &events[1],
            QueryStreamEvent::StatementEnd {
                truncated: false,
                rows_affected: Some(0),
                ..
            }
        ));
    }

    #[test]
    fn append_select_limit_adds_plus_one_and_skips_non_select() {
        assert_eq!(
            append_select_limit("SELECT * FROM t", None),
            ("SELECT * FROM t".into(), None)
        );
        assert_eq!(
            append_select_limit("SELECT * FROM t", Some(10)),
            ("SELECT * FROM t LIMIT 11".into(), Some(10))
        );
        assert_eq!(
            append_select_limit("SELECT * FROM t LIMIT 3", Some(10)),
            ("SELECT * FROM t LIMIT 3".into(), Some(10))
        );
        assert_eq!(
            append_select_limit("INSERT INTO t VALUES (1)", Some(10)),
            ("INSERT INTO t VALUES (1)".into(), None)
        );
        assert_eq!(
            append_select_limit("WITH x AS (SELECT 1) SELECT * FROM x", Some(5)),
            (
                "WITH x AS (SELECT 1) SELECT * FROM x LIMIT 6".into(),
                Some(5)
            )
        );
    }

    #[test]
    fn stream_decoded_rows_applies_sql_limit() {
        let (cb, events) = collect();
        stream_decoded_rows(
            &cb,
            0,
            "SELECT * FROM t".into(),
            vec![col("id")],
            (0..20).map(row),
            Some(5),
            3,
            None,
        );
        let events = events.lock().unwrap();
        let total_rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(total_rows, 5);
        assert!(matches!(
            &events.last(),
            Some(QueryStreamEvent::StatementEnd {
                truncated: true,
                ..
            })
        ));
    }

    #[test]
    fn emit_execute_statement_has_no_rows() {
        let (cb, events) = collect();
        emit_execute_statement(&cb, 2, "INSERT INTO t VALUES (1)".into(), 1, 4);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            QueryStreamEvent::StatementStart { index: 2, columns, .. } if columns.is_empty()
        ));
        assert!(matches!(
            &events[1],
            QueryStreamEvent::StatementEnd {
                index: 2,
                rows_affected: Some(1),
                truncated: false,
                ..
            }
        ));
    }

    #[test]
    fn emit_multi_query_empty_still_emits_done() {
        let (cb, events) = collect();
        emit_multi_query_as_stream(
            MultiQueryResult {
                results: vec![],
                total_time_ms: 7,
            },
            &cb,
        );
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            QueryStreamEvent::Done { total_time_ms: 7 }
        ));
    }

    #[test]
    fn batcher_start_is_idempotent_and_zero_batch_size_becomes_one() {
        let (cb, events) = collect();
        let mut batcher =
            QueryRowBatcher::new(Arc::clone(&cb), 0, "SELECT 1".into(), None).with_batch_size(0);
        batcher.start(vec![col("id")]);
        batcher.start(vec![col("ignored")]);
        assert!(batcher.push(row(1)));
        batcher.finish(1, None);
        let events = events.lock().unwrap();
        let starts: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, QueryStreamEvent::StatementStart { .. }))
            .collect();
        assert_eq!(starts.len(), 1);
        assert!(matches!(
            &events[1],
            QueryStreamEvent::Rows { rows, .. } if rows.len() == 1
        ));
    }

    #[test]
    fn stream_decoded_rows_empty_emits_start_and_end() {
        let (cb, events) = collect();
        stream_decoded_rows(
            &cb,
            0,
            "SELECT 1 WHERE 0".into(),
            vec![col("id")],
            Vec::<Vec<Option<Value>>>::new(),
            None,
            2,
            Some(0),
        );
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            QueryStreamEvent::StatementStart { .. }
        ));
        assert!(matches!(
            &events[1],
            QueryStreamEvent::StatementEnd {
                rows_affected: Some(0),
                truncated: false,
                ..
            }
        ));
    }
}
