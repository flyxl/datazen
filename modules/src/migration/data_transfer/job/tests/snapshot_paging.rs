use super::*;

#[tokio::test]
async fn text_keys_transfer_in_bounded_snapshot_pages_but_cannot_resume() {
    let mut schema = schema_with_snapshot(&["id", "name"]);
    schema.columns[0].data_type = "TEXT".into();
    let rows = (0..5)
        .map(|id| {
            vec![
                Some(Value::String(id.to_string())),
                Some(Value::String(format!("row-{id}"))),
            ]
        })
        .collect();
    let src = FakeDb {
        rows,
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    };
    let tgt = FakeDb {
        rows: vec![],
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    };
    let job = job_for_pipeline(2);
    let inspected = inspected_for(&["id", "name"]);
    let scope = crate::migration::data_transfer::recordset::SourceScope {
        where_sql: None,
        recordset_sql: None,
        params: vec![],
        count_params: vec![],
    };
    let mut checkpoint = InMemoryTransferCheckpoint::new();
    let mappings = columns(&["id", "name"]);
    let refs = mappings.iter().collect::<Vec<_>>();
    // 固定时钟：提交边界的时刻在断言里被钉死，而不是「某个合理的时间」。
    let fixed_clock = FixedClock::at("2026-06-07T08:09:10.000Z");
    let mut context = BoundedPipelineContext {
        job: &job,
        table: &inspected,
        source_schema: &schema,
        target_schema: &schema,
        source_driver: &src,
        source_handle: &src_handle(),
        target_driver: &tgt,
        target_handle: &tgt_handle(),
        source_scope: &scope,
        source_table_ref: "t",
        target_table_ref: "t",
        source_quote: '"',
        target_type: "postgresql",
        columns: &refs,
        formatter: &ValueFormatter::SameFamily,
        cancelled: None,
        write_started: None,
        checkpoint: &mut checkpoint,
        clock: &fixed_clock,
    };
    let result = execute_bounded_table(&mut context)
        .await
        .expect("snapshot transfer");
    assert!(
        result.result.result.success,
        "{:?}",
        result.result.result.error
    );
    assert_eq!(result.progress.committed.get(), 5);
    assert_eq!(result.boundaries.len(), 3);
    // 每个提交边界都读注入的时钟：换成固定时钟后时刻完全可断言。
    assert!(
        result
            .boundaries
            .iter()
            .all(|boundary| boundary.committed_at.as_str() == "2026-06-07T08:09:10.000Z"),
        "{:?}",
        result.boundaries
    );
    assert!(result.max_buffer_bytes <= PIPELINE_INITIAL_BYTES);
    let writes = tgt.state.lock().unwrap().committed.clone();
    assert_eq!(writes.len(), 3);
    assert_eq!(
        writes
            .iter()
            .flat_map(|batch| batch.chunks_exact(2))
            .map(|row| match &row[0] {
                Value::String(id) => id.clone(),
                other => panic!("unexpected key {other:?}"),
            })
            .collect::<Vec<_>>(),
        (0..5).map(|id| id.to_string()).collect::<Vec<_>>()
    );
    assert!(src
        .state
        .lock()
        .unwrap()
        .source_queries
        .iter()
        .all(|(sql, params)| sql.contains("OFFSET ") && params.is_empty()));
    assert!(
        execute_bounded_table(&mut context).await.is_err(),
        "offsets must never restart outside their original snapshot"
    );
    assert_eq!(
        format!("{:?}", tgt.state.lock().unwrap().committed),
        format!("{writes:?}")
    );
}

#[test]
fn snapshot_paging_does_not_relax_mysql_resume_key_admission() {
    let mut schema = schema_with_snapshot(&["id", "name"]);
    for native in ["BIGINT", "BIGINT UNSIGNED", "DECIMAL(65,30)"] {
        schema.columns[0].data_type = native.into();
        assert!(
            crate::migration::data_transfer::resume::resumable_primary_key(&schema, None, "mysql")
                .is_err()
        );
        assert_eq!(
            crate::migration::data_transfer::resume::fingerprint::primary_key_for_snapshot(
                &schema, None, "mysql"
            )
            .unwrap(),
            vec!["id"]
        );
    }
}

#[test]
fn snapshot_offset_page_preserves_recordset_predicates_and_bound_parameters() {
    let schema = schema_with_snapshot(&["id", "name"]);
    let driver = FakeDb {
        rows: vec![],
        schema,
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    };
    for predicate in ["WHERE id > ?", "id > ?"] {
        let scope = crate::migration::data_transfer::recordset::SourceScope {
            where_sql: Some(predicate.into()),
            recordset_sql: None,
            params: vec![],
            count_params: vec![Value::String("9007199254740993".into())],
        };
        let (sql, params) =
            crate::migration::data_transfer::resume::fingerprint::build_snapshot_offset_page(
                &driver,
                "SELECT id, name FROM t",
                &scope,
                &["id".into()],
                4,
                2,
                '"',
            )
            .unwrap();
        assert_eq!(
            sql,
            "SELECT id, name FROM t WHERE (id > ?) ORDER BY \"id\" LIMIT 2 OFFSET 4"
        );
        assert!(matches!(&params[..], [Value::String(value)] if value == "9007199254740993"));
    }
}
