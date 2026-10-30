//! Schema Diff deploy execution and cancellation.
//!
//! host-boundary F4c-3：复核消费与部署前目标/源验证下沉
//! `schema_diff::api::{consume_and_verify_reviewed_plan, revalidate_source_snapshot}`；
//! 本层保留历史记账、只读/破坏性门与 JobRuntime 接线，验证序列的错误
//! 顺序与文本逐字节不变。

use datazen::migration::schema_diff::types::PlanRequirement;
use datazen_driver_api::SchemaMigration as _;

use super::job;
use super::{
    plan_has_destructive, reviewed_scope, run_schema_diff_deploy, schema_diff_api,
    schema_diff_endpoint, AppState, CommandError, DeployOptions, SchemaDiffDeployResult,
    SchemaDiffPlan, SqlTarget, State, DESTRUCTIVE_CONFIRM_TOKEN,
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
        datazen::execution::application::LivePurpose::LongLivedHostWork,
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
    // Only `Unsupported` blocks deployment. `Backfill` is a follow-up the plan
    // performed *safely*: a NOT NULL column without a default is added nullable
    // and the requirement tells the user to populate existing rows before
    // enforcing NOT NULL. Refusing the deploy on it left the plan permanently
    // undeployable — preparing again cannot clear it, because the statement is
    // already the safe one.
    if plan
        .requirements
        .iter()
        .any(|requirement| matches!(requirement, PlanRequirement::Unsupported { .. }))
    {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation(
                "Resolve unsupported plan requirements and prepare again before deploying".into(),
            ),
        )
        .await;
    }
    if plan
        .requirements
        .iter()
        .any(|requirement| matches!(requirement, PlanRequirement::Backfill { .. }))
    {
        tracing::warn!(
            requirements = plan.requirements.len(),
            "deploying a plan with pending backfills; the added columns stay nullable"
        );
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

    // 复核消费 + 部署前目标侧验证（含表 DROP 的完整目录复核、逐表/对象
    // 快照复核、对象目录与表身份复核、MySQL 视图创建上下文复核）。
    let target_endpoint = schema_diff_api::SchemaDiffEndpoint {
        driver: driver.clone(),
        handle: handle.clone(),
        config: config.clone(),
    };
    let reviewed = match schema_diff_api::consume_and_verify_reviewed_plan(
        state.schema_diff_reviewed_plans.as_ref(),
        &reviewed_scope(&state)?,
        &plan,
        &target_endpoint,
        &target_db_session_id,
        effective_target_database,
        effective_target_schema,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return fail_schema_diff_deploy(&state, history_run, error.into()).await;
        }
    };
    // 源侧快照复核按原顺序随后执行（源会话从复核记录解析）。
    if let Some(source_snapshot) = reviewed.source_snapshot.as_ref() {
        let source_context = match crate::services::migration_endpoint::runtime_session_context(
            &state,
            &source_snapshot.session,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) = schema_diff_api::revalidate_source_snapshot(
            &schema_diff_endpoint(source_context),
            &reviewed,
        )
        .await
        {
            return fail_schema_diff_deploy(&state, history_run, error.into()).await;
        }
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
