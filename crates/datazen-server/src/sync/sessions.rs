//! Data Sync 的服务端专用会话与端点身份（W8 刀3e-4a）。
//!
//! 桌面对照 `commands/sync/job_api.rs` 的 `own_sessions` / `release_sessions`
//! 与 `services/migration_endpoint.rs` 的 `open/close_dedicated_session`：
//! durable Job 在**自有的专用会话**上跑（用户授权会话只用于受理前校验
//! 物理身份），受理时开、派发收尾时释放。服务端的 runtime 用例面
//! （`open_session` / `close_session`）与桌面是同一份共享层实现——本模块
//! 只是会话目录闸 + 组织作用域的服务端胶水。
//!
//! 身份闸（桌面 `session_identity` 同语义）：专用会话开立后必须解析出与
//! 用户会话**相同的物理端点身份**（`checked_identify`——host/port 驱动
//! 默认补齐 + database/schema + 位置性 options 的摘要），否则视为「专用
//! 会话不指向评审时的端点」拒绝。文件型驱动的观察命名空间不覆盖配置
//! 路径（两个不同文件不得塌缩成同一物理端点）。

use datazen::jobs::{EndpointRef, EndpointRole};
use datazen::services::migration_endpoint::EndpointIdentity;
use datazen::shared::context::RequestContext;
use datazen::shared::scope::OrganizationScope;

use crate::schema_diff::endpoints::{EndpointError, ResolvedSession, SessionEndpoints};

use super::execution::SyncExecutionError;

/// Runtime-only dedicated sessions. Their ids are never part of a durable plan.
pub(crate) struct OwnedSessions {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) source_identity: EndpointIdentity,
    pub(crate) target_identity: EndpointIdentity,
    pub(crate) source_endpoint: EndpointRef,
    pub(crate) target_endpoint: EndpointRef,
    pub(crate) source_target: datazen::jobs::domain::job::JobRecoveryTarget,
    pub(crate) target_target: datazen::jobs::domain::job::JobRecoveryTarget,
}

impl OwnedSessions {
    pub(crate) fn endpoints(&self) -> Vec<EndpointRef> {
        vec![self.source_endpoint.clone(), self.target_endpoint.clone()]
    }

    pub(crate) fn recovery_targets(&self) -> Vec<datazen::jobs::domain::job::JobRecoveryTarget> {
        vec![self.source_target.clone(), self.target_target.clone()]
    }

    pub(crate) fn ids(&self) -> Vec<String> {
        vec![self.source_id.clone(), self.target_id.clone()]
    }
}

/// 会话 → 物理端点身份（带 database/schema 作用域覆盖；桌面
/// `session_identity(state, id, Some((db, schema)))` 的服务端对应物）。
///
/// 作用域覆盖规则与桌面逐字一致：请求的 database 优先（空值塌缩到
/// 「未请求」），否则保留解析出的配置；schema 走 `metadata_schema`
/// 梯子（驱动感知）。`checked_identify` 在无位置键可指认时 fail-closed。
pub(crate) async fn scoped_identity(
    endpoints: &SessionEndpoints,
    ctx: &RequestContext,
    db_session_id: &str,
    scope: Option<(&str, Option<&str>)>,
) -> Result<EndpointIdentity, SyncExecutionError> {
    let session = endpoints
        .resolve(
            ctx,
            db_session_id,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await
        .map_err(endpoint_error)?;
    identity_of_session(&session, scope)
}

/// 已解析会话 → 端点身份（观察上下文覆盖 + 可选作用域覆盖）。
fn identity_of_session(
    session: &ResolvedSession,
    scope: Option<(&str, Option<&str>)>,
) -> Result<EndpointIdentity, SyncExecutionError> {
    let driver = &session.driver;
    let mut config = session.config.clone();
    // 文件型驱动上报合成命名空间（sqlite 对任何文件都答 main）：取它的
    // 答案会让两个不同文件看起来是同一端点，自覆写闸会拒绝一次合法迁移。
    if !driver.database_names_location() {
        config.database = session
            .view
            .observed_context
            .namespace
            .database
            .clone()
            .or(config.database);
    }
    config.schema = session
        .view
        .observed_context
        .namespace
        .schema
        .clone()
        .or(config.schema);
    if let Some((database, schema)) = scope {
        config.database = datazen::services::migration_endpoint::normalized(Some(database))
            .or_else(|| {
                datazen::services::migration_endpoint::normalized(config.database.as_deref())
            })
            .map(str::to_owned);
        config.schema = datazen::services::schema_scope::metadata_schema(
            driver.as_ref(),
            schema,
            None,
            config.schema.as_deref(),
        );
    }
    config.id = session.view.connection_id.as_str().to_owned();
    datazen::services::migration_endpoint::checked_identify(&config, driver.as_ref())
        .map_err(|error| SyncExecutionError::Validation(error.to_string()))
}

fn endpoint_error(error: EndpointError) -> SyncExecutionError {
    match error {
        // 防枚举：统一文案的会话不可见与真实的解析失败都折成校验错。
        EndpointError::NotVisible => SyncExecutionError::Validation(
            "The addressed resource does not exist or is not visible to you.".into(),
        ),
        EndpointError::ProfileUnavailable => {
            SyncExecutionError::Validation("Connection profile unavailable".into())
        }
        EndpointError::Unavailable(source) => {
            SyncExecutionError::Validation(format!("session endpoint unavailable: {source}"))
        }
    }
}

fn endpoint_ref(identity: &EndpointIdentity, database: &str, role: EndpointRole) -> EndpointRef {
    EndpointRef {
        connection_id: identity.connection_id.clone(),
        service_key: identity.service_key.clone(),
        objects: vec![database.to_string()],
        role,
    }
}

fn recovery_target(
    identity: &EndpointIdentity,
    database: &str,
    schema: Option<&str>,
) -> datazen::jobs::domain::job::JobRecoveryTarget {
    use sha2::{Digest, Sha256};
    let scope = serde_json::json!({ "database": database, "schema": schema });
    let digest = Sha256::digest(serde_json::to_vec(&scope).unwrap_or_default());
    datazen::jobs::domain::job::JobRecoveryTarget {
        connection_id: identity.connection_id.as_str().to_string(),
        object_ids: vec![format!("scope:{digest:x}")],
    }
}

/// 开一个 runtime 专用会话（桌面 `open_dedicated_session` 同语义：提交令牌
/// + `OpenSessionRequest`，owner 记 `ClientSession{ purpose }`）。
pub(crate) async fn open_dedicated_session(
    endpoints: &SessionEndpoints,
    ctx: &RequestContext,
    connection_id: &str,
    database: Option<&str>,
    purpose: &str,
) -> Result<String, SyncExecutionError> {
    use datazen::execution::application::idempotency::IdempotentOperation;
    use datazen::execution::application::requests::OpenSessionRequest;
    use datazen::execution::application::use_cases::ConnectionUseCases as _;
    use datazen::execution::target::{ExecutionTarget, NamespaceTarget};
    use datazen::shared::context::OwnerRef;
    use datazen::shared::id::ConnectionId;

    let database = datazen::services::migration_endpoint::normalized(database);
    let runtime = endpoints
        .lease
        .resolve()
        .await
        .map_err(SyncExecutionError::unavailable)?;
    let token = runtime
        .issue_submission_token(ctx, IdempotentOperation::OpenSession, None)
        .await
        .map_err(|error| SyncExecutionError::unavailable(error.to_string()))?;
    let receipt = runtime
        .open_session(
            ctx,
            OpenSessionRequest {
                initial_target: ExecutionTarget::new(
                    ConnectionId::new(connection_id.to_owned()),
                    NamespaceTarget::new(database.map(str::to_owned), None, None, Vec::new()),
                ),
                owner: OwnerRef::ClientSession {
                    client_instance_id: ctx.client_instance_id.clone(),
                    purpose: purpose.to_owned(),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .map_err(|error| SyncExecutionError::unavailable(error.to_string()))?;
    Ok(receipt.session.handle.db_session_id.as_str().to_owned())
}

/// 释放一个 runtime 专用会话（幂等：已回收的会话按成功处理——保存连接
/// 档案会退役该档案的全部会话，晚到的释放不得向用户报「会话已释放」）。
pub(crate) async fn close_dedicated_session(
    endpoints: &SessionEndpoints,
    ctx: &RequestContext,
    db_session_id: &str,
) -> Result<(), SyncExecutionError> {
    use datazen::execution::application::requests::{CloseMode, CloseSessionRequest};
    use datazen::execution::application::use_cases::ConnectionUseCases as _;

    let runtime = endpoints
        .lease
        .resolve()
        .await
        .map_err(SyncExecutionError::unavailable)?;
    let view = match runtime.get_session_by_id(ctx, db_session_id).await {
        Ok(view) => view,
        Err(error)
            if matches!(
                error.code,
                datazen::shared::error::ApiErrorCode::SessionNotFound
                    | datazen::shared::error::ApiErrorCode::SessionLost
            ) =>
        {
            tracing::info!(db_session_id, "session already retired before close");
            return Ok(());
        }
        Err(error) => return Err(SyncExecutionError::unavailable(error.to_string())),
    };
    runtime
        .close_session(
            ctx,
            CloseSessionRequest {
                handle: view.handle,
                mode: CloseMode::RollbackAndClose,
            },
        )
        .await
        .map(|_| ())
        .map_err(|error| SyncExecutionError::unavailable(error.to_string()))
}

/// 专用会话释放的 best-effort 包装（失败只 warn，不改变调用方结果）。
pub(crate) async fn release_sessions(
    endpoints: &SessionEndpoints,
    ctx: &RequestContext,
    sessions: &[String],
) {
    for session in sessions {
        if close_dedicated_session(endpoints, ctx, session)
            .await
            .is_err()
        {
            tracing::warn!("Data Sync job-owned session cleanup was not confirmed");
        }
    }
}

/// Open dedicated job-owned sessions and prove they resolve to the same
/// physical endpoint identity as the user-authorized sessions（桌面
/// `own_sessions` 逐字对齐）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn own_sessions(
    endpoints: &SessionEndpoints,
    ctx: &RequestContext,
    source_session_id: &str,
    target_session_id: &str,
    source_database: &str,
    target_database: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
) -> Result<OwnedSessions, SyncExecutionError> {
    let source_identity = scoped_identity(
        endpoints,
        ctx,
        source_session_id,
        Some((source_database, source_schema)),
    )
    .await?;
    let target_identity = scoped_identity(
        endpoints,
        ctx,
        target_session_id,
        Some((target_database, target_schema)),
    )
    .await?;

    let source_id = open_dedicated_session(
        endpoints,
        ctx,
        source_identity.connection_id.as_str(),
        Some(source_database),
        "dataSyncSource",
    )
    .await
    .map_err(|_| {
        SyncExecutionError::Validation("could not open a dedicated Data Sync source session".into())
    })?;
    let target_id = match open_dedicated_session(
        endpoints,
        ctx,
        target_identity.connection_id.as_str(),
        Some(target_database),
        "dataSyncTarget",
    )
    .await
    {
        Ok(id) => id,
        Err(_) => {
            let _ = close_dedicated_session(endpoints, ctx, &source_id).await;
            return Err(SyncExecutionError::Validation(
                "could not open a dedicated Data Sync target session".into(),
            ));
        }
    };

    let pair = async {
        let source_check = scoped_identity(
            endpoints,
            ctx,
            &source_id,
            Some((source_database, source_schema)),
        )
        .await?;
        let target_check = scoped_identity(
            endpoints,
            ctx,
            &target_id,
            Some((target_database, target_schema)),
        )
        .await?;
        if source_check != source_identity || target_check != target_identity {
            return Err(SyncExecutionError::Validation(
                "dedicated Data Sync session does not match the reviewed physical endpoint".into(),
            ));
        }
        Ok((source_check, target_check))
    }
    .await;
    let (source_identity, target_identity) = match pair {
        Ok(pair) => pair,
        Err(error) => {
            let _ = close_dedicated_session(endpoints, ctx, &source_id).await;
            let _ = close_dedicated_session(endpoints, ctx, &target_id).await;
            return Err(error);
        }
    };

    let source_endpoint = endpoint_ref(
        &source_identity,
        source_database,
        EndpointRole::SourceReader,
    );
    let target_endpoint = endpoint_ref(
        &target_identity,
        target_database,
        EndpointRole::TargetWriter,
    );
    let source_target = recovery_target(&source_identity, source_database, source_schema);
    let target_target = recovery_target(&target_identity, target_database, target_schema);

    Ok(OwnedSessions {
        source_id,
        target_id,
        source_identity,
        target_identity,
        source_endpoint,
        target_endpoint,
        source_target,
        target_target,
    })
}

/// 受理上下文的组织作用域（组织来自认证链事实，INV-01）。
pub(crate) fn scope_of(ctx: &RequestContext) -> OrganizationScope {
    OrganizationScope {
        organization_id: ctx.organization_id.clone(),
    }
}
