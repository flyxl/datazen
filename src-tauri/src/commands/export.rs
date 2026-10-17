//! Desktop file picker and source/event bindings for the portable export service.
use super::{AppState, error::CommandError};
use datazen_driver_api::{DatabaseDriver, Value};
pub use datazen_feature_core::export::*;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
struct DesktopExportSource {
    runtime: crate::services::runtime_command_source::RuntimeCommandSource,
    registry: Arc<crate::db::DriverRegistry>,
    cancel: datazen_runtime::job::CancelToken,
}
impl DesktopExportSource {
    async fn read_command(
        &self,
        session_id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, datazen_feature_core::error::FeatureError> {
        let operation = self
            .runtime
            .read_session_command(session_id, command, input);
        let cancelled = async {
            while !self.cancel.is_cancelled() {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        };
        tokio::select! {
            result = operation => result
                .map_err(datazen_feature_core::error::FeatureError::Internal),
            _ = cancelled => Err(datazen_feature_core::error::FeatureError::Validation("fileJobCancelled".into())),
        }
    }
}
#[async_trait::async_trait]
impl ExportSource for DesktopExportSource {
    async fn resolve_source(
        &self,
        id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, bool, bool), datazen_feature_core::error::FeatureError>
    {
        let (_, config, _) = self
            .runtime
            .profile_for_session(id)
            .await
            .map_err(datazen_feature_core::error::FeatureError::Internal)?;
        let driver = self
            .registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| {
                datazen_feature_core::error::FeatureError::Validation(
                    "Export driver unavailable".into(),
                )
            })?;
        Ok((
            driver,
            config.read_only,
            self.runtime.execution_safe_mode().await,
        ))
    }
    async fn stream_query(
        &self,
        id: &str,
        sql: &str,
        target: datazen_driver_api::SqlTarget<'_>,
        callback: datazen_driver_api::QueryStreamCallback,
    ) -> Result<(), datazen_feature_core::error::FeatureError> {
        if self.cancel.is_cancelled() {
            return Err(datazen_feature_core::error::FeatureError::Validation(
                "fileJobCancelled".into(),
            ));
        }
        let data = self.read_command(
            id,
            "query",
            serde_json::json!({"sql":sql,"database":target.database(),"schema":target.schema()}),
        ).await?;
        let result: datazen_driver_api::MultiQueryResult = serde_json::from_value(data)?;
        if result.results.iter().any(|statement| statement.truncated) {
            return Err(datazen_feature_core::error::FeatureError::Validation(
                "export source result was truncated".into(),
            ));
        }
        datazen_driver_api::emit_multi_query_as_stream(result, &callback);
        Ok(())
    }
}
struct ExportOperation {
    store: Arc<crate::store::Store>,
    registry: Arc<crate::db::DriverRegistry>,
    platform: crate::platform::PlatformEntry,
    request: ExportTablesRequest,
    app: AppHandle,
    staging: tempfile::TempDir,
}
#[async_trait::async_trait]
impl datazen_feature_core::file_job::FileJobOperation for ExportOperation {
    async fn run(
        &self,
        cancel: &datazen_runtime::job::CancelToken,
    ) -> Result<
        datazen_feature_core::file_job::FileJobOutput,
        datazen_feature_core::error::FeatureError,
    > {
        let (plans, zip) = prepare_export(&self.request)?;
        let path = self.staging.path().join(if zip {
            "export.zip".to_string()
        } else {
            format!("export.{}", self.request.data_format.extension())
        });
        let source = DesktopExportSource {
            runtime: crate::services::runtime_command_source::RuntimeCommandSource::new(
                self.registry.clone(),
                self.store.clone(),
                self.platform.clone(),
            ),
            registry: self.registry.clone(),
            cancel: cancel.clone(),
        };
        let events = DesktopExportEvents(self.app.clone());
        let rows = if zip {
            run_zip_export(&events, &source, &self.request, &plans, &path).await?
        } else {
            run_single_export(&events, &source, &self.request, &plans, &path).await?
        };
        Ok(datazen_feature_core::file_job::FileJobOutput { path, rows })
    }
}
struct DesktopExportEvents(AppHandle);
impl ExportEvents for DesktopExportEvents {
    fn progress_callback(&self) -> Arc<dyn Fn(ExportProgressEvent) + Send + Sync> {
        let app = self.0.clone();
        Arc::new(move |progress| {
            let _ = app.emit("batch-export-progress", progress);
        })
    }
}
#[tauri::command]
pub async fn export_tables_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExportTablesRequest,
) -> Result<ExportTablesResult, CommandError> {
    let use_zip = prepare_export(&request)?.1;

    let ext = if use_zip {
        "zip"
    } else {
        request.data_format.extension()
    };
    let picked = super::dialog::save_file(
        &app,
        ("Export".into(), vec![ext.to_string()]),
        default_export_name(&request, use_zip),
    )
    .await?;
    let Some(target) = picked else {
        return Ok(ExportTablesResult::Cancelled);
    };
    validate_export_path(&target)?;

    let session = request.db_session_id.clone();
    let database = request.database.clone();
    let operation = ExportOperation {
        store: state.store.clone(),
        registry: state.driver_registry.clone(),
        platform: state.platform.clone(),
        request,
        app: app.clone(),
        staging: tempfile::tempdir()?,
    };
    let rows_total = super::file_jobs::run_output_job(
        &state,
        &app,
        &session,
        database.as_deref(),
        "tableExport",
        Arc::new(operation),
        target,
    )
    .await?;

    Ok(ExportTablesResult::Saved(rows_total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ColumnInfo;
    use datazen_feature_core::error::FeatureError as CommandError;
    use std::{collections::HashSet, path::Path, sync::Mutex};

    #[tokio::test]
    async fn desktop_export_reads_independent_runtime_target() {
        let test =
            crate::testing::app_state::TestAppState::with_tables_and_resource_provider().await;
        let session = test.save_and_open_runtime_session("export-runtime").await;
        let source = DesktopExportSource {
            runtime: crate::services::runtime_command_source::RuntimeCommandSource::new(
                test.state.driver_registry.clone(),
                test.state.store.clone(),
                test.state.platform.clone(),
            ),
            registry: test.state.driver_registry.clone(),
            cancel: datazen_runtime::job::CancelToken::new(),
        };
        let rows = Arc::new(Mutex::new(0usize));
        let observed = rows.clone();
        source
            .stream_query(
                &session,
                "SELECT * FROM users",
                datazen_driver_api::SqlTarget::new(Some("app"), None),
                Arc::new(move |event| {
                    if let datazen_driver_api::QueryStreamEvent::Rows { rows, .. } = event {
                        *observed.lock().unwrap() += rows.len();
                    }
                }),
            )
            .await
            .unwrap();
        assert_eq!(*rows.lock().unwrap(), 1);
        assert!(
            test.state
                .platform_require()
                .expect("platform is ready")
                .runtime()
                .expect("runtime is ready")
                .connection_id_for_session(&datazen_runtime::connection::DbSessionId::new(&session))
                .is_some(),
            "export must leave the runtime session open"
        );
    }

    fn test_driver() -> Arc<dyn DatabaseDriver> {
        crate::testing::mock_driver::MockDriver::new(
            "sqlite",
            crate::testing::mock_driver::MockDriverOptions::default(),
        )
    }

    fn v_str(s: &str) -> Option<Value> {
        Some(Value::String(s.to_string()))
    }

    fn rows2() -> Vec<Vec<Option<Value>>> {
        vec![
            vec![Some(Value::Integer(1)), v_str("a,b")],
            vec![Some(Value::Integer(2)), Some(Value::Null)],
        ]
    }

    #[test]
    fn csv_escapes_commas_quotes_newlines() {
        let mut f = StreamFormatter::new(DataFormat::Csv, "t".into(), None, test_driver());
        let cols = vec!["id".to_string(), "name".to_string()];
        let header = f.header(&cols);
        assert_eq!(header, "id,name\n");
        let text = f.rows(&rows2(), &cols).unwrap();
        assert_eq!(text, "1,\"a,b\"\n2,\n");
        assert_eq!(f.tail(&cols).unwrap(), "");
    }

    #[test]
    fn csv_escapes_bare_carriage_returns() {
        let mut f = StreamFormatter::new(DataFormat::Csv, "t".into(), None, test_driver());
        let cols = vec!["text".to_string()];
        let _ = f.header(&cols);
        assert_eq!(
            f.rows(&[vec![v_str("before\rafter")]], &cols).unwrap(),
            "\"before\rafter\"\n"
        );
    }

    #[test]
    fn json_streams_objects_with_comma() {
        let mut f = StreamFormatter::new(DataFormat::Json, "t".into(), None, test_driver());
        let cols = vec!["id".to_string(), "name".to_string()];
        assert_eq!(f.header(&cols), "[");
        let rows: Vec<Vec<Option<Value>>> = vec![
            vec![Some(Value::Integer(1)), v_str("x")],
            vec![Some(Value::Integer(2)), Some(Value::Null)],
        ];
        // First call writes object 1 with a leading newline, no comma yet.
        let a = f.rows(&rows[..1], &cols).unwrap();
        assert_eq!(a, "\n{\"id\":1,\"name\":\"x\"}");
        // Second call adds a comma.
        let b = f.rows(&rows[1..], &cols).unwrap();
        assert_eq!(b, ",\n{\"id\":2,\"name\":null}");
        assert_eq!(f.tail(&cols).unwrap(), "\n]");
    }

    #[test]
    fn csv_bytes_use_lossless_marker_and_escape_reserved_text() {
        let mut f = StreamFormatter::new(DataFormat::Csv, "t".into(), None, test_driver());
        let cols = vec!["payload".to_string(), "text".to_string()];
        assert_eq!(f.header(&cols), "payload,text\n");
        let rows = vec![vec![
            Some(Value::Bytes(vec![0x00, 0xff, 0xfe])),
            v_str("datazen:bytes:hex:00fffe"),
        ]];
        assert_eq!(
            f.rows(&rows, &cols).unwrap(),
            "datazen:bytes:hex:00fffe,datazen:text:datazen:bytes:hex:00fffe\n"
        );
    }

    #[test]
    fn json_bytes_are_structured_and_valid_json() {
        let mut f = StreamFormatter::new(DataFormat::Json, "t".into(), None, test_driver());
        let cols = vec!["payload".to_string()];
        let mut text = f.header(&cols);
        text.push_str(
            &f.rows(&[vec![Some(Value::Bytes(vec![0x00, 0xff, 0xfe]))]], &cols)
                .unwrap(),
        );
        text.push_str(&f.tail(&cols).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON export");
        assert_eq!(
            parsed[0]["payload"],
            serde_json::json!({
                "$datazenType": "bytes",
                "encoding": "hex",
                "value": "00fffe",
            })
        );
    }

    #[test]
    fn json_marker_collision_is_enveloped_without_changing_normal_json() {
        let colliding = json_value(Some(Value::Json(serde_json::json!({
            "$datazenType": "bytes",
            "encoding": "hex",
            "value": "00fffe",
        }))));
        assert_eq!(
            colliding,
            serde_json::json!({
                "$datazenType": "json",
                "value": {
                    "$datazenType": "bytes",
                    "encoding": "hex",
                    "value": "00fffe",
                },
            })
        );
        let normal = json_value(Some(Value::Json(serde_json::json!({
            "kind": "event",
            "value": 42,
        }))));
        assert_eq!(normal, serde_json::json!({ "kind": "event", "value": 42 }));
    }

    #[test]
    fn sql_insert_single_transaction_never_per_insert() {
        let mut f = StreamFormatter::new(
            DataFormat::SqlInsert,
            "users".into(),
            Some("sqlite".into()),
            test_driver(),
        );
        let cols = vec!["id".to_string(), "email".to_string()];
        let header = f.header(&cols);
        assert_eq!(header, "BEGIN;\n");
        // Two rows -> single batched INSERT with two tuples.
        let text = f.rows(&rows2(), &cols).unwrap();
        assert_eq!(
            text,
            "INSERT INTO \"users\" (\"id\", \"email\") VALUES\n  (1, CAST(X'612c62' AS TEXT)),\n  (2, NULL);\n"
        );
        let tail = f.tail(&cols).unwrap();
        assert_eq!(tail, "COMMIT;\n");
    }

    #[test]
    fn sql_insert_batches_at_limit() {
        let mut f = StreamFormatter::new(DataFormat::SqlInsert, "t".into(), None, test_driver());
        let cols = vec!["id".to_string()];
        let rows: Vec<Vec<Option<Value>>> =
            (0..600).map(|i| vec![Some(Value::Integer(i))]).collect();
        let text = f.rows(&rows, &cols).unwrap();
        // 600 rows -> two batched INSERT statements (500 + 100 rows), all in
        // this rows call since flush drains every pending row.
        assert_eq!(text.matches("INSERT INTO").count(), 2);
        // The last row (599) is in the second batch.
        assert!(text.contains("(599);"));
        let tail = f.tail(&cols).unwrap();
        // No pending rows left; tail only adds COMMIT (single transaction).
        assert_eq!(tail, "COMMIT;\n");
        assert_eq!(tail.matches("INSERT INTO").count(), 0);
    }

    #[test]
    fn sql_insert_uses_driver_owned_binary_literal() {
        let mut f = StreamFormatter::new(DataFormat::SqlInsert, "t".into(), None, test_driver());
        let cols = vec!["payload".to_string()];
        let _ = f.header(&cols);
        let text = f
            .rows(&[vec![Some(Value::Bytes(vec![0x00, 0xff, 0xfe]))]], &cols)
            .unwrap();
        assert!(text.contains("X'00fffe'"), "{text}");
        assert!(!text.contains('\u{fffd}'), "{text}");
    }

    #[test]
    fn build_select_sql_quotes_mysql_backtick() {
        assert_eq!(
            build_select_sql("users", &["id".into(), "name".into()], Some("mysql")),
            "SELECT `id`, `name` FROM `users`"
        );
        assert_eq!(
            build_select_sql("users", &["id".into()], Some("postgres")),
            "SELECT \"id\" FROM \"users\""
        );
        assert_eq!(
            build_select_sql("users", &[], None),
            "SELECT * FROM \"users\""
        );
    }

    #[test]
    fn build_file_plans_matches_modes() {
        let mk = |name: &str, ddl: Option<String>| TableExportInput {
            table_name: name.into(),
            schema: None,
            columns: vec!["id".into()],
            ddl,
        };
        // data_and_structure + csv -> structure.sql + data.csv
        let req = ExportTablesRequest {
            db_session_id: "c".into(),
            database_type: Some("postgres".into()),
            database: None,
            schema: None,
            mode: ExportMode::DataAndStructure,
            data_format: DataFormat::Csv,
            output_mode: OutputMode::Zip,
            tables: vec![mk("users", Some("CREATE TABLE ...;".into()))],
        };
        let plans = build_file_plans(&req);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].filename(), "users.sql");
        assert_eq!(plans[1].filename(), "users.csv");

        // data_and_structure + sql_insert -> one combined .sql
        let req2 = ExportTablesRequest {
            data_format: DataFormat::SqlInsert,
            ..req
        };
        let plans2 = build_file_plans(&req2);
        assert_eq!(plans2.len(), 1);
        assert_eq!(plans2[0].filename(), "users.sql");
    }

    #[test]
    fn resolve_ddl_fallback() {
        let t = TableExportInput {
            table_name: "t".into(),
            schema: None,
            columns: vec![],
            ddl: None,
        };
        assert!(resolve_ddl(&t).contains("DDL unavailable"));
        let t2 = TableExportInput {
            table_name: "t".into(),
            schema: None,
            columns: vec![],
            ddl: Some("CREATE TABLE;".into()),
        };
        assert_eq!(resolve_ddl(&t2), "CREATE TABLE;");
    }

    #[test]
    fn column_info_used() {
        let ci = ColumnInfo {
            name: "a".into(),
            data_type: "int".into(),
            nullable: true,
        };
        assert_eq!(ci.name, "a");
    }

    fn poison_mutex<T>(mutex: &Mutex<T>) {
        let target = mutex as *const Mutex<T> as usize;
        let handle = std::thread::spawn(move || {
            let m = unsafe { &*(target as *const Mutex<T>) };
            let _guard = m.lock().unwrap();
            panic!("tester: intentional mutex poison");
        });
        assert!(handle.join().is_err());
    }

    /// [tester] Non-callback export locks propagate poison as `CommandError::Internal`.
    #[test]
    fn test_tester_lock_export_poison_returns_internal() {
        let mutex = Mutex::new(0_i32);
        poison_mutex(&mutex);
        let err = lock_export(&mutex).unwrap_err();
        match err {
            CommandError::Internal(msg) => {
                assert!(
                    msg.contains("export lock poisoned"),
                    "unexpected internal message: {msg}"
                );
            }
            other => panic!("expected CommandError::Internal, got {other:?}"),
        }
    }

    /// [tester] Stream callback locks recover from poison via `into_inner`.
    #[test]
    fn test_tester_lock_export_stream_recovers_from_poison() {
        let mutex = Mutex::new(vec!["col".to_string()]);
        poison_mutex(&mutex);
        let cols = lock_export_stream(&mutex);
        assert_eq!(*cols, vec!["col".to_string()]);
    }

    /// [tester] Concurrent `lock_export` calls serialize without error on a healthy mutex.
    #[test]
    fn test_tester_lock_export_concurrent_access_succeeds() {
        use std::sync::Arc;
        use std::thread;

        let mutex = Arc::new(Mutex::new(0_i32));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let m = Arc::clone(&mutex);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let mut guard = lock_export(&m).expect("lock should succeed");
                        *guard += 1;
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(*lock_export(&mutex).unwrap(), 800);
    }
}
