use std::cell::RefCell;

use serde_json::{json, Map, Value};

use super::*;

fn dialect() -> ImportSqlDialect {
    ImportSqlDialect {
        escape_ident: Box::new(|name, _| format!("\"{}\"", name.replace('"', "\"\""))),
        escape_value: Box::new(|_, value| match value {
            Value::Null => "NULL".to_owned(),
            Value::Number(number) => number.to_string(),
            Value::Bool(flag) => flag.to_string(),
            Value::String(text) => format!("'{}'", text.replace('\'', "''")),
            _ => "NULL".to_owned(),
        }),
    }
}

fn row(id: i64, name: &str) -> Map<String, Value> {
    Map::from_iter([("source_id".to_owned(), json!(id)), ("source_name".to_owned(), json!(name))])
}

fn pairs() -> Vec<ImportColumnPair> {
    vec![
        ImportColumnPair { source: "source_id".to_owned(), target: "id".to_owned() },
        ImportColumnPair { source: "source_name".to_owned(), target: "name".to_owned() },
    ]
}

fn opts(rows: Vec<Map<String, Value>>, commit_mode: ImportCommitMode) -> PlanImportChunksOptions {
    PlanImportChunksOptions {
        table: "users".to_owned(),
        target_columns: vec!["id".to_owned(), "name".to_owned()],
        database_type: None,
        rows,
        column_pairs: pairs(),
        commit_mode,
        max_rows_per_batch: IMPORT_DEFAULT_MAX_ROWS_PER_BATCH,
        max_sql_bytes: IMPORT_DEFAULT_MAX_SQL_BYTES,
        max_batches_per_chunk: IMPORT_DEFAULT_MAX_BATCHES_PER_CHUNK,
    }
}

fn transport_with(results: Vec<Result<u64, String>>) -> (ImportTransport, Rc<RefCell<Vec<String>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let sink = calls.clone();
    let mut remaining = results.into_iter();
    let transport = ImportTransport {
        run_stream: Box::new(move |sql| {
            sink.borrow_mut().push(sql.to_owned());
            remaining.next().unwrap_or(Ok(0))
        }),
        cancel_in_flight: Box::new(|| ()),
    };
    (transport, calls)
}

use std::rc::Rc;

#[test]
fn defaults_and_gap_are_pinned() {
    assert_eq!(IMPORT_DEFAULT_MAX_ROWS_PER_BATCH, 500);
    assert_eq!(IMPORT_DEFAULT_MAX_SQL_BYTES, 512 * 1024);
    assert_eq!(IMPORT_DEFAULT_MAX_BATCHES_PER_CHUNK, 8);
    assert_eq!(IMPORT_STREAM_GAP.len(), 4);
    assert!(IMPORT_STREAM_GAP.contains(&"queryCommands.executeQueryStream"));
}

#[test]
fn build_single_multi_row_insert_is_exact() {
    let rows = vec![
        Map::from_iter([("id".to_owned(), json!(1)), ("name".to_owned(), json!("alice"))]),
        Map::from_iter([("id".to_owned(), json!(2)), ("name".to_owned(), json!("o'brien"))]),
    ];
    let sql = build_multi_row_insert_sql(
        "users",
        &["id".to_owned(), "name".to_owned()],
        None,
        &rows,
        0,
        2,
        &dialect(),
    );
    assert_eq!(
        sql,
        "INSERT INTO \"users\" (\"id\", \"name\") VALUES\n  (1, 'alice'),\n  (2, 'o''brien');"
    );
}

#[test]
fn planning_projects_columns_and_respects_the_row_cap() {
    let rows: Vec<Map<String, Value>> = (0..5).map(|index| row(index, "n")).collect();
    let mut options = opts(rows, ImportCommitMode::Autocommit);
    options.max_rows_per_batch = 2;
    let chunks = plan_import_chunks(&options, &dialect());
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks.iter().map(|chunk| chunk.row_count).collect::<Vec<_>>(), vec![2, 2, 1]);
    assert_eq!(chunks[0].row_start, 0);
    assert_eq!(chunks[1].row_start, 2);
    assert!(chunks[0].sql.contains("'n'"));
    assert!(!chunks[0].sql.contains("source_id"));
}

#[test]
fn planning_respects_the_byte_cap_and_always_makes_progress() {
    let rows: Vec<Map<String, Value>> = (0..4).map(|index| row(index, "a-long-name")).collect();
    let mut options = opts(rows, ImportCommitMode::Autocommit);
    options.max_sql_bytes = 90;
    let chunks = plan_import_chunks(&options, &dialect());
    assert!(chunks.len() > 1);
    assert_eq!(chunks.iter().map(|chunk| chunk.row_count).sum::<usize>(), 4);
    assert!(chunks.iter().all(|chunk| chunk.row_count > 0));
}

#[test]
fn transaction_mode_wraps_batches_and_preserves_their_bounds() {
    let rows: Vec<Map<String, Value>> = (0..6).map(|index| row(index, "n")).collect();
    let mut options = opts(rows, ImportCommitMode::Transaction);
    options.max_rows_per_batch = 1;
    options.max_batches_per_chunk = 2;
    let chunks = plan_import_chunks(&options, &dialect());
    assert_eq!(chunks.len(), 3);
    assert!(chunks[0].sql.starts_with("BEGIN;\n"));
    assert!(chunks[0].sql.ends_with("\nCOMMIT;"));
    assert_eq!(chunks[0].batches.len(), 2);
    assert_eq!(chunks[0].row_start, 0);
    assert_eq!(chunks[0].row_count, 2);
    assert_eq!(chunks[2].row_start, 4);
}

#[test]
fn autocommit_mode_makes_each_batch_its_own_chunk() {
    let rows: Vec<Map<String, Value>> = (0..3).map(|index| row(index, "n")).collect();
    let mut options = opts(rows, ImportCommitMode::Autocommit);
    options.max_rows_per_batch = 1;
    options.max_batches_per_chunk = 3;
    let chunks = plan_import_chunks(&options, &dialect());
    assert_eq!(chunks.len(), 3);
    assert!(chunks.iter().all(|chunk| chunk.batches.len() == 1));
    assert!(chunks.iter().all(|chunk| !chunk.sql.contains("BEGIN")));
}

#[test]
fn a_full_run_reports_ordered_progress_and_exact_commits() {
    let rows: Vec<Map<String, Value>> = (0..4).map(|index| row(index, "n")).collect();
    let mut options = opts(rows, ImportCommitMode::Autocommit);
    options.max_rows_per_batch = 2;
    let (mut transport, calls) = transport_with(vec![Ok(2), Ok(2)]);
    let seen = RefCell::new(Vec::new());
    let mut progress = |event: &ImportProgress| {
        seen.borrow_mut().push((event.status, event.processed_rows, event.committed_rows, event.failed_rows));
    };
    let result = execute_import(
        &options,
        &dialect(),
        &[1, 2, 3, 4],
        ImportErrorStrategy::Stop,
        &AbortSignal::default(),
        &mut transport,
        Some(&mut progress),
    );
    assert_eq!(result.status, ImportFinalStatus::Completed);
    assert_eq!(result.committed_rows, 4);
    assert!(result.failed_rows.is_empty());
    assert_eq!(result.stopped_by, None);
    assert_eq!(calls.borrow().len(), 2);
    assert_eq!(
        seen.borrow().as_slice(),
        &[
            (ImportProgressStatus::Running, 2, 2, 0),
            (ImportProgressStatus::Running, 4, 4, 0),
            (ImportProgressStatus::Completed, 4, 4, 0),
        ]
    );
}

#[test]
fn stop_strategy_stops_at_the_first_failing_chunk() {
    let rows: Vec<Map<String, Value>> = (0..4).map(|index| row(index, "n")).collect();
    let mut options = opts(rows, ImportCommitMode::Autocommit);
    options.max_rows_per_batch = 2;
    let (mut transport, calls) = transport_with(vec![Ok(2), Err("boom".to_owned())]);
    let result = execute_import(
        &options,
        &dialect(),
        &[10, 20, 30, 40],
        ImportErrorStrategy::Stop,
        &AbortSignal::default(),
        &mut transport,
        None,
    );
    assert_eq!(result.status, ImportFinalStatus::Stopped);
    assert_eq!(result.committed_rows, 2);
    let stopped = result.stopped_by.expect("stopped by");
    assert_eq!(stopped.row_index, 2);
    assert_eq!(stopped.source_line, Some(30));
    assert_eq!(stopped.message, "boom");
    assert_eq!(calls.borrow().len(), 2, "no row-by-row retry in stop mode");
}

#[test]
fn skip_strategy_retries_a_failed_chunk_row_by_row() {
    let rows: Vec<Map<String, Value>> = (0..3).map(|index| row(index, "n")).collect();
    let mut options = opts(rows, ImportCommitMode::Transaction);
    options.max_rows_per_batch = 3;
    let (mut transport, calls) = transport_with(vec![
        Err("chunk failed".to_owned()),
        Ok(1),
        Err("row failed".to_owned()),
        Ok(1),
    ]);
    let result = execute_import(
        &options,
        &dialect(),
        &[7, 8, 9],
        ImportErrorStrategy::Skip,
        &AbortSignal::default(),
        &mut transport,
        None,
    );
    assert_eq!(result.status, ImportFinalStatus::Completed);
    assert_eq!(result.committed_rows, 2);
    assert_eq!(result.failed_rows.len(), 1);
    assert_eq!(result.failed_rows[0].row_index, 1);
    assert_eq!(result.failed_rows[0].source_line, Some(8));
    assert_eq!(result.failed_rows[0].message, "row failed");
    assert_eq!(calls.borrow().len(), 4);
    assert!(!calls.borrow()[1].contains("BEGIN"), "the retry is a single-row INSERT");
}

#[test]
fn abort_before_a_chunk_cancels_without_running_it() {
    let rows: Vec<Map<String, Value>> = (0..2).map(|index| row(index, "n")).collect();
    let options = opts(rows, ImportCommitMode::Autocommit);
    let (mut transport, calls) = transport_with(vec![Ok(2)]);
    let signal = AbortSignal::default();
    signal.abort();
    let result = execute_import(
        &options,
        &dialect(),
        &[],
        ImportErrorStrategy::Stop,
        &signal,
        &mut transport,
        None,
    );
    assert_eq!(result.status, ImportFinalStatus::Cancelled);
    assert_eq!(result.committed_rows, 0);
    assert!(calls.borrow().is_empty());
}

#[test]
fn a_rejection_after_abort_is_reported_as_cancellation() {
    let rows: Vec<Map<String, Value>> = (0..2).map(|index| row(index, "n")).collect();
    let options = opts(rows, ImportCommitMode::Autocommit);
    let signal = Rc::new(AbortSignal::default());
    let shared = signal.clone();
    let mut transport = ImportTransport {
        run_stream: Box::new(move |_| {
            shared.abort();
            Err("driver error".to_owned())
        }),
        cancel_in_flight: Box::new(|| ()),
    };
    let result = execute_import(
        &options,
        &dialect(),
        &[],
        ImportErrorStrategy::Stop,
        &signal,
        &mut transport,
        None,
    );
    assert_eq!(result.status, ImportFinalStatus::Cancelled);
    assert_eq!(result.failed_rows.len(), 0, "a cancellation is not a row failure");
}

#[test]
fn abort_during_skip_retries_stops_the_world() {
    let rows: Vec<Map<String, Value>> = (0..3).map(|index| row(index, "n")).collect();
    let mut options = opts(rows, ImportCommitMode::Transaction);
    options.max_rows_per_batch = 3;
    let (mut transport, calls) = transport_with(vec![Err("chunk".to_owned()), Ok(1)]);
    let signal = Rc::new(AbortSignal::default());
    let shared = signal.clone();
    let mut guarded = ImportTransport {
        run_stream: Box::new(move |sql| {
            if sql.contains("BEGIN") {
                shared.abort();
            }
            (transport.run_stream)(sql)
        }),
        cancel_in_flight: Box::new(|| ()),
    };
    let result = execute_import(
        &options,
        &dialect(),
        &[],
        ImportErrorStrategy::Skip,
        &signal,
        &mut guarded,
        None,
    );
    assert_eq!(result.status, ImportFinalStatus::Cancelled);
    assert_eq!(calls.borrow().len(), 1, "no row retry starts after the abort");
}

#[test]
fn failures_only_carry_error_text_not_row_data_or_credentials() {
    let rows = vec![row(1, "secret-password-value")];
    let options = opts(rows, ImportCommitMode::Autocommit);
    let (mut transport, _) = transport_with(vec![
        Err("db: permission denied".to_owned()),
        Err("db: permission denied".to_owned()),
    ]);
    let result = execute_import(
        &options,
        &dialect(),
        &[42],
        ImportErrorStrategy::Skip,
        &AbortSignal::default(),
        &mut transport,
        None,
    );
    assert_eq!(result.status, ImportFinalStatus::Completed);
    assert_eq!(result.failed_rows.len(), 1);
    assert_eq!(result.failed_rows[0].message, "db: permission denied");
    assert!(!result.failed_rows[0].message.contains("secret-password-value"));
}

#[test]
fn stream_event_fold_tracks_execution_id_and_affected_rows() {
    let mut progress = ImportStreamProgress::default();
    progress.on_event(&ImportStreamEvent::StatementEnd { rows_affected: 2 });
    progress.on_event(&ImportStreamEvent::ExecutionStarted { execution_id: "e1".to_owned() });
    progress.on_event(&ImportStreamEvent::StatementEnd { rows_affected: 3 });
    progress.on_event(&ImportStreamEvent::Other);
    assert_eq!(progress.execution_id(), Some("e1"));
    assert_eq!(progress.total_affected(), 5);
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::build_multi_row_insert_sql;
    let _ = crate::plan_import_chunks;
    let _ = crate::execute_import;
    assert_eq!(crate::IMPORT_DEFAULT_MAX_ROWS_PER_BATCH, 500);
    let _: Option<crate::ImportRunResult> = None;
    let _: Option<crate::ImportProgress> = None;
    let _: Option<crate::ImportRowFailure> = None;
    let _: Option<crate::ImportTransport> = None;
    let _: Option<crate::ImportSqlDialect> = None;
    let _: Option<crate::PlanImportChunksOptions> = None;
    let _: Option<crate::AbortSignal> = None;
    let _: Option<crate::ImportStreamProgress> = None;
    let _: Option<crate::ImportStreamEvent> = None;
    let _: Option<crate::ImportCommitMode> = None;
    let _: Option<crate::ImportErrorStrategy> = None;
    let _: Option<crate::ImportColumnPair> = None;
    let _: Option<crate::ImportBatch> = None;
    let _: Option<crate::ImportChunk> = None;
}
