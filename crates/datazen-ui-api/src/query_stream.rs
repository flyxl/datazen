use serde_json::Value;

use crate::{ColumnInfo, Id, QueryStreamEvent, StatementResult};

#[derive(Clone, Debug, PartialEq)]
pub struct StreamableQueryTab {
    pub results: Vec<StatementResult>,
    pub running: bool,
    pub error: Option<String>,
    pub execution_time_ms: Option<u64>,
    pub execution_id: Option<Id>,
    pub published_artifacts: Option<Value>,
}

fn empty_statement(sql: &str) -> StatementResult {
    StatementResult {
        source: None,
        runtime_binding: None,
        provenance: None,
        sql: sql.to_owned(),
        columns: Vec::new(),
        rows: Vec::new(),
        rows_affected: None,
        execution_time_ms: 0,
        truncated: Some(false),
        pinned: None,
    }
}

fn pad_to(results: &mut Vec<StatementResult>, target: usize) {
    while results.len() <= target {
        results.push(empty_statement(""));
    }
}

pub fn apply_query_stream_event(
    tab: &StreamableQueryTab,
    event: &QueryStreamEvent,
    base_offset: u32,
) -> StreamableQueryTab {
    let mut next = tab.clone();
    match event {
        QueryStreamEvent::ExecutionStarted { execution_id } => {
            next.execution_id = Some(execution_id.clone());
            next.published_artifacts = None;
        }
        QueryStreamEvent::PublishedArtifacts { artifacts } => {
            next.published_artifacts = serde_json::to_value(artifacts).ok();
        }
        QueryStreamEvent::StatementStart {
            index,
            sql,
            columns,
        } => {
            let target = (index.saturating_add(base_offset)) as usize;
            pad_to(&mut next.results, target);
            next.results[target] = StatementResult {
                source: None,
                runtime_binding: None,
                provenance: None,
                sql: sql.clone(),
                columns: columns.clone(),
                rows: Vec::new(),
                rows_affected: None,
                execution_time_ms: 0,
                truncated: Some(false),
                pinned: None,
            };
            next.error = None;
        }
        QueryStreamEvent::StatementSnapshot { index, result } => {
            let target = (index.saturating_add(base_offset)) as usize;
            pad_to(&mut next.results, target);
            next.results[target] = result.clone();
        }
        QueryStreamEvent::Rows { index, rows } => {
            let target = (index.saturating_add(base_offset)) as usize;
            if let Some(result) = next.results.get_mut(target) {
                result.rows.extend(rows.iter().cloned());
            }
        }
        QueryStreamEvent::StatementEnd {
            index,
            rows_affected,
            execution_time_ms,
            truncated,
        } => {
            let target = (index.saturating_add(base_offset)) as usize;
            if let Some(result) = next.results.get_mut(target) {
                result.rows_affected = *rows_affected;
                result.execution_time_ms = *execution_time_ms;
                result.truncated = Some(*truncated);
            }
        }
        QueryStreamEvent::Done { total_time_ms } => {
            next.running = false;
            next.execution_time_ms = Some(*total_time_ms);
            next.error = None;
            next.execution_id = None;
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn tab() -> StreamableQueryTab {
        StreamableQueryTab {
            results: Vec::new(),
            running: true,
            error: Some("old".to_owned()),
            execution_time_ms: None,
            execution_id: None,
            published_artifacts: None,
        }
    }

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.to_owned(),
            data_type: "text".to_owned(),
            nullable: true,
        }
    }

    #[test]
    fn execution_started_sets_id_and_clears_artifacts() {
        let mut start = tab();
        start.published_artifacts = Some(json!({"old": true}));
        let next = apply_query_stream_event(
            &start,
            &QueryStreamEvent::ExecutionStarted {
                execution_id: "exec-1".to_owned(),
            },
            0,
        );
        assert_eq!(next.execution_id.as_deref(), Some("exec-1"));
        assert!(next.published_artifacts.is_none());
        assert!(start.published_artifacts.is_some());
    }

    #[test]
    fn published_artifacts_are_captured_as_json() {
        let event: QueryStreamEvent = serde_json::from_value(json!({
            "type": "publishedArtifacts",
            "artifacts": {
                "executionId": "exec-1",
                "artifactIds": ["a1", "a2"],
                "resultCompleteness": "complete"
            }
        }))
        .unwrap();
        let next = apply_query_stream_event(&tab(), &event, 0);
        let artifacts = next.published_artifacts.unwrap();
        assert_eq!(artifacts["executionId"], "exec-1");
        assert_eq!(artifacts["artifactIds"], json!(["a1", "a2"]));
        assert_eq!(artifacts["resultCompleteness"], "complete");
    }

    #[test]
    fn statement_start_pads_creates_result_and_clears_error() {
        let next = apply_query_stream_event(
            &tab(),
            &QueryStreamEvent::StatementStart {
                index: 2,
                sql: "select 1".to_owned(),
                columns: vec![column("a")],
            },
            0,
        );
        assert_eq!(next.results.len(), 3);
        assert_eq!(next.results[0].sql, "");
        assert!(next.results[0].columns.is_empty());
        assert_eq!(next.results[2].sql, "select 1");
        assert_eq!(next.results[2].columns.len(), 1);
        assert_eq!(next.results[2].rows.len(), 0);
        assert_eq!(next.results[2].execution_time_ms, 0);
        assert_eq!(next.results[2].truncated, Some(false));
        assert!(next.error.is_none());
    }

    #[test]
    fn statement_snapshot_replaces_the_result() {
        let mut snapshot = empty_statement("snap");
        snapshot.rows.push(vec![Some(json!(7))]);
        let next = apply_query_stream_event(
            &tab(),
            &QueryStreamEvent::StatementSnapshot {
                index: 1,
                result: snapshot.clone(),
            },
            0,
        );
        assert_eq!(next.results.len(), 2);
        assert_eq!(next.results[1], snapshot);
    }

    #[test]
    fn rows_append_only_to_the_target_and_respect_base_offset() {
        let mut start = tab();
        start.results = vec![empty_statement("a"), empty_statement("b")];
        start.results[0].rows.push(vec![Some(json!(1))]);

        let event = QueryStreamEvent::Rows {
            index: 0,
            rows: vec![vec![Some(json!(2))], vec![Some(json!(3))]],
        };

        let next = apply_query_stream_event(&start, &event, 0);
        assert_eq!(next.results[0].rows.len(), 3);
        assert_eq!(next.results[1].rows.len(), 0);

        let shifted = apply_query_stream_event(&start, &event, 1);
        assert_eq!(shifted.results[0].rows.len(), 1);
        assert_eq!(shifted.results[1].rows.len(), 2);
    }

    #[test]
    fn rows_ignore_an_out_of_range_target() {
        let mut start = tab();
        start.results = vec![empty_statement("a")];
        let next = apply_query_stream_event(
            &start,
            &QueryStreamEvent::Rows {
                index: 5,
                rows: vec![vec![Some(json!(9))]],
            },
            0,
        );
        assert_eq!(next.results.len(), 1);
        assert!(next.results[0].rows.is_empty());
    }

    #[test]
    fn statement_end_sets_fields_only_on_the_target() {
        let mut start = tab();
        start.results = vec![empty_statement("a"), empty_statement("b")];
        let next = apply_query_stream_event(
            &start,
            &QueryStreamEvent::StatementEnd {
                index: 1,
                rows_affected: Some(4),
                execution_time_ms: 25,
                truncated: true,
            },
            0,
        );
        assert_eq!(next.results[0].rows_affected, None);
        assert_eq!(next.results[1].rows_affected, Some(4));
        assert_eq!(next.results[1].execution_time_ms, 25);
        assert_eq!(next.results[1].truncated, Some(true));
    }

    #[test]
    fn done_resets_run_state() {
        let mut start = tab();
        start.execution_id = Some("exec-1".to_owned());
        let next =
            apply_query_stream_event(&start, &QueryStreamEvent::Done { total_time_ms: 99 }, 0);
        assert!(!next.running);
        assert_eq!(next.execution_time_ms, Some(99));
        assert!(next.error.is_none());
        assert!(next.execution_id.is_none());
    }
}
