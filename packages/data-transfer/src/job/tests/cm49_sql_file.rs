//! CM-49：SQL 文件目标无目标连接，产物规格可复现。

use super::*;

// ------------------------------------------------------------------ CM-49

#[tokio::test]
async fn cm49_sql_file_transfer_uses_no_target_connection() {
    let rows: Rows = vec![
        vec![Some(Value::Integer(1)), Some(Value::String("a".into()))],
        vec![Some(Value::Integer(2)), Some(Value::String("b".into()))],
    ];
    let schema = schema_without_objects(&["id", "name"]);
    let src = Arc::new(FakeDb {
        rows,
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
    let dest = std::env::temp_dir().join(format!("dt-cm49-{}.sql", uuid::Uuid::new_v4()));
    let mut job = job_for_pipeline(2);
    job.target = None;
    job.sql_file_target = Some(SqlFileTarget {
        file_token: "token".into(),
        database_type: None,
        database: None,
        schema: None,
        encoding: None,
        compression: None,
    });
    job.mode = TransferMode::StructureAndData;
    let inspected = inspected_for(&["id", "name"]);
    let freeze = TransferFreezeBody {
        job: job.clone(),
        mapping_fingerprint: "sha256:map".into(),
        recovery_policy: "forbidAutoResume".into(),
        has_structure_ir: true,
        stable_key_columns: vec![vec!["id".into()]],
        snapshot_proven: true,
    };
    let names: HashMap<String, TableSchema> =
        [("t".to_string(), schema.clone())].into_iter().collect();
    let handler = DataTransferHandler::apply(
        freeze,
        vec![inspected],
        names,
        HashMap::new(),
        TransferEndpoints::SqlFile {
            source_driver: src.clone(),
            source_handle: src_handle(),
            source_type: "fixture".into(),
            target_type: "fixture".into(),
            destination: dest.clone(),
            structure: None,
            source_adapter: None,
            target_adapter: None,
        },
        None,
    );
    let spec = datazen_runtime::job::StageSpec {
        stage_id: StageId::new("apply"),
        kind: "apply".into(),
        depends_on: vec![],
    };
    // apply 计划只含一个 apply 阶段。
    let stages = handler
        .validate_plan(&frozen_plan_for_apply())
        .expect("stages");
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].kind, "apply");
    let cancel = datazen_runtime::job::CancelToken::new();
    let outcome = handler.run_stage(&spec, &cancel).await.expect("stage");
    assert!(
        matches!(
            outcome.terminal,
            datazen_runtime::job::StageTerminal::Succeeded
        ),
        "sql file stage: {:?}",
        outcome.terminal
    );
    let written = std::fs::read_to_string(&dest).expect("sql file written");
    assert!(written.contains("CREATE TABLE"), "artifact spec: {written}");
    // 无目标连接：source FakeDb 的 write SQL 计数保持 0 —— execute_with_target 不产生目标写入。
    assert_eq!(src.state.lock().unwrap().execute_calls, 0);
    assert!(!outcome.artifact_ids.is_empty());
    assert!(!outcome.commit_boundaries.is_empty());
    // 同一份映射 + 同一份源元数据 → 同一份产物规格（内容寻址的 artifact id）。
    assert!(
        outcome
            .artifact_ids
            .iter()
            .any(|id| id.as_str() == format!("transfer-sql-{}", artifact_digest_of(&dest)).as_str()),
        "artifact id must be content addressed: {:?}",
        outcome.artifact_ids
    );
}

/// SQL 文件目标遇到结构对象（索引/外键）但没有注册 IR 适配器时必须明确失败，
/// 不能静默丢掉这些 DDL。
#[tokio::test]
async fn cm49_structure_objects_without_ir_adapter_fail_explicitly() {
    let rows: Rows = vec![vec![
        Some(Value::Integer(1)),
        Some(Value::String("a".into())),
    ]];
    let schema = schema_with_snapshot(&["id", "name"]);
    let src = Arc::new(FakeDb {
        rows,
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
    let dest = std::env::temp_dir().join(format!("dt-cm49-neg-{}.sql", uuid::Uuid::new_v4()));
    let mut job = job_for_pipeline(2);
    job.target = None;
    job.sql_file_target = Some(SqlFileTarget {
        file_token: "token".into(),
        database_type: None,
        database: None,
        schema: None,
        encoding: None,
        compression: None,
    });
    job.mode = TransferMode::StructureAndData;
    let names: HashMap<String, TableSchema> = [("t".to_string(), schema)].into_iter().collect();
    let handler = DataTransferHandler::apply(
        TransferFreezeBody {
            job,
            mapping_fingerprint: "sha256:map".into(),
            recovery_policy: "forbidAutoResume".into(),
            has_structure_ir: true,
            stable_key_columns: vec![vec!["id".into()]],
            snapshot_proven: true,
        },
        vec![inspected_for(&["id", "name"])],
        names,
        HashMap::new(),
        TransferEndpoints::SqlFile {
            source_driver: src.clone(),
            source_handle: src_handle(),
            source_type: "fixture".into(),
            target_type: "fixture".into(),
            destination: dest.clone(),
            structure: None,
            source_adapter: None,
            target_adapter: None,
        },
        None,
    );
    let spec = datazen_runtime::job::StageSpec {
        stage_id: StageId::new("apply"),
        kind: "apply".into(),
        depends_on: vec![],
    };
    let cancel = datazen_runtime::job::CancelToken::new();
    let outcome = handler.run_stage(&spec, &cancel).await.expect("stage");
    assert!(
        matches!(
            outcome.terminal,
            datazen_runtime::job::StageTerminal::Failed
        ),
        "missing IR adapter must fail the stage, got {:?}",
        outcome.terminal
    );
    assert!(outcome.commit_boundaries.is_empty());
    assert!(outcome.artifact_ids.is_empty());
}

fn artifact_digest_of(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).expect("artifact bytes");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}
