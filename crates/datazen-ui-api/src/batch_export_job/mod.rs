//! Batch export jobs — port of React `packages/frontend/src/lib/batchExportJob.ts`
//! (352 lines, task T326).
//!
//! The file exports three entry points plus their option/result shapes. It is
//! *orchestration over injected ports*, not a store and not IPC:
//!
//! - [`run_batch_export_job`] — the real batch job. One combined Rust request
//!   for N tables; JS/Rune only gathers per-table metadata (DDL + column
//!   names) and forwards it — no table is buffered in memory here.
//! - [`export_single_table_via_rust`] — the thin wrapper around the
//!   `SqlDataFetcher` export (a cross-task port); this file only derives the
//!   fetcher config, publishes its cancel handle and maps the outcome.
//! - [`export_single_table_structure_via_rust`] — one `structure_only` request
//!   with a `sql_insert` format so the native dialog offers `.sql`.
//!
//! ## Stage sequence / state machine of the batch job
//!
//! `validate` → `mint token + publish cancel handle` → `per-table metadata
//! (progress before each load)` → `final cancel guard` → `one exporter call`
//! → `terminal { saved | cancelled | aborted }`. Every branch is one-shot;
//! this file has no resume/retry API — recovery belongs to the kernel's job
//! system. The only state that survives a call boundary is the idempotent
//! cancel flag in [`CancelHandle`], shared with the host's Cancel button.
//!
//! ## Seams (no IO owned here)
//!
//! React's injected async functions become `FnMut` fields and every `await`
//! becomes `Result` propagation (the T040 `ai_sessions` precedent):
//! `crypto.randomUUID` → `new_id`; `loadTableExportData` → the loader;
//! `exportTables` → the exporter; `cancelExport` → the canceller;
//! `onProgress` / `onCancelHandle` → callbacks. The `ExportTablesRequest` /
//! `ExportTablesResult` shapes are *local mirrors* of `commands/file.ts` (T298
//! owns the real port) so this crate stays independent of that unlanded task.
//!
//! ## Coverage / deviations (each pinned by a test)
//!
//! - React's `sql ? 'filtered_table' : 'entire_table'` uses JS truthiness — an
//!   empty statement is treated as absent for the scope but still forwarded
//!   verbatim in the config.
//! - `meta.tableName ?? tableName` is modelled as `Option` so a loader double
//!   that omits the name falls back to the requested one; `Some("")` is kept.
//! - `fetcher.abortedPartialDeleted ?? false` is an `Option<bool>` in the
//!   single-fetch outcome, so "never set" reports "kept", not "deleted".
//! - A cancel that lands during the last table's load is caught by the final
//!   guard; a cancel during an earlier load skips the remaining loads entirely.
//! - No production `unwrap`/`expect`; all failures are `Err(String)`.
//!
//! ## Layout
//!
//! - [`types`] — the React enums/interfaces and the `commands/file.ts` mirrors.
//! - [`runner`] — the three functions, their options and [`CancelHandle`].
//! - tests live here (ui-api has no `tests/mod.rs`).

mod runner;
mod types;

pub use runner::{
    export_single_table_structure_via_rust, export_single_table_via_rust, run_batch_export_job,
    CancelHandle, RunBatchExportJobOptions, SingleTableExportOptions,
    SingleTableStructureExportOptions,
};
pub use types::{
    BatchExportColumn, BatchExportDataFormat, BatchExportMode, BatchExportOutputMode,
    BatchExportProgress, BatchExportTableInput, ExportTableInput, ExportTablesRequest,
    ExportTablesResult, LoadedTableExportData, RunBatchExportJobResult, SingleFetchOutcome,
    SingleTableExportResult, SingleTableFetchConfig, SingleTableFetchScope,
    SingleTableStructureExportResult,
};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    /// Records every seam observation so a test can pin stage order and the
    /// exact request that reached the exporter.
    #[derive(Default)]
    struct Recorder {
        requests: Rc<RefCell<Vec<ExportTablesRequest>>>,
        progress: Rc<RefCell<Vec<BatchExportProgress>>>,
        loads: Rc<RefCell<Vec<String>>>,
        cancels: Rc<RefCell<Vec<String>>>,
        ids: Rc<RefCell<usize>>,
        handles: Rc<RefCell<Vec<CancelHandle>>>,
    }

    impl Recorder {
        /// A happy-path options set: metadata with `id`/`name` columns, an
        /// exporter that answers `Saved`, a single incrementing token.
        fn options(&self, table_names: &[&str]) -> RunBatchExportJobOptions {
            let requests = self.requests.clone();
            let progress = self.progress.clone();
            let loads = self.loads.clone();
            let cancels = self.cancels.clone();
            let ids = self.ids.clone();
            let handles = self.handles.clone();
            RunBatchExportJobOptions {
                table_names: table_names.iter().map(|name| (*name).to_owned()).collect(),
                mode: BatchExportMode::DataOnly,
                data_format: BatchExportDataFormat::Csv,
                output_mode: BatchExportOutputMode::Single,
                database_type: Some("postgres".to_owned()),
                db_session_id: "s1".to_owned(),
                database: Some("main".to_owned()),
                schema: Some("public".to_owned()),
                load_table_export_data: Box::new(move |name| {
                    loads.borrow_mut().push(name.to_owned());
                    Ok(LoadedTableExportData {
                        columns: vec![
                            BatchExportColumn { name: "id".into() },
                            BatchExportColumn { name: "name".into() },
                        ],
                        ..Default::default()
                    })
                }),
                on_progress: Some(Box::new(move |event| {
                    progress.borrow_mut().push(event.clone());
                })),
                export_tables: Box::new(move |request| {
                    requests.borrow_mut().push(request.clone());
                    Ok(Some(ExportTablesResult::Saved(2)))
                }),
                cancel_export: Box::new(move |token| {
                    cancels.borrow_mut().push(token.to_owned());
                    Ok(true)
                }),
                on_cancel_handle: Some(Box::new(move |handle| {
                    handles.borrow_mut().push(handle);
                })),
                new_id: Box::new(move || {
                    let mut next = ids.borrow_mut();
                    *next += 1;
                    format!("token-{next}")
                }),
            }
        }

        fn handle(&self) -> CancelHandle {
            self.handles.borrow().first().cloned().expect("a handle was published")
        }
    }

    #[test]
    fn happy_path_progress_and_one_combined_request() {
        let rec = Recorder::default();
        let result = run_batch_export_job(rec.options(&["users", "orders"])).expect("run");
        assert_eq!(result, RunBatchExportJobResult::Saved);

        // One progress event per table, 1-based, before the load.
        assert_eq!(
            rec.progress.borrow().as_slice(),
            &[
                BatchExportProgress { current: 1, total: 2, table_name: "users".into() },
                BatchExportProgress { current: 2, total: 2, table_name: "orders".into() },
            ]
        );
        assert_eq!(rec.loads.borrow().as_slice(), &["users".to_owned(), "orders".to_owned()]);
        assert_eq!(*rec.ids.borrow(), 1, "one UUID for the whole batch");
        assert!(rec.cancels.borrow().is_empty());

        let requests = rec.requests.borrow();
        assert_eq!(requests.len(), 1, "a single combined Rust request");
        let request = &requests[0];
        assert_eq!(request.db_session_id, "s1");
        assert_eq!(request.cancel_token.as_deref(), Some("token-1"));
        assert_eq!(request.database_type.as_deref(), Some("postgres"));
        assert_eq!(request.database.as_deref(), Some("main"));
        assert_eq!(request.schema.as_deref(), Some("public"));
        assert_eq!(request.mode, BatchExportMode::DataOnly);
        assert_eq!(request.data_format, BatchExportDataFormat::Csv);
        assert_eq!(request.output_mode, BatchExportOutputMode::Single);
        assert_eq!(
            request.tables,
            vec![
                ExportTableInput {
                    table_name: "users".into(),
                    schema: None,
                    columns: vec!["id".into(), "name".into()],
                    ddl: None,
                },
                ExportTableInput {
                    table_name: "orders".into(),
                    schema: None,
                    columns: vec!["id".into(), "name".into()],
                    ddl: None,
                },
            ]
        );
    }

    #[test]
    fn empty_and_missing_session_reject_before_any_seam_is_called() {
        let rec = Recorder::default();
        assert_eq!(
            run_batch_export_job(rec.options(&[])).unwrap_err(),
            "no_tables_selected"
        );
        assert_eq!(*rec.ids.borrow(), 0, "validation precedes the token mint");
        assert!(rec.requests.borrow().is_empty());

        let rec = Recorder::default();
        let mut options = rec.options(&["users"]);
        options.db_session_id.clear();
        assert_eq!(run_batch_export_job(options).unwrap_err(), "Missing connection");
        assert_eq!(*rec.ids.borrow(), 0);
        assert!(rec.requests.borrow().is_empty());
        assert!(rec.handles.borrow().is_empty(), "no handle is published on failure");
    }

    #[test]
    fn dirty_loader_metadata_falls_back_or_is_kept_verbatim() {
        let rec = Recorder::default();
        let mut options = rec.options(&["users", "orders"]);
        options.load_table_export_data = Box::new(|name| {
            if name == "users" {
                Ok(LoadedTableExportData {
                    table_name: None, // React's `meta.tableName ?? tableName`
                    schema: Some("custom".into()),
                    ddl: Some("CREATE TABLE users (...)".into()),
                    columns: Vec::new(),
                    rows: Vec::new(),
                })
            } else {
                Ok(LoadedTableExportData {
                    table_name: Some(String::new()), // `??` keeps an empty name
                    columns: vec![BatchExportColumn { name: "x".into() }],
                    ..Default::default()
                })
            }
        });
        assert_eq!(run_batch_export_job(options).expect("run"), SingleTableExportResult::Saved);

        let requests = rec.requests.borrow();
        assert_eq!(requests[0].tables[0].table_name, "users", "missing name falls back");
        assert_eq!(requests[0].tables[0].schema.as_deref(), Some("custom"));
        assert_eq!(requests[0].tables[0].ddl.as_deref(), Some("CREATE TABLE users (...)"));
        assert!(requests[0].tables[0].columns.is_empty());
        assert_eq!(requests[0].tables[1].table_name, "", "an empty name is not a fallback");
        assert_eq!(requests[0].tables[1].columns, vec!["x".to_owned()]);
    }

    #[test]
    fn cancel_at_a_loop_guard_skips_remaining_loads_and_the_exporter() {
        let rec = Recorder::default();
        let mut options = rec.options(&["t1", "t2", "t3"]);
        let slot = Rc::new(RefCell::new(None));
        let store = slot.clone();
        options.on_cancel_handle = Some(Box::new(move |handle| *store.borrow_mut() = Some(handle)));
        let watch = slot.clone();
        options.on_progress = Some(Box::new(move |event| {
            if event.current == 2 {
                if let Some(handle) = watch.borrow().as_ref() {
                    handle.cancel();
                }
            }
        }));

        assert_eq!(
            run_batch_export_job(options).expect("resolved, not rejected"),
            SingleTableExportResult::Aborted { partial_deleted: true }
        );
        assert_eq!(rec.loads.borrow().as_slice(), &["t1".to_owned(), "t2".to_owned()]);
        assert!(rec.requests.borrow().is_empty(), "the exporter is never called after a cancel");
        assert_eq!(rec.cancels.borrow().as_slice(), &["token-1".to_owned()]);
    }

    #[test]
    fn cancel_on_the_last_load_is_caught_by_the_final_guard() {
        let rec = Recorder::default();
        let mut options = rec.options(&["t1", "t2"]);
        let slot = Rc::new(RefCell::new(None));
        let store = slot.clone();
        options.on_cancel_handle = Some(Box::new(move |handle| *store.borrow_mut() = Some(handle)));
        let watch = slot.clone();
        let loads = Rc::new(RefCell::new(0usize));
        let count = loads.clone();
        options.load_table_export_data = Box::new(move |name| {
            *count.borrow_mut() += 1;
            if name == "t2" {
                if let Some(handle) = watch.borrow().as_ref() {
                    handle.cancel();
                }
            }
            Ok(LoadedTableExportData::default())
        });

        assert_eq!(
            run_batch_export_job(options).expect("resolved, not rejected"),
            SingleTableExportResult::Aborted { partial_deleted: true }
        );
        assert_eq!(*loads.borrow(), 2, "both loads happen; the final guard catches it");
        assert!(rec.requests.borrow().is_empty());
    }

    #[test]
    fn the_cancel_handle_is_idempotent() {
        let rec = Recorder::default();
        let options = rec.options(&["t1"]);
        run_batch_export_job(options).expect("run");
        let handle = rec.handle();
        handle.cancel();
        handle.cancel();
        assert_eq!(rec.cancels.borrow().len(), 1, "one token cancel per run");
        assert!(handle.is_requested());
    }

    #[test]
    fn exporter_results_map_to_the_three_honest_states() {
        let outcomes = [
            (Some(ExportTablesResult::Saved(7)), SingleTableExportResult::Saved),
            (Some(ExportTablesResult::Cancelled), SingleTableExportResult::Cancelled),
            (None, SingleTableExportResult::Cancelled),
            (
                Some(ExportTablesResult::Aborted { partial_deleted: false }),
                SingleTableExportResult::Aborted { partial_deleted: false },
            ),
            (
                Some(ExportTablesResult::Aborted { partial_deleted: true }),
                SingleTableExportResult::Aborted { partial_deleted: true },
            ),
        ];
        for (outcome, expected) in outcomes {
            let rec = Recorder::default();
            let mut options = rec.options(&["t1"]);
            let calls = Rc::new(RefCell::new(0usize));
            let count = calls.clone();
            options.export_tables = Box::new(move |_| {
                *count.borrow_mut() += 1;
                Ok(outcome)
            });
            let result = run_batch_export_job(options).expect("run");
            assert_eq!(result, expected);
            assert_eq!(result.status(), expected.status());
            assert_eq!(*calls.borrow(), 1, "the exporter is called exactly once");
        }
    }

    #[test]
    fn loader_and_exporter_errors_propagate_as_rejections() {
        let rec = Recorder::default();
        let mut options = rec.options(&["t1", "t2"]);
        options.load_table_export_data = Box::new(|_| Err("load failed".into()));
        assert_eq!(run_batch_export_job(options).unwrap_err(), "load failed");
        assert!(rec.requests.borrow().is_empty(), "a failed load stops before export");

        let rec = Recorder::default();
        let mut options = rec.options(&["t1"]);
        options.export_tables = Box::new(|_| Err("export failed".into()));
        assert_eq!(run_batch_export_job(options).unwrap_err(), "export failed");
    }

    #[test]
    fn structure_export_builds_its_exact_request_and_maps_every_result() {
        fn structure_options(
            session: &str,
            result: Option<ExportTablesResult>,
            seen: &Rc<RefCell<Vec<ExportTablesRequest>>>,
        ) -> SingleTableStructureExportOptions {
            let sink = seen.clone();
            SingleTableStructureExportOptions {
                db_session_id: session.to_owned(),
                database_type: Some("postgres".to_owned()),
                database: Some("main".to_owned()),
                schema: Some("public".to_owned()),
                table_name: "users".to_owned(),
                ddl: Some("CREATE TABLE users (...)".to_owned()),
                export_tables: Box::new(move |request| {
                    sink.borrow_mut().push(request.clone());
                    Ok(result)
                }),
            }
        }

        let seen = Rc::new(RefCell::new(Vec::new()));
        let result = export_single_table_structure_via_rust(structure_options(
            "s1",
            Some(ExportTablesResult::Saved(1)),
            &seen,
        ))
        .expect("run");
        assert_eq!(result, SingleTableStructureExportResult::Saved);
        assert_eq!(result.as_str(), "saved");
        let recorded = seen.borrow();
        let request = &recorded[0];
        assert_eq!(request.db_session_id, "s1");
        assert_eq!(request.cancel_token, None, "the structure path mints no token");
        assert_eq!(request.mode, BatchExportMode::StructureOnly);
        assert_eq!(request.data_format, BatchExportDataFormat::SqlInsert);
        assert_eq!(request.output_mode, BatchExportOutputMode::Single);
        assert_eq!(request.schema.as_deref(), Some("public"));
        assert_eq!(
            request.tables,
            vec![ExportTableInput {
                table_name: "users".into(),
                schema: Some("public".into()),
                columns: Vec::new(),
                ddl: Some("CREATE TABLE users (...)".into()),
            }]
        );

        for outcome in [
            Some(ExportTablesResult::Cancelled),
            None,
            Some(ExportTablesResult::Aborted { partial_deleted: true }),
        ] {
            let seen = Rc::new(RefCell::new(Vec::new()));
            assert_eq!(
                export_single_table_structure_via_rust(structure_options("s1", outcome, &seen))
                    .expect("run"),
                SingleTableStructureExportResult::Cancelled
            );
        }

        let seen = Rc::new(RefCell::new(Vec::new()));
        assert_eq!(
            export_single_table_structure_via_rust(structure_options("", None, &seen)).unwrap_err(),
            "Missing connection"
        );
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn single_table_scope_follows_js_truthiness_and_config_is_pinned() {
        fn single_options(
            sql: Option<&str>,
            outcome: SingleFetchOutcome,
            seen: &Rc<RefCell<Vec<SingleTableFetchConfig>>>,
            published: &Rc<RefCell<bool>>,
        ) -> SingleTableExportOptions {
            let sink = seen.clone();
            let flag_read = published.clone();
            let flag_write = published.clone();
            SingleTableExportOptions {
                db_session_id: "s1".to_owned(),
                database_type: Some("postgres".to_owned()),
                database: Some("main".to_owned()),
                schema: Some("public".to_owned()),
                table_name: "users".to_owned(),
                columns: vec!["id".into(), "name".into()],
                data_format: BatchExportDataFormat::Xlsx,
                sql: sql.map(str::to_owned),
                primary_key: Some("id".to_owned()),
                fetch: Box::new(move |config| {
                    assert!(*flag_read.borrow(), "the cancel handle is published before fetch");
                    sink.borrow_mut().push(config.clone());
                    Ok(outcome)
                }),
                cancel_fetch: Box::new(|| Ok(true)),
                on_cancel_handle: Some(Box::new(move |_| *flag_write.borrow_mut() = true)),
            }
        }

        let seen = Rc::new(RefCell::new(Vec::new()));
        let published = Rc::new(RefCell::new(false));
        let result = export_single_table_via_rust(single_options(
            Some("SELECT * FROM users WHERE id = 1"),
            SingleFetchOutcome::Aborted { partial_deleted: None },
            &seen,
            &published,
        ))
        .expect("run");
        assert_eq!(
            result,
            SingleTableExportResult::Aborted { partial_deleted: false },
            "the ?? false fallback reports 'kept'"
        );
        let recorded = seen.borrow();
        let config = &recorded[0];
        assert_eq!(config.scope, SingleTableFetchScope::FilteredTable);
        assert_eq!(config.scope.as_str(), "filtered_table");
        assert_eq!(config.table_name, "users");
        assert_eq!(config.db_session_id, "s1");
        assert_eq!(config.database.as_deref(), Some("main"));
        assert_eq!(config.schema.as_deref(), Some("public"));
        assert_eq!(config.column_names, vec!["id".to_owned(), "name".to_owned()]);
        assert_eq!(config.primary_key.as_deref(), Some("id"));
        assert_eq!(config.mode, BatchExportMode::DataOnly);
        assert_eq!(config.output_mode, BatchExportOutputMode::Single);
        assert_eq!(config.data_format, BatchExportDataFormat::Xlsx);

        for (sql, expected) in [
            (Some(""), SingleTableFetchScope::EntireTable),
            (None, SingleTableFetchScope::EntireTable),
        ] {
            let seen = Rc::new(RefCell::new(Vec::new()));
            let published = Rc::new(RefCell::new(false));
            export_single_table_via_rust(single_options(
                sql,
                SingleFetchOutcome::Saved,
                &seen,
                &published,
            ))
            .expect("run");
            let recorded = seen.borrow();
            let config = &recorded[0];
            assert_eq!(config.scope, expected, "an empty statement is JS-falsy");
            assert_eq!(config.sql.as_deref(), sql, "the statement is forwarded verbatim");
        }

        let seen = Rc::new(RefCell::new(Vec::new()));
        let published = Rc::new(RefCell::new(false));
        assert_eq!(
            export_single_table_via_rust(single_options(
                Some("select 1"),
                SingleFetchOutcome::Aborted { partial_deleted: Some(true) },
                &seen,
                &published,
            ))
            .expect("run"),
            SingleTableExportResult::Aborted { partial_deleted: true }
        );

        let seen = Rc::new(RefCell::new(Vec::new()));
        let published = Rc::new(RefCell::new(false));
        assert_eq!(
            export_single_table_via_rust(single_options(
                None,
                SingleFetchOutcome::Cancelled,
                &seen,
                &published,
            ))
            .expect("run"),
            SingleTableExportResult::Cancelled
        );
    }

    #[test]
    fn single_table_fetch_errors_propagate_and_cancel_is_idempotent() {
        fn failing_options(
            calls: &Rc<RefCell<usize>>,
            cancellations: &Rc<RefCell<usize>>,
            handle_slot: &Rc<RefCell<Option<CancelHandle>>>,
        ) -> SingleTableExportOptions {
            let sink = calls.clone();
            let counts = cancellations.clone();
            let store = handle_slot.clone();
            SingleTableExportOptions {
                db_session_id: "s1".into(),
                database_type: None,
                database: None,
                schema: None,
                table_name: "t".into(),
                columns: Vec::new(),
                data_format: BatchExportDataFormat::Csv,
                sql: None,
                primary_key: None,
                fetch: Box::new(move |_| {
                    *sink.borrow_mut() += 1;
                    Err("fetch failed".into())
                }),
                cancel_fetch: Box::new(move || {
                    *counts.borrow_mut() += 1;
                    Ok(true)
                }),
                on_cancel_handle: Some(Box::new(move |handle| *store.borrow_mut() = Some(handle))),
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let cancellations = Rc::new(RefCell::new(0usize));
        let handle_slot = Rc::new(RefCell::new(None));
        let options = failing_options(&calls, &cancellations, &handle_slot);
        assert_eq!(export_single_table_via_rust(options).unwrap_err(), "fetch failed");
        assert_eq!(*calls.borrow(), 1);
        let handle = handle_slot.borrow().clone().expect("a handle was published");
        handle.cancel();
        handle.cancel();
        assert_eq!(*cancellations.borrow(), 1, "cancel_fetch is called once");
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        // Pin every public name against removal, the same way `ai_providers`
        // and `ai_query_actions` pin their crate-root surfaces.
        let _ = crate::run_batch_export_job;
        let _ = crate::export_single_table_via_rust;
        let _ = crate::export_single_table_structure_via_rust;
        let _: Option<crate::CancelHandle> = None;
        let _: Option<crate::RunBatchExportJobOptions> = None;
        let _: Option<crate::SingleTableExportOptions> = None;
        let _: Option<crate::SingleTableStructureExportOptions> = None;
        let _: Option<crate::BatchExportColumn> = None;
        let _: Option<crate::BatchExportDataFormat> = None;
        let _: Option<crate::BatchExportMode> = None;
        let _: Option<crate::BatchExportOutputMode> = None;
        let _: Option<crate::BatchExportProgress> = None;
        let _: Option<crate::BatchExportTableInput> = None;
        let _: Option<crate::ExportTableInput> = None;
        let _: Option<crate::ExportTablesRequest> = None;
        let _: Option<crate::ExportTablesResult> = None;
        let _: Option<crate::LoadedTableExportData> = None;
        let _: Option<crate::RunBatchExportJobResult> = None;
        let _: Option<crate::SingleFetchOutcome> = None;
        let _: Option<crate::SingleTableExportResult> = None;
        let _: Option<crate::SingleTableFetchConfig> = None;
        let _: Option<crate::SingleTableFetchScope> = None;
        let _: Option<crate::SingleTableStructureExportResult> = None;
    }
}
