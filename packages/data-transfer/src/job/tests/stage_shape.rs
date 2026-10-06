//! 阶段投影形状：钉死 `validate_plan` 把一个 Job 展开成哪几个阶段。
//!
//! 为什么这张表值得单独立一个文件：`docs/architecture/platform/data-migration-jobs.md` 与
//! `packages/runtime/src/job/runtime.rs` 的注释都把「data-transfer 的哪些 Job 是单阶段」
//! 当作事实来引用——阶段内轮询读失败后，只有「下一个阶段边界」能把漏掉的取消意图捡回来。
//! 这条事实一旦被 `handler.rs` 的重构悄悄改掉，两处注释会一起变成假话，而没有任何用例会
//! 变红。所以整张映射表在这里钉死，注释与测试同源。
//!
//! `handler.rs` 已逼近 800 行硬上限，本文件是它的配套用例，不往它身上加行。

use super::*;

fn frozen_plan_for_prepare() -> datazen_runtime::job::FrozenPlan {
    datazen_runtime::job::FrozenPlan {
        kind: "dataTransferPrepare".into(),
        is_apply: false,
        consumed_plan_id: None,
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: Some(1),
    }
}

/// 把投影压成 (kind, stage_id, depends_on)：阶段**顺序**本身就是边界顺序，必须一起比对，
/// 只断言个数会让「三个阶段换了个顺序」这类改动悄悄通过。
fn projection(
    handler: &DataTransferHandler,
    plan: &datazen_runtime::job::FrozenPlan,
) -> Vec<(String, String, Vec<String>)> {
    handler
        .validate_plan(plan)
        .expect("阶段投影应当成功")
        .into_iter()
        .map(|spec| {
            (
                spec.kind.clone(),
                spec.stage_id.as_str().to_string(),
                spec.depends_on
                    .iter()
                    .map(|dep| dep.as_str().to_string())
                    .collect(),
            )
        })
        .collect()
}

fn empty_driver() -> Arc<FakeDb> {
    Arc::new(FakeDb {
        rows: vec![],
        schema: schema_without_objects(&["id", "name"]),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    })
}

fn database_endpoints() -> TransferEndpoints {
    let driver = empty_driver();
    TransferEndpoints::Database {
        source_driver: driver.clone(),
        source_handle: src_handle(),
        target_driver: driver,
        target_handle: tgt_handle(),
        source_type: "fixture".into(),
        target_type: "fixture".into(),
    }
}

fn freeze_for(mode: TransferMode) -> TransferFreezeBody {
    let mut job = job_for_pipeline(2);
    job.mode = mode;
    TransferFreezeBody {
        job,
        mapping_fingerprint: "sha256:map".into(),
        recovery_policy: "forbidAutoResume".into(),
        has_structure_ir: true,
        stable_key_columns: vec![vec!["id".into()]],
        snapshot_proven: true,
    }
}

fn apply_handler_for(mode: TransferMode) -> DataTransferHandler {
    let schema = schema_without_objects(&["id", "name"]);
    let names: HashMap<String, TableSchema> = [("t".to_string(), schema)].into_iter().collect();
    DataTransferHandler::apply(
        freeze_for(mode),
        vec![inspected_for(&["id", "name"])],
        names,
        HashMap::new(),
        database_endpoints(),
        None,
    )
}

/// 头号事实：`structureAndData` 模式的 apply 是**三个**阶段，所以它**有**下一个边界。
///
/// 曾经有一版注释把 data-transfer 的 `apply` 一律当成单阶段，于是「结构+数据一起迁」这条
/// 最常见的全量路径被写成了读失败后无处收敛取消意图。改注释不够——钉在这里，改坏就红。
#[test]
fn structure_and_data_apply_expands_into_three_stages() {
    let stages = projection(
        &apply_handler_for(TransferMode::StructureAndData),
        &frozen_plan_for_apply(),
    );
    assert_eq!(
        stages,
        vec![
            ("structure".to_string(), "structure".to_string(), vec![]),
            (
                "data".to_string(),
                "data".to_string(),
                vec!["structure".to_string()]
            ),
            (
                "foreignKeys".to_string(),
                "foreignKeys".to_string(),
                vec!["data".to_string()]
            ),
        ],
        "structureAndData 的 apply 必须展开成 structure → data → foreignKeys 三个阶段"
    );
}

/// 单阶段的**完整**清单：`prepare`、SQL 文件目标的 `apply`、`structure` / `data` 模式的 `apply`。
///
/// 注释里凡是点名单阶段 Job 的地方，指的都是这张表；多写一个或漏写一个，注释就又开始撒谎。
#[test]
fn the_only_single_stage_projections_are_prepare_sqlfile_apply_and_single_mode_apply() {
    let plan = frozen_plan_for_apply();

    let structure_only = projection(&apply_handler_for(TransferMode::Structure), &plan);
    let data_only = projection(&apply_handler_for(TransferMode::Data), &plan);
    let mut prepare_job = job_for_pipeline(2);
    prepare_job.target = None;
    prepare_job.sql_file_target = Some(SqlFileTarget {
        file_token: "token".into(),
        database_type: None,
        database: None,
        schema: None,
        encoding: None,
        compression: None,
    });
    let prepare = DataTransferHandler::prepare(
        TransferFreezeBody {
            job: prepare_job,
            mapping_fingerprint: "sha256:map".into(),
            recovery_policy: "forbidAutoResume".into(),
            has_structure_ir: true,
            stable_key_columns: vec![vec!["id".into()]],
            snapshot_proven: true,
        },
        vec![inspected_for(&["id", "name"])],
        HashMap::new(),
        database_endpoints(),
    );
    let sql_file_destination =
        std::env::temp_dir().join(format!("dt-stage-shape-{}.sql", uuid::Uuid::new_v4()));
    let driver = empty_driver();
    let mut sql_file_job = job_for_pipeline(2);
    sql_file_job.target = None;
    sql_file_job.mode = TransferMode::StructureAndData;
    sql_file_job.sql_file_target = Some(SqlFileTarget {
        file_token: "token".into(),
        database_type: None,
        database: None,
        schema: None,
        encoding: None,
        compression: None,
    });
    let sql_file = DataTransferHandler::apply(
        TransferFreezeBody {
            job: sql_file_job,
            mapping_fingerprint: "sha256:map".into(),
            recovery_policy: "forbidAutoResume".into(),
            has_structure_ir: true,
            stable_key_columns: vec![vec!["id".into()]],
            snapshot_proven: true,
        },
        vec![inspected_for(&["id", "name"])],
        HashMap::new(),
        HashMap::new(),
        TransferEndpoints::SqlFile {
            source_driver: driver.clone(),
            source_handle: src_handle(),
            source_type: "fixture".into(),
            target_type: "fixture".into(),
            destination: sql_file_destination,
            structure: None,
            source_adapter: None,
            target_adapter: None,
        },
        None,
    );

    let cases: [(&str, Vec<(String, String, Vec<String>)>, &str); 4] = [
        ("structure 模式 apply", structure_only, "structure"),
        ("data 模式 apply", data_only, "data"),
        (
            "prepare",
            projection(&prepare, &frozen_plan_for_prepare()),
            "prepare",
        ),
        ("SQL 文件目标 apply", projection(&sql_file, &plan), "apply"),
    ];
    for (name, stages, kind) in cases {
        assert_eq!(stages.len(), 1, "{name} 应当是单阶段，得到 {stages:?}");
        assert_eq!(stages[0].1, kind, "{name} 的阶段 id 应当是 {kind}");
    }
}
