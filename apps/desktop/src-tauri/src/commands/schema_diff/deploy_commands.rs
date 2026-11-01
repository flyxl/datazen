//! Schema Diff deploy execution and cancellation.
//!
//! host-boundary F4c-3：复核消费与部署前目标/源验证下沉
//! `schema_diff::api::{consume_and_verify_reviewed_plan, revalidate_source_snapshot}`。
//! B2 schema_diff 域刀：`execute_schema_diff_deploy_impl` /
//! `execute_schema_diff_deploy_impl_with_cancel` 转发体已删——部署编排
//! （历史记账、只读/破坏性门、复核消费、DDL 执行）逐字迁入 `job.rs`
//! （生产调用方 `AppStateBackend::deploy`），本文件只剩 JobRuntime 的
//! 命令入口；验证序列的错误顺序与文本逐字节不变。

use super::job;
use super::{AppState, CommandError, SchemaDiffPlan, State};

/// Execute a reviewed schema diff plan on the target connection.
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

/// Persist cancellation intent against the shared desktop Job lifecycle.
#[tauri::command]
pub async fn cancel_schema_diff_deploy(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<bool, CommandError> {
    job::cancel_schema_diff_job(&state, &job_id).await
}
