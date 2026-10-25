//! Schema Diff deploy execution and cancellation.

use datazen_driver_api::SchemaMigration as _;

use super::job;
use super::unified_plan;
use super::{
    fetch_schema_object, fetch_target_table_dependency_catalog, fetch_target_table_schema,
    has_table_drop_statement, plan_has_destructive, resolve_reviewed_table_snapshot,
    reviewed_scope, run_schema_diff_deploy, validate_target_dependency_catalog, AppState,
    CommandError, DeployOptions, SchemaDiffDeployResult, SchemaDiffPlan, SqlTarget, State,
    DESTRUCTIVE_CONFIRM_TOKEN,
};

/// Execute a reviewed schema diff plan on the target connection.
async fn fail_schema_diff_deploy<T>(
    state: &AppState,
    run: crate::store::MigrationRunRecord,
    error: CommandError,
) -> Result<T, CommandError> {
    crate::commands::history::finish_migration_run(state, run, false, false, 0, 1, 0, "unknown")
        .await;
    Err(error)
}

#[tauri::command]
pub async fn execute_schema_diff_deploy(
    state: State<'_, AppState>,
    target_db_session_id: String,
    plan: SchemaDiffPlan,
    use_transaction: Option<bool>,
    require_rollback: Option<bool>,
    confirm_destructive: Option<String>,
    job_id: Option<String>,
    target_database: Option<String>,
    target_schema: Option<String>,
    profile: Option<crate::store::MigrationProfileRef>,
    plan_id: Option<String>,
    selection_revision: Option<u64>,
) -> Result<job::SchemaDiffJobAccepted, CommandError> {
    let apply_request = crate::schema_diff::job::ApplyRequest {
        target_db_session_id: target_db_session_id.clone(),
        use_transaction: use_transaction.unwrap_or(true),
        require_rollback: require_rollback.unwrap_or(false),
        confirm_destructive,
        job_id,
        target_database: target_database.clone(),
        target_schema: target_schema.clone(),
        profile: profile.map(|p| (p.id, p.revision)),
    };
    let (effective_plan_id, effective_selection_revision) = match (plan_id, selection_revision) {
        (Some(plan_id), Some(sel_rev)) => (plan_id, sel_rev),
        _ => {
            // 兼容路径：客户端直接携带计划正文 ⇒ 先注册进 PlanRepository 再跑 apply Job。
            job::register_plan_for_apply(&state, plan, &target_db_session_id).await?
        }
    };
    job::run_apply_job(
        &state,
        &effective_plan_id,
        effective_selection_revision,
        apply_request,
    )
    .await
}

/// 进程内直接部署 Schema Diff（不挂 Job）的执行入口。
///
/// 用户可见的部署命令走 `JobRuntime`（见 `execute_schema_diff_deploy`）。这个
/// 无取消令的薄封装在 workflow 的迁移 step 移除后只剩三件套自己的用例在调用，
/// 因此非测试构建里它是死代码——保留它是因为它属于 Schema Diff 域自身的执行面。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn execute_schema_diff_deploy_impl(
    state: &AppState,
    target_db_session_id: String,
    plan: SchemaDiffPlan,
    use_transaction: Option<bool>,
    require_rollback: Option<bool>,
    confirm_destructive: Option<String>,
    job_id: Option<String>,
    target_database: Option<String>,
    target_schema: Option<String>,
    profile: Option<crate::store::MigrationProfileRef>,
) -> Result<SchemaDiffDeployResult, CommandError> {
    execute_schema_diff_deploy_impl_with_cancel(
        state,
        target_db_session_id,
        plan,
        use_transaction,
        require_rollback,
        confirm_destructive,
        job_id,
        target_database,
        target_schema,
        profile,
        None,
    )
    .await
}

pub(crate) async fn execute_schema_diff_deploy_impl_with_cancel(
    state: &AppState,
    target_db_session_id: String,
    plan: SchemaDiffPlan,
    use_transaction: Option<bool>,
    require_rollback: Option<bool>,
    confirm_destructive: Option<String>,
    // Legacy client key, kept only for the historical IPC signature: the
    // window-registry lookup it used to arm is gone (see the `cancelled`
    // comment below).
    _job_id: Option<String>,
    target_database: Option<String>,
    target_schema: Option<String>,
    profile: Option<crate::store::MigrationProfileRef>,
    cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<SchemaDiffDeployResult, CommandError> {
    crate::commands::history::validate_migration_profile_ref(
        &state,
        "schemaDiff",
        profile.as_ref(),
    )
    .await?;
    let mut history_run =
        crate::commands::history::start_migration_run(&state, "schemaDiff", profile.as_ref()).await;
    history_run.selected_count = plan.statements.len() as u64;
    tracing::info!(
        %target_db_session_id,
        statements = plan.statements.len(),
        ?_job_id,
        "execute_schema_diff_deploy"
    );

    let target_context = match crate::services::migration_endpoint::runtime_session_context(
        &state,
        &target_db_session_id,
        datazen_runtime::application::LivePurpose::LongLivedHostWork,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
    };
    let driver = target_context.driver.clone();
    let handle = target_context.handle.clone();
    let config = target_context.config.clone();
    let owner = target_context.view.connection_id.as_str().to_owned();
    history_run.target_connection_id = Some(owner.clone());
    let persisted =
        match super::persisted_connection(&state, &owner, "execute_schema_diff_deploy").await? {
            Some(value) => value,
            None => {
                return fail_schema_diff_deploy(
                    &state,
                    history_run,
                    CommandError::Validation("Target connection was removed".into()),
                )
                .await;
            }
        };
    if config.read_only || persisted.read_only {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation("Target connection is read-only".into()),
        )
        .await;
    }
    let effective_target_database = target_database
        .as_deref()
        .or(config.database.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let effective_target_schema = target_schema
        .as_deref()
        .or(config.schema.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if require_rollback.unwrap_or(false)
        && (!plan.rollback_completeness.complete
            || !use_transaction.unwrap_or(true)
            || !matches!(
                datazen_driver_api::LegacySchemaMigration::new(driver.as_ref()).ddl_atomicity(),
                datazen_driver_api::DdlAtomicity::Transactional
            ))
    {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation(
                "Complete rollback requires transactional DDL and a complete rollback plan".into(),
            ),
        )
        .await;
    }
    if !plan.requirements.is_empty() {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation(
                "Resolve plan requirements and prepare again before deploying".into(),
            ),
        )
        .await;
    }
    if plan_has_destructive(&plan) {
        let token = confirm_destructive.as_deref().unwrap_or("");
        if token != DESTRUCTIVE_CONFIRM_TOKEN {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation(format!(
                    "destructive plan requires confirm_destructive = \"{DESTRUCTIVE_CONFIRM_TOKEN}\""
                )),
            )
            .await;
        }
    }

    let reviewed = match crate::schema_diff::reviewed::consume(
        state.schema_diff_reviewed_plans.as_ref(),
        &reviewed_scope(state)?,
        &plan,
        &target_db_session_id,
        &handle,
        &config,
        effective_target_database,
        effective_target_schema,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    };
    if has_table_drop_statement(&reviewed.plan) {
        if !reviewed.has_complete_target_dependency_catalog {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation(
                    "Cannot verify the complete target dependency catalog for this table drop; compare again with full catalog visibility".into(),
                ),
            )
            .await;
        }
        let current_catalog = match fetch_target_table_dependency_catalog(
            driver.as_ref(),
            &handle,
            effective_target_database.unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
            &[],
        )
        .await
        {
            Ok(catalog) => catalog,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) =
            validate_target_dependency_catalog(&reviewed.snapshots, &current_catalog)
        {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    } else {
        for (table, snapshot) in &reviewed.snapshots {
            let (relation, snapshot_database, snapshot_schema) = resolve_reviewed_table_snapshot(
                &config.database_type,
                table,
                effective_target_database.unwrap_or_default(),
                effective_target_schema,
            );
            let current = match fetch_target_table_schema(
                driver.as_ref(),
                &handle,
                &relation,
                &snapshot_database,
                snapshot_schema.as_deref(),
            )
            .await
            {
                Ok(value) => value,
                Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
            };
            if let Err(error) =
                crate::schema_diff::reviewed::validate_snapshot(table, snapshot, &current)
            {
                return fail_schema_diff_deploy(
                    &state,
                    history_run,
                    CommandError::Validation(error),
                )
                .await;
            }
        }
    }
    for snapshot in &reviewed.object_snapshots {
        let object = datazen_driver_api::DatabaseObject {
            kind: snapshot.kind.as_str().into(),
            schema: snapshot.schema.clone(),
            name: snapshot.name.clone(),
            signature: snapshot.signature.clone(),
            target_schema: snapshot.target_schema.clone(),
            target_name: snapshot.target_name.clone(),
        };
        let current = match fetch_schema_object(driver.as_ref(), &handle, &object).await {
            Ok(value) => value,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) =
            crate::schema_diff::reviewed::validate_object_snapshot(snapshot, &current)
        {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    }
    if reviewed.has_complete_target_object_catalog {
        let mut current_catalog_complete = true;
        let (_, current_object_catalog) = match unified_plan::read_target_object_catalog(
            driver.as_ref(),
            &handle,
            config.database.as_deref().unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
            &mut current_catalog_complete,
        )
        .await
        {
            value if current_catalog_complete => value,
            _ => {
                return fail_schema_diff_deploy(
                    &state,
                    history_run,
                    CommandError::Validation(
                        "Target object dependency catalog can no longer be proved complete; compare again".into(),
                    ),
                )
                .await
            }
        };
        if let Err(error) = crate::schema_diff::reviewed::validate_object_dependency_catalog(
            &reviewed.object_dependency_catalog,
            &current_object_catalog,
        ) {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
        let current_table_catalog = match unified_plan::read_target_table_identities(
            driver.as_ref(),
            &handle,
            config.database.as_deref().unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
        )
        .await
        {
            Ok(value) => value,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) = crate::schema_diff::reviewed::validate_table_identity_catalog(
            &reviewed.target_table_identity_catalog,
            &current_table_catalog,
        ) {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    }
    if let Some(expected_context) = reviewed.target_mysql_view_scope_context.as_ref() {
        let current_context =
            unified_plan::read_mysql_view_scope_context(driver.as_ref(), &handle).await;
        if current_context.as_ref() != Some(expected_context) {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation(
                    "Target MySQL view creation context changed after review; compare again".into(),
                ),
            )
            .await;
        }
    }
    if let Err(error) = unified_plan::revalidate_source_snapshot(&state, &reviewed).await {
        return fail_schema_diff_deploy(&state, history_run, error).await;
    }
    let plan = reviewed.plan;
    // The durable Job path always passes a cancel flag (`run_apply_job` drives
    // the deploy through the runtime), and the unattended workflow bridge
    // passes no job id at all, so the legacy window-registry lookup retired
    // together with the old connection manager: `cancelled` is just the
    // external flag when present, `None` otherwise.
    let cancelled = cancel_flag;

    let opts = DeployOptions {
        use_transaction: use_transaction.unwrap_or(true),
        stop_on_error: true,
    };

    let result = run_schema_diff_deploy(
        driver.as_ref(),
        &handle,
        &plan,
        opts,
        cancelled,
        SqlTarget::new(effective_target_database, effective_target_schema),
    )
    .await;

    tracing::info!(
        ?result.status,
        executed = result.executed_count,
        "execute_schema_diff_deploy OK"
    );
    let cancelled_outcome = matches!(result.status, crate::schema_diff::DeployStatus::Cancelled);
    let success_outcome = matches!(result.status, crate::schema_diff::DeployStatus::Committed);
    let rollback_outcome = match result.status {
        crate::schema_diff::DeployStatus::RolledBack => "completed",
        crate::schema_diff::DeployStatus::Unknown | crate::schema_diff::DeployStatus::Mixed => {
            "unknown"
        }
        _ => "notRequired",
    };
    crate::commands::history::finish_migration_run(
        &state,
        history_run,
        success_outcome,
        cancelled_outcome,
        result.executed_count as u64,
        result.errors.len() as u64,
        0,
        rollback_outcome,
    )
    .await;
    Ok(result)
}

/// Persist cancellation intent against the shared desktop Job lifecycle.
#[tauri::command]
pub async fn cancel_schema_diff_deploy(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<bool, CommandError> {
    job::cancel_schema_diff_job(&state, &job_id).await
}
