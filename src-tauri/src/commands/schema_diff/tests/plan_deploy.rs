//! Plan preparation, deploy gating and profile persistence tests.

use super::super::profile_commands::validate_schema_diff_profile_connections;
use super::*;

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
    source_schemas: HashMap<String, crate::db::TableSchema>,
    target_schemas: HashMap<String, crate::db::TableSchema>,
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

async fn prepare_pg_table_plan(
    test: &crate::testing::app_state::TestAppState,
    source_session: &str,
    target_session: &str,
    tables: &[&str],
) -> SchemaDiffPlan {
    prepare_schema_diff_plan_with_schemas_impl(
        &test.state,
        source_session.to_string(),
        target_session.to_string(),
        tables.iter().map(|table| (*table).into()).collect(),
        tables.iter().map(|table| (*table).into()).collect(),
        Vec::new(),
        false,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("prepare PostgreSQL table plan")
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

async fn prepare_pg_target_only_drop_plan(
    test: &crate::testing::app_state::TestAppState,
    source_session: &str,
    target_session: &str,
    tables: &[&str],
) -> SchemaDiffPlan {
    prepare_schema_diff_plan_with_schemas_impl(
        &test.state,
        source_session.to_string(),
        target_session.to_string(),
        Vec::new(),
        Vec::new(),
        tables.iter().map(|table| (*table).into()).collect(),
        true,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("prepare PostgreSQL target-only drop plan")
}

async fn deploy_pg_reviewed_plan(
    test: &crate::testing::app_state::TestAppState,
    target_session: &str,
    plan: SchemaDiffPlan,
) -> Result<crate::schema_diff::SchemaDiffDeployResult, CommandError> {
    execute_schema_diff_deploy_impl(
        &test.state,
        target_session.to_string(),
        plan,
        Some(false),
        Some(false),
        None,
        None,
        None,
        None,
        None,
    )
    .await
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
    let plan = prepare_pg_table_plan(
        &test,
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

    let result = deploy_pg_reviewed_plan(&test, &target_session, plan)
        .await
        .expect("unchanged create/FK plan should deploy");
    assert_eq!(result.status, crate::schema_diff::DeployStatus::Committed);
    assert!(result.executed_count > 0);
    assert!(test.mock.execute_calls() > 0);
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
    let plan = prepare_pg_table_plan(
        &test,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;
    assert!(plan.statements.iter().any(|statement| {
        statement.summary.contains("FOREIGN KEY") || statement.sql.contains("FOREIGN KEY")
    }));

    let result = deploy_pg_reviewed_plan(&test, &target_session, plan)
        .await
        .expect("unchanged add-FK plan should deploy");
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
    let plan = prepare_pg_table_plan(
        &test,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;

    test.mock
        .set_table_schema_for_test("target_db", "child", pg_child_schema("child", true));
    let error = deploy_pg_reviewed_plan(&test, &target_session, plan)
        .await
        .expect_err("a post-review FK change must invalidate the plan");
    assert!(error
        .to_string()
        .contains("Target schema changed for public.child"));
    assert_eq!(test.mock.execute_calls(), 0);
}

#[tokio::test]
async fn postgres_child_parent_drop_revalidates_qualified_catalog_before_deploy() {
    let (test, source_session, target_session) =
        pg_command_test_sessions(pg_target_drop_options()).await;
    let plan = prepare_pg_target_only_drop_plan(
        &test,
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

    let result = execute_schema_diff_deploy_impl(
        &test.state,
        target_session,
        plan,
        Some(false),
        Some(false),
        Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN.to_string()),
        None,
        None,
        None,
        None,
    )
    .await
    .expect("unchanged full target catalog should validate");
    assert_eq!(result.status, crate::schema_diff::DeployStatus::Committed);
    assert_eq!(result.executed_count, 2);
}

#[tokio::test]
async fn postgres_new_dependent_relation_after_review_blocks_drop_before_write() {
    let (test, source_session, target_session) =
        pg_command_test_sessions(pg_target_drop_options()).await;
    let plan = prepare_pg_target_only_drop_plan(
        &test,
        &source_session,
        &target_session,
        &["public.parent", "public.child"],
    )
    .await;
    test.mock
        .add_table_for_test("target_db", target_table("late_child", Some("public")));

    let error = execute_schema_diff_deploy_impl(
        &test.state,
        target_session,
        plan,
        Some(false),
        Some(false),
        Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN.to_string()),
        None,
        None,
        None,
        None,
    )
    .await
    .expect_err("a new dependency catalog relation must invalidate the drop plan");
    assert!(error
        .to_string()
        .contains("Target dependency catalog changed after review"));
    assert!(error.to_string().contains("public.late_child"));
    assert_eq!(test.mock.execute_calls(), 0);
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

/// D1：兼容路径（未传 planId、直接携带计划正文）登记的 meta 必须与
/// verify_authorization / read_target_fingerprint 的哈希口径一致，
/// 使不携带 planId 的旧调用也能经 JobRuntime 跑通 apply。
#[tokio::test]
async fn d1_legacy_full_plan_path_runs_through_job_runtime() {
    let options = pg_mock_options(
        HashMap::from([("public.parent".into(), pg_parent_schema("public.parent"))]),
        HashMap::new(),
    );
    let (test, source_session, target_session) = pg_command_test_sessions(options).await;
    let plan =
        prepare_pg_table_plan(&test, &source_session, &target_session, &["public.parent"]).await;

    let (plan_id, revision) = job::register_plan_for_apply(&test.state, plan, &target_session)
        .await
        .expect("register legacy plan");
    let apply_request = crate::schema_diff::job::ApplyRequest {
        target_db_session_id: target_session.clone(),
        use_transaction: false,
        require_rollback: false,
        confirm_destructive: None,
        job_id: None,
        target_database: None,
        target_schema: Some("public".into()),
        profile: None,
    };
    let accepted = job::run_apply_job(&test.state, &plan_id, revision, apply_request)
        .await
        .expect("legacy path must be accepted");
    assert_eq!(accepted.kind, "schemaDiffApply");
    let details = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let details = job::desktop::read_schema_diff_job_details(&test.state, &accepted.job_id)
                .await
                .expect("accepted Schema Diff Job remains queryable");
            if details.details.job.state.is_terminal() {
                break details;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("legacy apply Job reaches a durable terminal state");
    let result = details
        .deploy_result
        .expect("terminal apply details contain a report");
    assert!(
        matches!(result.status, crate::schema_diff::DeployStatus::Committed),
        "expected Committed, got {result:?}"
    );
}
