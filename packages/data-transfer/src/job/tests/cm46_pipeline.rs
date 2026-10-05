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
    // 字节账：读页前按行数预留额度，读回后按实测结算、立刻退还预留，再记转换副本与
    // 待发送参数——峰值是「预留」而不是 page + converted + params 三份叠加。
    let budget = PipelineBudget::new();
    assert_eq!(budget.capacity(), PIPELINE_INITIAL_BYTES, "§6.2 bound");
    assert!(
        outcome.max_buffer_bytes > 0,
        "the pipeline must account for the bytes it holds"
    );
    assert!(
        outcome.max_buffer_bytes <= PIPELINE_INITIAL_BYTES,
        "peak {} exceeds the {} byte bound",
        outcome.max_buffer_bytes,
        PIPELINE_INITIAL_BYTES
    );
}

/// 回归：宽行（每行 3 MiB、批大小 2）在有界缓冲内迁移，峰值不得越过容量。
///
/// 修复前这组数据让账面峰值涨到 18 MiB 仍报成功——预留发生在读页之后、且 `account`
/// 没有拒绝分支，于是三份副本无界叠加。现在预留先于读页、结算走 `try_account`，
/// 窗口按剩余额度自动收窄成一行一页。
#[tokio::test]
async fn cm46_wide_rows_stay_inside_the_pipeline_bound() {
    let wide = "w".repeat(3 * 1024 * 1024);
    let rows: Rows = (1..=4)
        .map(|id| vec![Some(Value::Integer(id)), Some(Value::String(wide.clone()))])
        .collect();
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
    assert_eq!(
        outcome.boundaries.len(),
        4,
        "3 MiB rows leave room for one row per page, so four batches"
    );
    assert!(
        outcome.max_buffer_bytes <= PIPELINE_INITIAL_BYTES,
        "peak {} exceeds the {} byte bound",
        outcome.max_buffer_bytes,
        PIPELINE_INITIAL_BYTES
    );
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

/// 回归（§6.2）：`Value::Json` 必须按编码后长度记账。
///
/// 修复前 Json / Timestamp / Bool / Float 一律按 16 字节计费，于是 9 MiB 以上的
/// JSON 单值既骗过「单值超限」检查、又让字节账显示余量充足，管道照常报成功。
#[tokio::test]
async fn cm46_oversize_json_payload_is_not_charged_sixteen_bytes() {
    let payload = serde_json::Value::String("j".repeat(9 * 1024 * 1024));
    assert!(
        super::super::pipeline::value_bytes(Some(&Value::Json(payload.clone())))
            > PIPELINE_INITIAL_BYTES,
        "the encoded JSON must be measured, not guessed at 16 bytes"
    );
    let rows: Rows = vec![vec![
        Some(Value::Integer(1)),
        Some(Value::Json(payload)),
    ]];
    let schema = schema_with_snapshot(&["id", "payload"]);
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
    let inspected = inspected_for(&["id", "payload"]);
    let scope = crate::recordset::SourceScope {
        where_sql: None,
        recordset_sql: None,
        params: vec![],
        count_params: vec![],
    };
    let mut checkpoint = InMemoryTransferCheckpoint::new();
    let mappings = columns(&["id", "payload"]);
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
    assert!(!outcome.result.result.success, "a 9 MiB payload must not pass");
    assert!(
        err.contains("pipeline buffer"),
        "unexpected error: {err}"
    );
    assert!(
        tgt.state.lock().unwrap().write_sqls.is_empty(),
        "nothing may be written once the payload is refused"
    );
}

/// 回归：取消标记必须真的被管道读到——修复前 handler 传的是 `cancelled: None`，
/// 管道里三处取消检查全是死代码，作业只能一路跑到底。
#[tokio::test]
async fn cm46_cancel_flag_stops_the_pipeline_before_the_first_write() {
    let rows: Rows = (1..=4)
        .map(|id| {
            vec![
                Some(Value::Integer(id)),
                Some(Value::String(format!("v{id}"))),
            ]
        })
        .collect();
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
    let flag = Arc::new(AtomicBool::new(true));
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
        cancelled: Some(flag),
        write_started: None,
        checkpoint: &mut checkpoint,
    };
    let error = match execute_bounded_table(&mut context).await {
        Ok(_) => panic!("a cancelled run never succeeds"),
        Err(error) => error,
    };
    assert!(
        matches!(error, TransferError::Cancelled(_)),
        "unexpected error: {error}"
    );
    assert!(
        tgt.state.lock().unwrap().write_sqls.is_empty(),
        "the cancel flag must be read before the first write"
    );
    assert!(
        tgt.state.lock().unwrap().committed.is_empty(),
        "nothing may be committed once the cancel is seen"
    );
}

/// 中途取消：写入中取消必须回滚在途批次、不产出边界，且此后再无写入。
#[tokio::test]
async fn cm46_cancel_during_transfer_rolls_the_inflight_batch_back() {
    let rows: Rows = (1..=6)
        .map(|id| {
            vec![
                Some(Value::Integer(id)),
                Some(Value::String(format!("v{id}"))),
            ]
        })
        .collect();
    let schema = schema_with_snapshot(&["id", "name"]);
    let src = Arc::new(FakeDb {
        rows,
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
    let job = job_for_pipeline(2);
    let inspected = inspected_for(&["id", "name"]);
    let flag = Arc::new(AtomicBool::new(false));
    let src2 = src.clone();
    let tgt2 = tgt.clone();
    let schema2 = schema.clone();
    let scope = crate::recordset::SourceScope {
        where_sql: None,
        recordset_sql: None,
        params: vec![],
        count_params: vec![],
    };
    let cancel_flag = flag.clone();
    let handle = tokio::spawn(async move {
        let mut checkpoint = InMemoryTransferCheckpoint::new();
        let mappings = columns(&["id", "name"]);
        let col_refs: Vec<&ColumnMapping> = mappings.iter().collect();
        let mut context = BoundedPipelineContext {
            job: &job,
            table: &inspected,
            source_schema: &schema2,
            target_schema: &schema2,
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
            cancelled: Some(cancel_flag),
            write_started: None,
            checkpoint: &mut checkpoint,
        };
        execute_bounded_table(&mut context).await
    });
    let entered = tokio::time::timeout(std::time::Duration::from_secs(10), gate.entered.notified());
    assert!(entered.await.is_ok(), "pipeline never reached the gated write");
    flag.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.opened.notify_one();
    let outcome = handle.await.expect("join").expect("pipeline");
    assert!(
        outcome.result.cancelled,
        "the cancel flag must be honoured mid-flight"
    );
    assert!(
        !outcome.result.result.success,
        "a cancelled run never succeeds"
    );
    assert!(
        outcome.boundaries.is_empty(),
        "the in-flight batch was rolled back, so nothing is confirmed"
    );
    let target = tgt.state.lock().unwrap();
    assert_eq!(
        target.write_sqls.len(),
        1,
        "no batch may be written after the cancel"
    );
    assert!(
        target.committed.is_empty(),
        "a rolled-back batch must not stay committed"
    );
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

/// 回归（§7）：字节账越界必须显式失败，但已确认的那部分要保留——不得抹成
/// `RolledBack` + 空边界。修复前 handler 把越界交给 `failed_stage`，提交证据被丢弃。
#[test]
fn cm46_budget_overrun_keeps_the_confirmed_boundaries() {
    use crate::job::outcome::unbounded_stage;
    use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
    use datazen_runtime::job::{StageSpec, StageTerminal};

    let spec = StageSpec {
        stage_id: StageId::new("data"),
        kind: "data".into(),
        depends_on: vec![],
    };
    let confirmed = vec![boundary(2)];
    let outcome = unbounded_stage(&spec, &confirmed, PIPELINE_INITIAL_BYTES + 1);
    assert_eq!(outcome.stage_id, spec.stage_id);
    assert!(matches!(outcome.terminal, StageTerminal::Failed));
    assert_eq!(
        outcome.commit_boundaries, confirmed,
        "the confirmed portion survives the overrun"
    );
    assert_eq!(
        outcome.effect_outcome,
        EffectOutcome::PartiallyApplied,
        "an overrun is not a rollback of what was already committed"
    );
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::SqlError));
}

/// 取消是既成事实：确认过的边界必须留在 Cancelled 阶段里（§7），
/// 修复前取消被降级成 failed_stage，边界被清空成 RolledBack。
#[test]
fn cm46_cancelled_stage_keeps_the_confirmed_boundaries() {
    use crate::job::outcome::cancelled_stage;
    use datazen_platform_api::dto::execution::EffectOutcome;
    use datazen_runtime::job::{StageSpec, StageTerminal};

    let spec = StageSpec {
        stage_id: StageId::new("data"),
        kind: "data".into(),
        depends_on: vec![],
    };
    let confirmed = vec![boundary(2)];
    let outcome = cancelled_stage(&spec, &confirmed, "cancelled by the operator");
    assert!(matches!(outcome.terminal, StageTerminal::Cancelled));
    assert_eq!(
        outcome.commit_boundaries, confirmed,
        "a cancel never erases what was already confirmed"
    );
    assert_eq!(outcome.effect_outcome, EffectOutcome::PartiallyApplied);
    assert_eq!(
        outcome.error_code, None,
        "a cancel is not an execution error"
    );

    let before_any_commit = cancelled_stage(&spec, &[], "cancelled before the first write");
    assert_eq!(
        before_any_commit.effect_outcome,
        EffectOutcome::NotStarted,
        "nothing was touched yet"
    );
    assert!(before_any_commit.commit_boundaries.is_empty());
}
