//! P5 Wave-1：Schema Diff 窗口命令经 JobRuntime 执行（`p5-schema-diff` 轨）。
//!
//! 结构：
//!
//! * [`SchemaDiffJobInfra`]：进程级 Job 基础设施（仓储/预算/计划库）。
//! * [`AppStateBackend`]：[`SchemaDiffJobBackend`] 的宿主实现。
//! * [`run_prepare_job`] / [`run_apply_job`]：IPC 命令的 JobRuntime 入口。
//!
//! 不变量（§2.1）：同一请求只走一种管理器——IPC 一律构造 Job、注册 handler、
//! 跑 JobRuntime，不直接调旧 prepare/deploy 管理器。

use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::id::{ClientInstanceId, JobId, OrganizationId, PrincipalId, RequestId};
use datazen_runtime::job::{EndpointRef, EndpointRole};

use datazen_schema_diff::job::{
    ApplyRequest, PlanStore, PrepareRequest, PreparedPlan, ReadOnlyVerdict, SchemaDiffFrozenPlan,
    SchemaDiffHandler, SchemaDiffJobBackend, SchemaDiffPlanError, StoredPlan,
};
use datazen_schema_diff::types::{SchemaDiffDeployResult, SchemaDiffPlan};

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::schema_diff::prepare_schema_diff_plan_with_schemas_impl;
use crate::commands::schema_diff::unified_plan::prepare_schema_unified_plan_impl;
use crate::AppState;

mod recovery;

/// 进程级 Job 基础设施。置于 [`AppState`] 中，驻留整个应用生命周期：
/// Job 关闭后仍保留终态与 effect 投影（§8：窗口关闭不释放 Job 资源）。
pub struct SchemaDiffJobInfra {
    pub plans: Arc<PlanStore>,
}

impl SchemaDiffJobInfra {
    pub fn new() -> Self {
        Self {
            plans: Arc::new(PlanStore::new()),
        }
    }
}

fn fnv_hex_of_json<T: serde::Serialize>(v: &T) -> String {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    datazen_schema_diff::job::fnv1a64_hex(&bytes)
}

fn source_session_id(request: &PrepareRequest) -> String {
    match request {
        PrepareRequest::Table {
            source_db_session_id,
            ..
        } => source_db_session_id.clone(),
        PrepareRequest::Unified {
            source_db_session_id,
            ..
        } => source_db_session_id.clone(),
    }
}

fn target_session_id(request: &PrepareRequest) -> String {
    match request {
        PrepareRequest::Table {
            target_db_session_id,
            ..
        } => target_db_session_id.clone(),
        PrepareRequest::Unified {
            target_db_session_id,
            ..
        } => target_db_session_id.clone(),
    }
}

async fn owner_connection_id(state: &AppState, session_id: &str) -> Result<String, CommandError> {
    state
        .connection_manager
        .owner_connection_id(session_id)
        .await
        .ok_or_else(|| CommandError::Validation("Migration connection owner is unavailable".into()))
}

/// §4.2 桌面确认边界：运行时仓配化的恢复策略字符串（§2.2）。
fn recovery_policy_str(transactional_ddl: bool) -> &'static str {
    if transactional_ddl {
        "inTransactionBatch"
    } else {
        "readOnlyVerify"
    }
}

/// capability/版本/credential 快照（§2.2）：当前连接配置的可核验身份。
async fn current_capability_hash(
    state: &AppState,
    target_db_session_id: &str,
) -> Result<String, CommandError> {
    let config = state
        .connection_manager
        .get_session_config(target_db_session_id)
        .await
        .cmd_err("current_capability_hash")?;
    let owner = state
        .connection_manager
        .owner_connection_id(target_db_session_id)
        .await;
    let read_only = match owner.as_ref() {
        Some(owner) => {
            let persisted = state.store.get_connection(owner).await;
            config.read_only || persisted.as_ref().map(|p| p.read_only).unwrap_or(false)
        }
        None => config.read_only,
    };
    Ok(fnv_hex_of_json(&serde_json::json!({
        "databaseType": config.database_type,
        "database": config.database,
        "schema": config.schema,
        "readOnly": read_only,
        "owner": owner,
    })))
}

/// 宿主 [`SchemaDiffJobBackend`] 实现。
pub struct AppStateBackend {
    state: Arc<AppState>,
    plans: Arc<PlanStore>,
    target_session_id: Option<String>,
}

impl AppStateBackend {
    pub fn new(
        state: Arc<AppState>,
        plans: Arc<PlanStore>,
        target_session_id: Option<String>,
    ) -> Self {
        Self {
            state,
            plans,
            target_session_id,
        }
    }
}

#[async_trait]
impl SchemaDiffJobBackend for AppStateBackend {
    async fn prepare_plan(
        &self,
        request: &PrepareRequest,
        _cancel: &datazen_runtime::job::CancelToken,
    ) -> Result<PreparedPlan, SchemaDiffPlanError> {
        let (plan, tgt_sess, target_db, target_schema) = match request {
            PrepareRequest::Table {
                source_db_session_id,
                target_db_session_id,
                table_names,
                target_table_names,
                target_only_table_names,
                source_schema,
                target_schema,
                allow_destructive,
                include_indexes,
                type_overrides,
            } => {
                let plan = prepare_schema_diff_plan_with_schemas_impl(
                    &self.state,
                    source_db_session_id.clone(),
                    target_db_session_id.clone(),
                    table_names.clone(),
                    target_table_names.clone(),
                    target_only_table_names.clone(),
                    *allow_destructive,
                    *include_indexes,
                    Some(type_overrides.clone()),
                    source_schema.clone(),
                    target_schema.clone(),
                )
                .await
                .map_err(|e| SchemaDiffPlanError::PlanNotFound(e.to_string()))?;
                (
                    plan,
                    target_db_session_id.clone(),
                    target_database_from_config(&self.state, &target_db_session_id).await,
                    target_schema.clone(),
                )
            }
            PrepareRequest::Unified {
                source_db_session_id,
                target_db_session_id,
                table_names,
                target_table_names,
                target_only_table_names,
                source_objects,
                target_objects,
                source_schema,
                target_schema,
                allow_destructive,
                include_indexes,
                type_overrides,
            } => {
                let plan = prepare_schema_unified_plan_impl(
                    &self.state,
                    source_db_session_id.clone(),
                    target_db_session_id.clone(),
                    table_names.clone(),
                    Some(target_table_names.clone()),
                    Some(target_only_table_names.clone()),
                    Some(source_objects.clone()),
                    Some(target_objects.clone()),
                    source_schema.clone(),
                    target_schema.clone(),
                    *allow_destructive,
                    *include_indexes,
                    Some(type_overrides.clone()),
                )
                .await
                .map_err(|e| SchemaDiffPlanError::PlanNotFound(e.to_string()))?;
                (
                    plan,
                    target_db_session_id.clone(),
                    target_database_from_config(&self.state, &target_db_session_id).await,
                    target_schema.clone(),
                )
            }
        };
        let recovery_targets = recovery::prepare_target_objects(&self.state, request)
            .await
            .map_err(|_| SchemaDiffPlanError::PlanStale("target identity unavailable".into()))?;
        let plan_id = plan
            .plan_id
            .clone()
            .ok_or_else(|| SchemaDiffPlanError::PlanNotFound("planId missing".into()))?;
        let fingerprint = recovery::fingerprint_targets(
            &self.state,
            &tgt_sess,
            target_db.as_deref(),
            &recovery_targets,
        )
        .await
        .map_err(|e| SchemaDiffPlanError::PlanStale(e.to_string()))?;
        let capability_hash = current_capability_hash(&self.state, &tgt_sess)
            .await
            .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?;
        let transactional_ddl = plan.statements.iter().any(|s| s.requires_transaction)
            || plan.source_dialect.contains("sqlite")
            || plan.source_dialect.contains("postgres");
        let mut endpoint_evidence = vec![format!("planTables:{}", plan.tables.join(","))];
        recovery::record_target_objects(&mut endpoint_evidence, &recovery_targets);
        let meta = SchemaDiffFrozenPlan {
            plan_id: plan_id.clone(),
            plan_version: 1,
            handler_version: 1,
            checkpoint_version: 1,
            selection_revision: 1,
            created_at: datazen_platform_api::id::Timestamp::new(
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            ),
            expires_at: datazen_platform_api::id::Timestamp::new(
                (chrono::Utc::now() + chrono::Duration::hours(2))
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            ),
            source_connection_id: owner_connection_id(&self.state, &source_session_id(request))
                .await
                .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?,
            target_connection_id: owner_connection_id(&self.state, &target_session_id(request))
                .await
                .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?,
            source_database: None,
            target_database: target_db,
            source_schema: None,
            target_schema,
            endpoint_evidence,
            capability_snapshot_hash: capability_hash,
            schema_fingerprint: fingerprint,
            mapping_fingerprint: fnv_hex_of_json(&plan.tables),
            consistency: "tableSnapshot".into(),
            transaction_scope: if transactional_ddl {
                "task".into()
            } else {
                "nonAtomicDDL".into()
            },
            recovery_policy: datazen_schema_diff::job::RecoveryPolicy::parse(recovery_policy_str(
                transactional_ddl,
            ))
            .ok_or(SchemaDiffPlanError::RecoveryPolicyInvalid(
                "policy parse".into(),
            ))?,
            body_artifact_ids: vec![format!("artifact:plan:{plan_id}")],
            body_digests: vec![fnv_hex_of_json(&plan.statements)],
            confirmed_actions: Vec::new(),
        };
        let body_digest = fnv_hex_of_json(&plan.statements);
        Ok(PreparedPlan {
            meta,
            plan,
            body_digest,
        })
    }

    async fn verify_authorization(
        &self,
        plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<(), SchemaDiffPlanError> {
        let target_session = self
            .target_session_id
            .as_deref()
            .ok_or(SchemaDiffPlanError::CapabilityChanged)?;
        let current = current_capability_hash(&self.state, target_session)
            .await
            .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?;
        if current != plan_meta.capability_snapshot_hash {
            return Err(SchemaDiffPlanError::CapabilityChanged);
        }
        Ok(())
    }

    async fn read_target_fingerprint(
        &self,
        _plan: &SchemaDiffPlan,
        plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<String, SchemaDiffPlanError> {
        let target_session =
            self.target_session_id
                .as_deref()
                .ok_or(SchemaDiffPlanError::PlanStale(
                    "target session unavailable".into(),
                ))?;
        let targets = recovery::targets_from_evidence(&plan_meta.endpoint_evidence).ok_or(
            SchemaDiffPlanError::PlanStale("target identity unavailable".into()),
        )?;
        recovery::fingerprint_targets(
            &self.state,
            target_session,
            plan_meta.target_database.as_deref(),
            &targets,
        )
        .await
        .map_err(|e| SchemaDiffPlanError::PlanStale(e.to_string()))
    }

    async fn deploy(
        &self,
        plan: &SchemaDiffPlan,
        request: &ApplyRequest,
        cancel: &datazen_runtime::job::CancelToken,
    ) -> Result<SchemaDiffDeployResult, SchemaDiffPlanError> {
        let result = crate::commands::schema_diff::execute_schema_diff_deploy_impl_with_cancel(
            &self.state,
            request.target_db_session_id.clone(),
            plan.clone(),
            Some(request.use_transaction),
            Some(request.require_rollback),
            request.confirm_destructive.clone(),
            request.job_id.clone(),
            request.target_database.clone(),
            request.target_schema.clone(),
            request
                .profile
                .as_ref()
                .map(|(id, rev)| crate::store::MigrationProfileRef {
                    id: id.clone(),
                    revision: rev.clone(),
                }),
            Some(cancel.flag()),
        )
        .await;
        match result {
            Ok(r) => Ok(r),
            Err(e) => Err(SchemaDiffPlanError::PlanStale(e.to_string())),
        }
    }

    async fn read_only_verify(
        &self,
        _plan_meta: &SchemaDiffFrozenPlan,
        _operation_id: &str,
    ) -> Result<ReadOnlyVerdict, SchemaDiffPlanError> {
        // A fresh session may be supplied by an explicit recovery command. Until a
        // safe after-state projection is available, do not infer a committed outcome
        // from a changed target fingerprint alone.
        Ok(ReadOnlyVerdict::Indeterminate)
    }
}

// ----------------------------------------------------------------- 载荷/上下文

pub(super) fn job_ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new("datazen-local-client"),
        RequestId::new(format!("schema-diff-{}", uuid::Uuid::new_v4())),
        None,
    )
}

pub(super) fn apply_payload(
    stored: &StoredPlan,
    request: &ApplyRequest,
) -> Result<serde_json::Value, CommandError> {
    let plan_bytes = serde_json::to_vec(&stored.plan)
        .map_err(|_| CommandError::Internal("Schema Diff plan digest failed".into()))?;
    let plan_digest = format!("sha256:{:x}", Sha256::digest(plan_bytes));
    let profile_revision_digest = request
        .profile
        .as_ref()
        .map(|profile| {
            serde_json::to_vec(profile)
                .map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
                .map_err(|_| CommandError::Internal("Schema Diff profile digest failed".into()))
        })
        .transpose()?;
    let object_ids = recovery::target_ids_from_evidence(&stored.meta.endpoint_evidence);
    let recovery_targets = serde_json::json!([{
        "connectionId": stored.meta.target_connection_id,
        "objectIds": object_ids,
    }]);
    let mut payload = serde_json::json!({
        "consumedPlanId": stored.meta.plan_id,
        "planDigest": plan_digest,
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "selectionRevision": stored.meta.selection_revision,
        "recoveryTargets": recovery_targets,
        "targetBeforeFingerprint": stored.meta.schema_fingerprint,
        "recoveryPolicy": stored.meta.recovery_policy.as_str(),
        "useTransaction": request.use_transaction,
        "requireRollback": request.require_rollback,
        "confirmedDestructive": request.confirm_destructive.as_deref()
            == Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN),
    });
    if let Some(digest) = profile_revision_digest {
        payload["profileRevisionDigest"] = serde_json::Value::String(digest);
    }
    Ok(payload)
}

pub(super) fn prepare_payload() -> serde_json::Value {
    serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
    })
}

async fn target_database_from_config(state: &AppState, session: &str) -> Option<String> {
    state
        .connection_manager
        .get_session_config(session)
        .await
        .ok()
        .and_then(|c| c.database)
}

async fn target_schema_from_config(state: &AppState, session: &str) -> Option<String> {
    state
        .connection_manager
        .get_session_config(session)
        .await
        .ok()
        .and_then(|c| c.schema)
}

pub(super) fn owner_ref(_state_unused: &AppState) -> datazen_platform_api::OwnerRef {
    datazen_platform_api::OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("desktop"),
        purpose: "migration".into(),
    }
}

pub(super) async fn endpoints_from_session_pair(
    state: &AppState,
    source_session: Option<&str>,
    target_session: &str,
    objects: &[String],
) -> Result<Vec<EndpointRef>, CommandError> {
    let mut out = Vec::new();
    if let Some(session) = source_session {
        let identity = crate::services::migration_endpoint::session_identity(
            &state.connection_manager,
            session,
            None,
        )
        .await?;
        out.push(EndpointRef {
            connection_id: identity.connection_id,
            service_key: identity.service_key,
            objects: objects.to_vec(),
            role: EndpointRole::SourceReader,
        });
    }
    let identity = crate::services::migration_endpoint::session_identity(
        &state.connection_manager,
        target_session,
        None,
    )
    .await?;
    out.push(EndpointRef {
        connection_id: identity.connection_id,
        service_key: identity.service_key,
        objects: objects.to_vec(),
        role: EndpointRole::TargetWriter,
    });
    Ok(out)
}

/// Envelope：prepare 的 IPC 出参（计划 Artifact + 冻结元数据）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffPrepareEnvelope {
    pub plan: SchemaDiffPlan,
    pub plan_id: String,
    pub source_connection_id: String,
    pub target_connection_id: String,
    pub selection_revision: u64,
    pub plan_version: u64,
    pub handler_version: u64,
    pub checkpoint_version: u64,
    pub expires_at: String,
    pub recovery_policy: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::Timestamp;
    use datazen_schema_diff::job::{RecoveryPolicy, SchemaDiffFrozenPlan};
    use datazen_schema_diff::types::{
        PlanRequirement, PlanStatement, RollbackCompleteness, StatementRisk,
    };

    #[test]
    fn apply_payload_persists_only_allowlisted_schema_diff_receipt_fields() {
        let target = recovery::table_target("users", Some("public"));
        let mut evidence = vec!["planTables:users".into()];
        recovery::record_target_objects(&mut evidence, &[target]);
        let stored = StoredPlan {
            meta: SchemaDiffFrozenPlan {
                plan_id: "plan-opaque-1".into(),
                plan_version: 1,
                handler_version: 1,
                checkpoint_version: 1,
                selection_revision: 4,
                created_at: Timestamp::new("2026-10-08T00:00:00Z"),
                expires_at: Timestamp::new("2026-10-08T02:00:00Z"),
                source_connection_id: "source-connection".into(),
                target_connection_id: "target-connection".into(),
                source_database: None,
                target_database: Some("app".into()),
                source_schema: None,
                target_schema: Some("public".into()),
                endpoint_evidence: evidence,
                capability_snapshot_hash: "capability-digest".into(),
                schema_fingerprint: format!("sha256:{}", "a".repeat(64)),
                mapping_fingerprint: "mapping-digest".into(),
                consistency: "tableSnapshot".into(),
                transaction_scope: "task".into(),
                recovery_policy: RecoveryPolicy::ReadOnlyVerify,
                body_artifact_ids: vec!["artifact:plan:plan-opaque-1".into()],
                body_digests: vec!["body-digest".into()],
                confirmed_actions: Vec::new(),
            },
            plan: SchemaDiffPlan {
                plan_id: Some("plan-opaque-1".into()),
                table: "users".into(),
                tables: vec!["users".into()],
                source_dialect: "postgresql".into(),
                target_dialect: "postgresql".into(),
                same_dialect: true,
                statements: vec![PlanStatement {
                    sql: "DROP TABLE private_data".into(),
                    risk: StatementRisk::Destructive,
                    rollback_sql: None,
                    summary: "Remove private data".into(),
                    requires_transaction: false,
                }],
                warnings: Vec::new(),
                requirements: vec![PlanRequirement::Unsupported {
                    operation: "private_data".into(),
                    reason: "not selected".into(),
                }],
                rollback_completeness: RollbackCompleteness {
                    complete: false,
                    missing: vec!["private_data".into()],
                },
                type_suggestions: Vec::new(),
                expected_target_schemas: Vec::new(),
            },
            body_digest: "body-digest".into(),
        };
        let request = ApplyRequest {
            target_db_session_id: "target-live-session".into(),
            use_transaction: true,
            require_rollback: true,
            confirm_destructive: Some("DEPLOY".into()),
            job_id: Some("schemaDiffApply-live-id".into()),
            target_database: None,
            target_schema: None,
            profile: Some(("private-profile-id".into(), "revision-7".into())),
        };

        let payload = apply_payload(&stored, &request).expect("safe payload projection succeeds");
        assert_eq!(payload["useTransaction"], true);
        assert_eq!(payload["requireRollback"], true);
        assert_eq!(payload["selectionRevision"], 4);
        assert!(payload["planDigest"]
            .as_str()
            .is_some_and(|value| { value.starts_with("sha256:") && value.len() == 71 }));
        assert!(payload["profileRevisionDigest"]
            .as_str()
            .is_some_and(|value| { value.starts_with("sha256:") && value.len() == 71 }));
        let encoded = payload.to_string();
        for forbidden in [
            "DROP TABLE private_data",
            "target-live-session",
            "schemaDiffApply-live-id",
            "private-profile-id",
            "revision-7",
        ] {
            assert!(!encoded.contains(forbidden), "payload contains {forbidden}");
        }
        assert_eq!(
            payload["recoveryTargets"][0]["connectionId"],
            "target-connection"
        );
        assert!(payload["recoveryTargets"][0]["objectIds"][0]
            .as_str()
            .is_some_and(|id| id.starts_with("obj-") && id.len() <= 512));
    }
}

pub mod desktop;
pub use desktop::{
    cancel_schema_diff_job, get_schema_diff_job_details, list_schema_diff_jobs, run_apply_job,
    run_prepare_job, verify_schema_diff_job_recovery, SchemaDiffJobAccepted, SchemaDiffJobDetails,
};

/// 旧 IPC 兼容路径：客户端直接携带计划正文时，先注册进 PlanStore 再走 apply Job。
/// 两种受理（prepare 产物 / 直接携带正文）都满足「只经 JobRuntime」。
/// D1：登记的 meta 与 verify_authorization/read_target_fingerprint 使用
/// **相同**的哈希计算方式（current_capability_hash / compute_target_fingerprint），
/// 保证兼容路径调用能跑通。
pub async fn register_plan_for_apply(
    state: &AppState,
    plan: SchemaDiffPlan,
    target_db_session_id: &str,
) -> Result<(String, u64), CommandError> {
    let plan_id = plan
        .plan_id
        .clone()
        .ok_or_else(|| CommandError::Validation("plan requires planId".into()))?;
    let target_schema = target_schema_from_config(state, target_db_session_id).await;
    let targets = plan
        .tables
        .iter()
        .map(|table| recovery::table_target(table, target_schema.as_deref()))
        .collect::<Vec<_>>();
    let fingerprint = recovery::fingerprint_targets(
        state,
        target_db_session_id,
        target_database_from_config(state, target_db_session_id)
            .await
            .as_deref(),
        &targets,
    )
    .await?;
    let mut endpoint_evidence = vec![format!("planTables:{}", plan.tables.join(","))];
    recovery::record_target_objects(&mut endpoint_evidence, &targets);
    let meta = SchemaDiffFrozenPlan {
        plan_id: plan_id.clone(),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: 1,
        created_at: datazen_platform_api::id::Timestamp::new(
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        ),
        expires_at: datazen_platform_api::id::Timestamp::new(
            (chrono::Utc::now() + chrono::Duration::hours(2))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        ),
        source_connection_id: String::new(),
        target_connection_id: owner_connection_id(state, target_db_session_id).await?,
        source_database: None,
        target_database: target_database_from_config(state, target_db_session_id).await,
        source_schema: None,
        target_schema,
        endpoint_evidence,
        capability_snapshot_hash: current_capability_hash(state, target_db_session_id)
            .await
            .map_err(|e| e)?,
        schema_fingerprint: fingerprint,
        mapping_fingerprint: datazen_schema_diff::job::fnv1a64_hex(
            &serde_json::to_vec(&plan.tables).unwrap_or_default(),
        ),
        consistency: "tableSnapshot".into(),
        transaction_scope: "task".into(),
        recovery_policy: datazen_schema_diff::job::RecoveryPolicy::ReadOnlyVerify,
        body_artifact_ids: vec![],
        body_digests: vec![],
        confirmed_actions: Vec::new(),
    };
    state.schema_diff_jobs.plans.insert(StoredPlan {
        meta,
        plan,
        body_digest: String::new(),
    });
    Ok((plan_id, 1))
}
