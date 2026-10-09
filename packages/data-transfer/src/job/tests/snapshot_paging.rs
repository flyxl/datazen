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
    let scope = crate::recordset::SourceScope {
        where_sql: None,
        recordset_sql: None,
        params: vec![],
        count_params: vec![],
    };
    let mut checkpoint = InMemoryTransferCheckpoint::new();
    let mappings = columns(&["id", "name"]);
    let refs = mappings.iter().collect::<Vec<_>>();
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
        assert!(crate::resume::resumable_primary_key(&schema, None, "mysql").is_err());
        assert_eq!(
            crate::resume::fingerprint::primary_key_for_snapshot(&schema, None, "mysql").unwrap(),
            vec!["id"]
        );
    }
}
