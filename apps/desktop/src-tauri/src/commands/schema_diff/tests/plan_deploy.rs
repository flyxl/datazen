//! Plan preparation, deploy gating and profile persistence tests.
//!
//! B2 schema_diff 域刀：`prepare_schema_diff_plan_with_schemas_impl` /
//! `execute_schema_diff_deploy_impl` 转发体已删（编排迁入 `job.rs` 的
//! `AppStateBackend`），这些测试改走**真实命令入口**——`prepare_schema_diff_plan`
//! / `execute_schema_diff_deploy` 受理后轮询 `get_schema_diff_job_details` 取
//! 终态与计划/部署报告（与前端同一旅程，C4 适配器烟测）。计划组装与部署
//! 验证的**规则级**断言（含精确错误文本）在 modules：
//! `schema_diff/api_tests.rs` 与 `reviewed_tests.rs`。

use super::super::command_test_host::{host_command_app, HostedCommandApp};
use super::super::profile_commands::validate_schema_diff_profile_connections;
use super::*;
use datazen::jobs::domain::job::JobState;

fn test_profile() -> SchemaDiffProfile {
    let now = chrono::Utc::now();
    SchemaDiffProfile {
        version: SchemaDiffProfile::CURRENT_VERSION,
        id: "profile-1".into(),
        name: "profile".into(),
        source_connection_id: "source".into(),
        target_connection_id: "target".into(),
        source_database: "app".into(),
        target_database: "app".into(),
        source_schema: None,
        target_schema: None,
        target_only_tables: vec![],
        tables: vec!["users".into()],
        source_objects: vec![],
        target_objects: vec![],
        allow_destructive: false,
        include_indexes: true,
        require_rollback: false,
        type_overrides: vec![],
        created_at: now,
        updated_at: now,
    }
}

async fn pg_command_test_sessions(
    options: MockDriverOptions,
) -> (crate::testing::app_state::TestAppState, String, String) {
    let test = crate::testing::app_state::TestAppState::with_options(options).await;
    for (id, database) in [("source", "source_db"), ("target", "target_db")] {
        let mut config = crate::testing::app_state::sample_postgres_config(id);
        config.database = Some(database.into());
        config.schema = Some("public".into());
        test.store
            .save_connection(config)
            .await
            .expect("save fixture connection");
    }
    let source_session = test.open_runtime_session("source").await;
    let target_session = test.open_runtime_session("target").await;
    (test, source_session, target_session)
}

fn pg_mock_options(
    source_schemas: HashMap<String, datazen_driver_api::TableSchema>,
    target_schemas: HashMap<String, datazen_driver_api::TableSchema>,
) -> MockDriverOptions {
    MockDriverOptions {
        has_schema_level: true,
        default_schema: Some("public"),
        table_schemas_by_database: HashMap::from([
            ("source_db".into(), source_schemas),
            ("target_db".into(), target_schemas),
        ]),
        ..Default::default()
    }
}

/// 轮询到 Job 终态（与 d1 兼容路径测试同一节奏：受理回执先回，终态后到）。
async fn await_terminal_job(
    app: &HostedCommandApp,
    job_id: &str,
) -> job::desktop::SchemaDiffJobDetails {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let details = job::desktop::get_schema_diff_job_details(
                app.state(),
                job_id.to_string(),
            )
            .await
            .expect("accepted Schema Diff Job remains queryable");
            if details.details.job.state.is_terminal() {
                return details;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Schema Diff Job reaches a durable terminal state")
}

/// 经真实命令入口准备表级计划，并取回渲染好的计划正文。
async fn prepare_pg_table_plan(
    app: &HostedCommandApp,
    source_session: &str,
    target_session: &str,
    tables: &[&str],
) -> SchemaDiffPlan {
    let accepted = prepare_schema_diff_plan(
        app.state(),
        source_session.to_string(),
        target_session.to_string(),
        tables.iter().map(|table| (*table).into()).collect(),
        None,
        None,
        None,
        None,
        false,
        None,
        None,
    )
    .await
    .expect("prepare_schema_diff_plan command accepts the request");
    let details = await_terminal_job(app, &accepted.job_id).await;
    assert_eq!(
        details.details.job.state,
        JobState::Succeeded,
        "prepare must succeed: {:?}",
        details.details.job.error
    );
    details
        .prepared
        .expect("a succeeded prepare job exposes its plan envelope")
        .plan
}

/// 经真实命令入口准备 target-only DROP 计划（allow_destructive）。
async fn prepare_pg_target_only_drop_plan(
    app: &HostedCommandApp,
    source_session: &str,
    target_session: &str,
    tables: &[&str],
) -> SchemaDiffPlan {
    let accepted = prepare_schema_diff_plan(
        app.state(),
        source_session.to_string(),
        target_session.to_string(),
        Vec::new(),
        None,
        Some(tables.iter().map(|table| (*table).into()).collect()),
        None,
        None,
        true,
        None,
        None,
    )
    .await
    .expect("prepare_schema_diff_plan command accepts the drop request");
    let details = await_terminal_job(app, &accepted.job_id).await;
    assert_eq!(
        details.details.job.state,
        JobState::Succeeded,
        "drop prepare must succeed: {:?}",
        details.details.job.error
    );
    details
        .prepared
        .expect("a succeeded prepare job exposes its plan envelope")
        .plan
}

/// 经真实命令入口部署已评审计划（兼容路径：直接携带计划正文），取回终态详情。
async fn deploy_pg_reviewed_plan(
    app: &HostedCommandApp,
    target_session: &str,
    plan: SchemaDiffPlan,
    confirm_destructive: Option<String>,
) -> job::desktop::SchemaDiffJobDetails {
    let accepted = execute_schema_diff_deploy(
        app.state(),
        target_session.to_string(),
        plan,
        Some(false),
        Some(false),
        confirm_destructive,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("execute_schema_diff_deploy command accepts the plan body");
    assert_eq!(accepted.kind, "schemaDiffApply");
    await_terminal_job(app, &accepted.job_id).await
}

fn pg_target_drop_options() -> MockDriverOptions {
    let parent_qualified = pg_parent_schema("public.parent");
    let parent_bare = pg_parent_schema("parent");
    let child_qualified = pg_child_schema("public.child", true);
    let child_bare = pg_child_schema("child", true);
    MockDriverOptions {
        has_schema_level: true,
        default_schema: Some("public"),
        tables_by_database: HashMap::from([(
            "target_db".into(),
            vec![
                target_table("parent", Some("public")),
                target_table("child", Some("public")),
            ],
        )]),
        table_schemas_by_database: HashMap::from([(
            "target_db".into(),
            HashMap::from([
                ("parent".into(), parent_bare),
                ("public.parent".into(), parent_qualified),
                ("child".into(), child_bare),
                ("public.child".into(), child_qualified),
            ]),
        )]),
        ..Default::default()
    }
}

#[tokio::test]
async fn postgres_create_with_foreign_key_deploy_accepts_unchanged_reviewed_snapshots() {
    let options = pg_mock_options(
        HashMap::from([
            ("public.parent".into(), pg_parent_schema("public.parent")),
            ("public.child".into(), pg_child_schema("public.child", true)),
        ]),
        HashMap::new(),
    );
    let (test, source_session, target_session) = pg_command_test_sessions(options).await;
    let app = host_command_app(test);
    let plan = prepare_pg_table_plan(
        &app,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;
    assert!(plan.statements.iter().any(|statement| {
        statement.summary.contains("CREATE TABLE") && statement.summary.contains("public.parent")
    }));
    assert!(plan.statements.iter().any(|statement| {
        statement.summary.contains("FOREIGN KEY") || statement.sql.contains("FOREIGN KEY")
    }));

    let details = deploy_pg_reviewed_plan(&app, &target_session, plan, None).await;
    let result = details
        .deploy_result
        .expect("a terminal apply job carries a deploy report");
    assert_eq!(result.status, crate::schema_diff::DeployStatus::Committed);
    assert!(result.executed_count > 0);
    assert!(app.keep.mock.execute_calls() > 0);
}

#[tokio::test]
async fn postgres_add_fk_deploy_accepts_unchanged_reviewed_snapshots() {
    let source_child = pg_child_schema("public.child", true);
    let target_child = pg_child_schema("public.child", false);
    let options = pg_mock_options(
        HashMap::from([
            ("public.parent".into(), pg_parent_schema("public.parent")),
            ("public.child".into(), source_child),
        ]),
        HashMap::from([
            ("public.parent".into(), pg_parent_schema("public.parent")),
            ("parent".into(), pg_parent_schema("parent")),
            ("public.child".into(), target_child.clone()),
            ("child".into(), target_child),
        ]),
    );
    let (test, source_session, target_session) = pg_command_test_sessions(options).await;
    let app = host_command_app(test);
    let plan = prepare_pg_table_plan(
        &app,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;
    assert!(plan.statements.iter().any(|statement| {
        statement.summary.contains("FOREIGN KEY") || statement.sql.contains("FOREIGN KEY")
    }));

    let details = deploy_pg_reviewed_plan(&app, &target_session, plan, None).await;
    let result = details
        .deploy_result
        .expect("a terminal apply job carries a deploy report");
    assert_eq!(result.status, crate::schema_diff::DeployStatus::Committed);
    assert!(result.executed_count > 0);
}

#[tokio::test]
async fn postgres_target_mutation_after_review_is_rejected_before_any_write() {
    let target_child = pg_child_schema("public.child", false);
    let options = pg_mock_options(
        HashMap::from([
            ("public.parent".into(), pg_parent_schema("public.parent")),
            ("public.child".into(), pg_child_schema("public.child", true)),
        ]),
        HashMap::from([
            ("public.parent".into(), pg_parent_schema("public.parent")),
            ("parent".into(), pg_parent_schema("parent")),
            ("public.child".into(), target_child.clone()),
            ("child".into(), target_child),
        ]),
    );
    let (test, source_session, target_session) = pg_command_test_sessions(options).await;
    let app = host_command_app(test);
    let plan = prepare_pg_table_plan(
        &app,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;

    app.keep
        .mock
        .set_table_schema_for_test("target_db", "child", pg_child_schema("child", true));
    let details = deploy_pg_reviewed_plan(&app, &target_session, plan, None).await;
    // 「Target schema changed for public.child」的规则级文本钉在 modules
    // （api_tests 的 complete_target_dependency_catalog_rejects_*）；这里钉
    // 适配器旅程：复核失效 ⇒ Job 失败，且**任何写都没有发生**。
    assert_eq!(
        details.details.job.state,
        JobState::Failed,
        "a post-review FK change must invalidate the plan"
    );
    assert_eq!(app.keep.mock.execute_calls(), 0);
}

#[tokio::test]
async fn postgres_child_parent_drop_revalidates_qualified_catalog_before_deploy() {
    let (test, source_session, target_session) =
        pg_command_test_sessions(pg_target_drop_options()).await;
    let app = host_command_app(test);
    let plan = prepare_pg_target_only_drop_plan(
        &app,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.statements.len(), 2);
    let child_drop = plan
        .statements
        .iter()
        .position(|statement| statement.sql.contains("child"))
        .expect("render child drop");
    let parent_drop = plan
        .statements
        .iter()
        .position(|statement| statement.sql.contains("parent"))
        .expect("render parent drop");
    assert!(child_drop < parent_drop, "{:?}", plan.statements);

    let details = deploy_pg_reviewed_plan(
        &app,
        &target_session,
        plan,
        Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN.to_string()),
    )
    .await;
    let result = details
        .deploy_result
        .expect("a terminal apply job carries a deploy report");
    assert_eq!(
        result.status,
        crate::schema_diff::DeployStatus::Committed,
        "unchanged full target catalog should validate"
    );
    assert_eq!(result.executed_count, 2);
}

#[tokio::test]
async fn postgres_new_dependent_relation_after_review_blocks_drop_before_write() {
    let (test, source_session, target_session) =
        pg_command_test_sessions(pg_target_drop_options()).await;
    let app = host_command_app(test);
    let plan = prepare_pg_target_only_drop_plan(
        &app,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;
    app.keep
        .mock
        .add_table_for_test("target_db", target_table("late_child", Some("public")));

    let details = deploy_pg_reviewed_plan(
        &app,
        &target_session,
        plan,
        Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN.to_string()),
    )
    .await;
    // 「Target dependency catalog changed after review (new relations:
    // [public.late_child] …)」的规则级文本钉在 modules（api.rs 的
    // validate_target_dependency_catalog / api_tests）；这里钉适配器旅程：
    // 新依赖目录 ⇒ Job 失败，且任何写都没有发生。
    assert_eq!(
        details.details.job.state,
        JobState::Failed,
        "a new dependency catalog relation must invalidate the drop plan"
    );
    assert_eq!(app.keep.mock.execute_calls(), 0);
}

#[tokio::test]
async fn schema_diff_profile_save_requires_existing_connections() {
    let test = crate::testing::app_state::TestAppState::new().await;
    test.save_connection("source").await;
    let profile = test_profile();
    let error = validate_schema_diff_profile_connections(&test.state, &profile)
        .await
        .expect_err("missing target should be rejected");
    assert!(error.to_string().contains("target connection"));

    test.save_connection("target").await;
    assert!(
        validate_schema_diff_profile_connections(&test.state, &profile)
            .await
            .is_ok()
    );
}

/// D1：兼容路径（未传 planId、直接携带计划正文）经**真实命令入口**走通
/// JobRuntime——命令内部先 `register_plan_for_apply` 再受理 apply Job，登记的
/// meta 与 verify_authorization / read_target_fingerprint 的哈希口径一致。
#[tokio::test]
async fn d1_legacy_full_plan_path_runs_through_job_runtime() {
    let options = pg_mock_options(
        HashMap::from([("public.parent".into(), pg_parent_schema("public.parent"))]),
        HashMap::new(),
    );
    let (test, source_session, target_session) = pg_command_test_sessions(options).await;
    let app = host_command_app(test);
    let plan =
        prepare_pg_table_plan(&app, &source_session, &target_session, &["public.parent"]).await;

    let accepted = execute_schema_diff_deploy(
        app.state(),
        target_session.clone(),
        plan,
        Some(false),
        Some(false),
        None,
        None,
        None,
        Some("public".into()),
        None,
        None,
        None,
    )
    .await
    .expect("legacy path must be accepted");
    assert_eq!(accepted.kind, "schemaDiffApply");
    let details = await_terminal_job(&app, &accepted.job_id).await;
    let result = details
        .deploy_result
        .expect("terminal apply details contain a report");
    assert!(
        matches!(result.status, crate::schema_diff::DeployStatus::Committed),
        "expected Committed, got {result:?}"
    );
}
