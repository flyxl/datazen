//! CM-46：有界管道字节记账、超限单值与慢目标背压。

use super::*;

// ------------------------------------------------------------------ CM-46

#[tokio::test]
async fn cm46_pipeline_byte_accounting_is_bounded_and_counted() {
    let rows: Rows = vec![
        vec![Some(Value::Integer(1)), Some(Value::String("aa".into()))],
        vec![Some(Value::Integer(2)), Some(Value::String("bbbb".into()))],
        vec![Some(Value::Integer(3)), Some(Value::String("c".into()))],
        vec![Some(Value::Integer(4)), Some(Value::String("dd".into()))],
    ];
    let schema = schema_with_snapshot(&["id", "name"]);
    let src = Arc::new(FakeDb {
        rows: rows.clone(),
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
    let tgt = Arc::new(FakeDb {
        rows: vec![],
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
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
    let col_refs: Vec<&ColumnMapping> = mappings.iter().collect();
    let mut context = BoundedPipelineContext {
        job: &job,
        table: &inspected,
        source_schema: &schema,
        target_schema: &schema,
        source_driver: src.as_ref(),
        source_handle: &src_handle(),
        target_driver: tgt.as_ref(),
        target_handle: &tgt_handle(),
        source_scope: &scope,
        source_table_ref: "t",
        target_table_ref: "t",
        source_quote: '"',
        target_type: "postgresql",
        columns: &col_refs,
        formatter: &ValueFormatter::SameFamily,
        cancelled: None,
        write_started: None,
        checkpoint: &mut checkpoint,
    };
    let outcome = execute_bounded_table(&mut context).await.expect("pipeline");
    assert!(
        outcome.result.result.success,
        "{:?}",
        outcome.result.result.error
    );
    assert_eq!(outcome.boundaries.len(), 2);
    // 字节账：page(1+2*2)+converted+params for the {2,aa/bbbb} page — 16 + 16 + 16 = 48 峰值。
    let page_bytes: usize = vec![
        vec![Some(Value::Integer(1)), Some(Value::String("aa".into()))],
        vec![Some(Value::Integer(2)), Some(Value::String("bbbb".into()))],
    ]
    .iter()
    .map(|row| row_bytes_of(row))
    .sum();
    // page_bytes: 16+16 (ids) + 2 + 4 = 38? id 的 Integer 计 16B， name 分别 2/4。
    assert_eq!(
        outcome.max_buffer_bytes,
        3 * page_bytes,
        "decoded+converted+params peak"
    );
    let _ = PipelineBudget::new();
}

#[tokio::test]
async fn cm46_oversize_single_value_fails_explicitly() {
    let big = "x".repeat(9 * 1024 * 1024);
    let rows: Rows = vec![vec![Some(Value::Integer(1)), Some(Value::String(big))]];
    let schema = schema_with_snapshot(&["id", "name"]);
    let src = Arc::new(FakeDb {
        rows,
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
    let tgt = Arc::new(FakeDb {
        rows: vec![],
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
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
    let col_refs: Vec<&ColumnMapping> = mappings.iter().collect();
    let mut context = BoundedPipelineContext {
        job: &job,
        table: &inspected,
        source_schema: &schema,
        target_schema: &schema,
        source_driver: src.as_ref(),
        source_handle: &src_handle(),
        target_driver: tgt.as_ref(),
        target_handle: &tgt_handle(),
        source_scope: &scope,
        source_table_ref: "t",
        target_table_ref: "t",
        source_quote: '"',
        target_type: "postgresql",
        columns: &col_refs,
        formatter: &ValueFormatter::SameFamily,
        cancelled: None,
        write_started: None,
        checkpoint: &mut checkpoint,
    };
    let outcome = execute_bounded_table(&mut context).await.expect("pipeline");
    let err = outcome.result.result.error.unwrap_or_default();
    assert!(
        err.contains("exceeding the 8 MiB pipeline buffer bound"),
        "unexpected error: {err}"
    );
    assert!(!outcome.result.result.success);
}

#[tokio::test]
async fn cm46_slow_writer_pauses_source_reader() {
    let rows: Rows = (1..=8)
        .map(|i| {
            vec![
                Some(Value::Integer(i)),
                Some(Value::String(format!("v{i}"))),
            ]
        })
        .collect();
    let schema = schema_with_snapshot(&["id", "name"]);
    let src = Arc::new(FakeDb {
        rows: rows.clone(),
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
    let gate = Arc::new(SlowGate {
        entered: tokio::sync::Notify::new(),
        opened: tokio::sync::Notify::new(),
        first: AtomicBool::new(true),
    });
    let tgt = Arc::new(FakeDb {
        rows: vec![],
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: Some(gate.clone()),
    });
    // spawn 中构造所有权上下文：job/schema/scope/checkpoint 都是局部变量。
    let src2 = src.clone();
    let tgt2 = tgt.clone();
    let handle = tokio::spawn(async move {
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
        let col_refs: Vec<&ColumnMapping> = mappings.iter().collect();
        let mut context = BoundedPipelineContext {
            job: &job,
            table: &inspected,
            source_schema: &schema,
            target_schema: &schema,
            source_driver: src2.as_ref(),
            source_handle: &src_handle(),
            target_driver: tgt2.as_ref(),
            target_handle: &tgt_handle(),
            source_scope: &scope,
            source_table_ref: "t",
            target_table_ref: "t",
            source_quote: '"',
            target_type: "postgresql",
            columns: &col_refs,
            formatter: &ValueFormatter::SameFamily,
            cancelled: None,
            write_started: None,
            checkpoint: &mut checkpoint,
        };
        execute_bounded_table(&mut context).await
    });
    // 若管道在到达被闸住的目标写入前就失败，握手会永远等下去；用超时把挂起转成明确失败。
    let entered =
        tokio::time::timeout(std::time::Duration::from_secs(10), gate.entered.notified()).await;
    let queries_at_block = src.state.lock().unwrap().source_queries.len();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let queries_later = src.state.lock().unwrap().source_queries.len();
    // 先放闸再断言：断言失败时管道任务仍能收敛，测试不会挂起。
    gate.opened.notify_one();
    let outcome = handle.await.expect("join").expect("pipeline");
    assert!(
        entered.is_ok(),
        "pipeline never reached the gated target write"
    );
    assert_eq!(
        queries_at_block, queries_later,
        "slow target must pause source paging; got {queries_at_block} -> {queries_later}"
    );
    assert!(outcome.result.result.success);
}
