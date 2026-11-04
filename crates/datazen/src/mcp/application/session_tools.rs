//! MCP 有状态工具（open_session / use_session / close_session）的业务用例
//! （M1 收官 + M2 主体归位）。
//!
//! 工具编排（admission、目标校验、TTL 归一化、归属盖章、SQL 守卫、提交与
//! 等待、产物读回、报告与关闭判定、未交付守卫）归本模块；`#[tool]` 宏、
//! Input 的 JsonSchema、连接白名单检查（`ensure_allowed`）与 rmcp 错误
//! 映射留形态绑定层。
//!
//! 形态侧资源走窄端口 [`StatefulSessionHost`]：runtime 租约解析、归属裁决
//! （形态的身份策略）与 cleanup worker 调度钩子。runtime 本身
//! （`RuntimeConnectionUseCases`）是共享类型，直接消费，不另立端口。
//!
//! 错误通道用中立的 [`SessionToolError`] 三分——这是有状态工具宿主映射的
//! 原样承载：SessionLost 类文案映射裸 invalid_params（不附帮助块），守卫/
//! 解析/身份类映射 invalid_params + 工具帮助块，运行时失败映射
//! internal_error。文本逐字保留宿主历史行为。

use crate::execution::application::idempotency::IdempotentOperation;
use crate::execution::application::requests::{
    CloseMode, CloseSessionRequest, CommandCall, ExecuteInSessionRequest, OpenSessionRequest,
};
use crate::execution::application::use_cases::ConnectionUseCases;
use crate::execution::application::RuntimeConnectionUseCases;
use crate::execution::domain::permission::{self, McpPermissionMode};
use crate::execution::target::{ExecutionTarget, NamespaceTarget, TargetNamespaceLayer};
use crate::execution::{ExecutionState, ExecutionView, OpenSessionReceipt, ResourceRelease};
use crate::mcp::application::session::{sanitize_ttl, McpSessionRegistry, SessionBinding};
use crate::shared::context::{OwnerRef, RequestContext};
use crate::shared::error::ApiError;
use crate::shared::id::{ArtifactId, ClientInstanceId, ConnectionId, Counter};
use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// MCP 客户端断连后的 SessionLost 文案（映射裸 invalid_params）。
pub const DISCONNECT_MESSAGE: &str = "SessionLost: MCP client disconnected";

/// `use_session` 等待终态的默认时长。
pub const DEFAULT_USE_TIMEOUT_MS: u64 = 60_000;
/// `use_session` 等待终态的时长上限。
pub const MAX_USE_TIMEOUT_MS: u64 = 300_000;
/// 单次 `use_session` 回复的行数上限。
pub const MCP_SESSION_ROW_LIMIT: usize = 200;

/// 有状态工具错误的三分通道（宿主原错误映射的显式承载）。
///
/// - [`SessionToolError::WithHelp`]：参数/校验类，宿主映射 `invalid_params`
///   并附工具帮助块（SQL 守卫、close mode 解析、身份裁决、空目标）。
/// - [`SessionToolError::Plain`]：参数类但裸映射 `invalid_params`，不附
///   帮助块（SessionLost 家族——历史 wire 行为如此）。
/// - [`SessionToolError::Internal`]：运行时失败，宿主映射 `internal_error`。
pub enum SessionToolError {
    WithHelp(String),
    Plain(String),
    Internal(String),
}

fn internal(error: String) -> SessionToolError {
    SessionToolError::Internal(error)
}

/// 归属盖章：一条有状态映射属于哪个后端生成的客户端实例、为何种用途
/// 存在。用途仅展示，从不作为授权输入（summary §5.1(3)）。
pub struct OwnerStamp {
    /// 后端生成的 MCP clientInstanceId（§4.1：不由客户端提供）。
    pub client_instance_id: String,
    /// owner 用途（形态常量，例如 `mcp-stateful-session`）。
    pub purpose: String,
}

impl OwnerStamp {
    fn owner(&self) -> OwnerRef {
        OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(self.client_instance_id.clone()),
            purpose: self.purpose.clone(),
        }
    }
}

/// 一次有状态调用所需的宿主解析产物：当前 runtime 租约与调用上下文。
pub struct StatefulCall {
    pub runtime: Arc<RuntimeConnectionUseCases>,
    pub context: RequestContext,
}

/// 有状态会话步骤的宿主端口。
///
/// 实现持有形态侧的适配器租约与身份装配（runtime 可能被吊销，必须每次
/// 调用现取，不得缓存）；归属裁决走形态的身份策略——MCP 层不自建
/// allow 规则。`ensure_cleanup_worker` 是调度钩子：登记变化后由形态
/// 启动/保活它自己的 cleanup worker。
#[async_trait::async_trait]
pub trait StatefulSessionHost: Send + Sync {
    /// 解析当前 runtime 租约与调用上下文；宿主不可用时返回 Err。
    async fn resolve_call(&self) -> Result<StatefulCall, String>;
    /// 形态身份策略裁决 owner（失败即拒绝本次有状态调用）。
    fn authorize_owner(&self, context: &RequestContext, owner: &OwnerRef) -> Result<(), String>;
    /// 调度钩子：确保该客户端的 cleanup worker 在运行。
    fn ensure_cleanup_worker(&self);
}

/// Admission（原宿主 `session_call`）：断连守卫、过期回收、worker 调度、
/// 资源解析与归属裁决。MCP 从不自带 ownership allow 规则。
async fn admit(
    host: &dyn StatefulSessionHost,
    registry: &McpSessionRegistry,
    stamp: &OwnerStamp,
) -> Result<(StatefulCall, OwnerRef), SessionToolError> {
    if registry.disconnected.load(Ordering::Acquire) {
        return Err(SessionToolError::Plain(DISCONNECT_MESSAGE.to_owned()));
    }
    registry.purge_expired(Instant::now());
    host.ensure_cleanup_worker();
    let call = host.resolve_call().await.map_err(internal)?;
    let owner = stamp.owner();
    host.authorize_owner(&call.context, &owner)
        .map_err(SessionToolError::WithHelp)?;
    Ok((call, owner))
}

/// `open_session`：在**显式**目标上开会话并交回不透明引用（summary §4.2）。
/// GUI 活动连接、标签页或数据库一概不参与。
///
/// 空目标是目标错误而不是回退到 GUI（§4.1）。
pub async fn open_session(
    host: Arc<dyn StatefulSessionHost>,
    registry: Arc<McpSessionRegistry>,
    stamp: &OwnerStamp,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
    ttl_ms: Option<u64>,
) -> Result<String, SessionToolError> {
    let namespace = namespace_target(database, schema);
    // §4.1: an empty explicit target is a target error, not a fallback to the GUI.
    if !namespace.has_value(TargetNamespaceLayer::Database) {
        return Err(SessionToolError::WithHelp(
            "open_session needs an explicit target: pass database (and schema when the driver has one). The active interactive connection is never used as a fallback."
                .to_owned(),
        ));
    }
    let ttl = sanitize_ttl(ttl_ms.map(Duration::from_millis));

    let (call, owner) = admit(&*host, &registry, stamp).await?;
    let target = ExecutionTarget::new(ConnectionId::new(connection_id.to_owned()), namespace);
    let token = call
        .runtime
        .issue_submission_token(&call.context, IdempotentOperation::OpenSession, None)
        .await
        .map_err(|error| internal(error.to_string()))?;

    let request = OpenSessionRequest::new(target, owner, token.idempotency_key);
    complete_session_open(
        host,
        registry,
        stamp.client_instance_id.clone(),
        connection_id.to_owned(),
        ttl,
        async move { call.runtime.open_session(&call.context, request).await },
    )
    .await
}

/// 开_session 的完成语义：离题接管 acquisition（请求被取消时，晚到的
/// 资源由 [`UndeliveredSession`] 守卫撤销而非泄漏）、reference 铸造与
/// 归属盖章注册、唤醒 TTL worker、断连拒收未交付的回复。
///
/// `acquisition` 由调用方装配（通常是一次 runtime `open_session` 往返）；
/// 它在分离的任务里跑完——丢弃其输出即触发守卫撤销。
pub async fn complete_session_open(
    host: Arc<dyn StatefulSessionHost>,
    registry: Arc<McpSessionRegistry>,
    client_instance_id: String,
    connection_id: String,
    ttl: Duration,
    acquisition: impl Future<Output = Result<OpenSessionReceipt, ApiError>> + Send + 'static,
) -> Result<String, SessionToolError> {
    // Dropping the request's JoinHandle detaches rather than cancels this
    // acquisition. Its output guard then owns an unconsumed receipt.
    let task = tokio::spawn(async move {
        let receipt = acquisition
            .await
            .map_err(|error| internal(error.to_string()))?;
        let binding = registry.open(
            &client_instance_id,
            &connection_id,
            receipt.session.handle,
            receipt.session.context_revision,
            ttl,
            Instant::now(),
        );
        registry.wake.notify_one();
        host.ensure_cleanup_worker();
        let reply = serde_json::json!({
            "clientSessionRef": binding.reference,
            "state": receipt.session.state,
            "contextRevision": receipt.session.context_revision,
            "expiresInMs": ttl.as_millis().min(u128::from(u64::MAX)) as u64,
        })
        .to_string();
        let owned = UndeliveredSession {
            registry,
            host,
            client_instance_id,
            reference: Some(binding.reference),
            reply,
        };
        if owned
            .registry
            .disconnected
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(SessionToolError::Plain(DISCONNECT_MESSAGE.to_owned()));
        }
        Ok(owned)
    });
    let owned = task
        .await
        .map_err(|_| internal("MCP session acquisition task failed".to_owned()))??;
    Ok(owned.deliver())
}

/// 未交付守卫：请求方尚未消费回复时，它持有这条新登记；守卫被丢弃
/// （请求取消、任务输出无人接管）即撤销晚到 acquired 的资源并触发清理。
struct UndeliveredSession {
    registry: Arc<McpSessionRegistry>,
    host: Arc<dyn StatefulSessionHost>,
    client_instance_id: String,
    reference: Option<String>,
    reply: String,
}

impl UndeliveredSession {
    fn deliver(mut self) -> String {
        self.reference = None;
        std::mem::take(&mut self.reply)
    }
}

impl Drop for UndeliveredSession {
    fn drop(&mut self) {
        if let Some(reference) = self.reference.take() {
            self.registry.revoke(&reference, &self.client_instance_id);
            self.host.ensure_cleanup_worker();
        }
    }
}

/// `use_session`：在一条本客户端持有的映射里跑一个命令。
pub async fn use_session(
    host: &dyn StatefulSessionHost,
    registry: &McpSessionRegistry,
    permission_mode: McpPermissionMode,
    stamp: &OwnerStamp,
    reference: &str,
    command: &str,
    input: serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<String, SessionToolError> {
    let binding = registry
        .lookup(reference, &stamp.client_instance_id, Instant::now())
        .map_err(|reason| SessionToolError::Plain(reason.client_message().to_owned()))?;

    // §4.1: stdio mode runs the same SQL guard as the GUI, not a bypass.
    guard_session_sql(permission_mode, command, &input)?;

    let (call, _owner) = admit(host, registry, stamp).await?;
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DEFAULT_USE_TIMEOUT_MS)
            .min(MAX_USE_TIMEOUT_MS),
    );
    let receipt = dispatch_session_command(
        &call,
        registry,
        &stamp.client_instance_id,
        &binding,
        command,
        input,
        timeout,
    )
    .await?;

    let mut report = serde_json::json!({
        "executionId": receipt.execution_id,
        "state": receipt.state,
        "effectOutcome": receipt.effect_outcome,
        "resultCompleteness": receipt.result_completeness,
        "artifactIds": receipt.artifact_ids,
        "truncationReason": receipt.truncation_reason,
        "errorCode": receipt.error_code,
    });
    if receipt.state == ExecutionState::Succeeded {
        let (columns, rows, truncated) = read_rows(&call, &receipt.artifact_ids).await?;
        report["columns"] = columns;
        report["rows"] = serde_json::json!(rows);
        report["rowsTruncated"] = serde_json::json!(truncated);
    }
    Ok(report.to_string())
}

/// Run the SQL guard on the SQL a stateful command carries. The command name
/// is the contract, not the payload: `query` and `execute` carry SQL under
/// different keys depending on the driver, so only a payload that is plainly a
/// string is guarded and anything else is left to the driver command itself.
fn guard_session_sql(
    permission_mode: McpPermissionMode,
    command: &str,
    input: &serde_json::Value,
) -> Result<(), SessionToolError> {
    if command != "query" && command != "execute" {
        return Ok(());
    }
    let Some(sql) = input.get("sql").and_then(serde_json::Value::as_str) else {
        return Ok(());
    };
    permission::check_sql_allowed(sql, permission_mode).map_err(SessionToolError::WithHelp)
}

/// Submit one command into an already owned session and wait for a terminal
/// state. The wait is bounded: a client that stops waiting gets its
/// execution id back and can ask again, instead of holding the call open.
async fn dispatch_session_command(
    call: &StatefulCall,
    registry: &McpSessionRegistry,
    client_instance_id: &str,
    binding: &SessionBinding,
    command: &str,
    input: serde_json::Value,
    timeout: Duration,
) -> Result<ExecutionView, SessionToolError> {
    let session = call
        .runtime
        .get_session(&call.context, binding.handle.clone())
        .await
        .map_err(|error| internal(error.to_string()))?;
    // CAS against the revision recorded when the mapping was opened: a context
    // switch made through another path is a conflict, not something to overwrite.
    let expected = match session.context_revision == binding.context_revision {
        true => session.context_revision,
        false => binding.context_revision,
    };
    let token = call
        .runtime
        .issue_submission_token(
            &call.context,
            IdempotentOperation::ExecuteInSession,
            Some(&binding.handle),
        )
        .await
        .map_err(|error| internal(error.to_string()))?;
    // get_session/token issuance await the Runtime. Recheck the mapping lease
    // immediately before submission so TTL/disconnect cannot lend a stale clone
    // access after it was revoked while those calls were in flight.
    registry
        .lookup(&binding.reference, client_instance_id, Instant::now())
        .map_err(|reason| SessionToolError::Plain(reason.client_message().to_owned()))?;
    if registry.disconnected.load(Ordering::Acquire) {
        return Err(SessionToolError::Plain(DISCONNECT_MESSAGE.to_owned()));
    }
    let receipt = call
        .runtime
        .execute_in_session(
            &call.context,
            ExecuteInSessionRequest::new(
                binding.handle.clone(),
                CommandCall::new(command, input),
                token.idempotency_key,
            )
            .with_expected_context_revision(expected),
        )
        .await
        .map_err(|error| internal(error.to_string()))?;

    let deadline = Instant::now() + timeout;
    loop {
        let view = call
            .runtime
            .get_execution(&call.context, receipt.execution_id.clone())
            .await
            .map_err(|error| internal(error.to_string()))?;
        if view.state.is_terminal() || Instant::now() >= deadline {
            return Ok(view);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Read the result rows back out of the artifacts the execution published.
/// `ExecutionView` carries no inline rows by design, so the MCP surface has
/// to fetch chunks itself; the row cap keeps one call from flooding a model
/// context, and the caller is told the payload was cut.
async fn read_rows(
    call: &StatefulCall,
    artifact_ids: &[ArtifactId],
) -> Result<(serde_json::Value, Vec<serde_json::Value>, bool), SessionToolError> {
    let mut columns = serde_json::Value::Null;
    let mut rows: Vec<serde_json::Value> = Vec::new();
    let mut truncated = false;
    for artifact in artifact_ids {
        let first = call
            .runtime
            .read_artifact_chunk(&call.context, artifact.clone(), Counter::ZERO)
            .await
            .map_err(|error| internal(error.to_string()))?;
        if columns.is_null() {
            columns = first.columns.clone();
        }
        for index in 0..first.published_chunk_count.get() {
            let chunk = if index == 0 {
                first.clone()
            } else {
                call.runtime
                    .read_artifact_chunk(&call.context, artifact.clone(), Counter::new(index))
                    .await
                    .map_err(|error| internal(error.to_string()))?
            };
            for row in chunk.rows {
                if rows.len() >= MCP_SESSION_ROW_LIMIT {
                    truncated = true;
                    return Ok((columns, rows, truncated));
                }
                rows.push(row);
            }
        }
    }
    Ok((columns, rows, truncated))
}

/// `close_session`：释放一条本客户端持有的映射。
///
/// 未知、已关闭与过期引用一律报「没有可关的东西」，过期引用不泄漏它
/// 是否曾经存在（§4.2）；关闭失败保留映射以便客户端重试——在这里丢弃
/// 它会泄漏数据库会话且再无关闭途径。
pub async fn close_session(
    host: &dyn StatefulSessionHost,
    registry: &McpSessionRegistry,
    stamp: &OwnerStamp,
    reference: &str,
    mode: Option<&str>,
) -> Result<String, SessionToolError> {
    // Idempotent: a reference this client does not hold is reported as already
    // closed, without distinguishing unknown from foreign (§4.2).
    let Ok(binding) = registry.lookup(reference, &stamp.client_instance_id, Instant::now()) else {
        return Ok(serde_json::json!({ "closed": false }).to_string());
    };

    let mode = close_mode(mode).map_err(SessionToolError::WithHelp)?;
    let (call, _owner) = admit(host, registry, stamp).await?;
    let receipt = call
        .runtime
        .close_session(
            &call.context,
            CloseSessionRequest::new(binding.handle.clone(), mode),
        )
        .await
        // The mapping is kept on failure so the client can retry; dropping it
        // here would leak the database session with no way left to close it.
        .map_err(|error| internal(error.to_string()))?;
    if receipt.resource_release != ResourceRelease::Confirmed {
        registry.revoke(reference, &stamp.client_instance_id);
        host.ensure_cleanup_worker();
        return Ok(serde_json::json!({ "closed": false, "cleanupPending": true }).to_string());
    }
    registry.close(reference, &stamp.client_instance_id);
    Ok(serde_json::json!({ "closed": true }).to_string())
}

/// Explicit namespace for an opened session. An empty value is meaningful: it
/// says the client did not name a database, which is an error for a stateful
/// session rather than a licence to fall back to the GUI target.
fn namespace_target(database: Option<String>, schema: Option<String>) -> NamespaceTarget {
    NamespaceTarget::new(
        database.filter(|value| !value.trim().is_empty()),
        None,
        schema.filter(|value| !value.trim().is_empty()),
        Vec::new(),
    )
}

/// Parse the close mode. Unknown strings are rejected instead of defaulting:
/// a typo must not silently downgrade to "discard uncommitted work".
fn close_mode(requested: Option<&str>) -> Result<CloseMode, String> {
    match requested.unwrap_or("requireNoTransaction") {
        "requireNoTransaction" => Ok(CloseMode::RequireNoTransaction),
        "rollbackAndClose" => Ok(CloseMode::RollbackAndClose),
        other => Err(format!(
            "{other} is not a close mode; use requireNoTransaction or rollbackAndClose"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unnamed_database_stays_unnamed_in_the_session_target() {
        let namespace = namespace_target(None, None);
        assert!(
            !namespace.has_value(TargetNamespaceLayer::Database),
            "a session target must never fall back to the active interactive database"
        );
        assert!(!namespace.has_value(TargetNamespaceLayer::Schema));
    }

    #[test]
    fn a_blank_database_is_dropped_instead_of_becoming_an_empty_namespace_layer() {
        let namespace = namespace_target(Some("   ".to_owned()), Some("public".to_owned()));
        assert!(!namespace.has_value(TargetNamespaceLayer::Database));
        assert!(
            namespace.has_value(TargetNamespaceLayer::Schema),
            "the named schema still reaches the target"
        );
    }

    #[test]
    fn the_named_database_and_schema_reach_the_session_target() {
        let namespace = namespace_target(Some("app".to_owned()), Some("billing".to_owned()));
        assert!(namespace.has_value(TargetNamespaceLayer::Database));
        assert!(namespace.has_value(TargetNamespaceLayer::Schema));
    }

    #[test]
    fn closing_without_a_mode_refuses_to_discard_uncommitted_work() {
        assert!(matches!(
            close_mode(None),
            Ok(CloseMode::RequireNoTransaction)
        ));
        assert!(matches!(
            close_mode(Some("requireNoTransaction")),
            Ok(CloseMode::RequireNoTransaction)
        ));
    }

    #[test]
    fn the_client_can_ask_for_a_rollback_and_close() {
        assert!(matches!(
            close_mode(Some("rollbackAndClose")),
            Ok(CloseMode::RollbackAndClose)
        ));
    }

    #[test]
    fn an_unknown_close_mode_is_rejected_rather_than_defaulted() {
        let reason = close_mode(Some("force"))
            .expect_err("an unknown close mode must not be silently mapped");
        assert!(
            reason.contains("force"),
            "the rejection must name what the client asked for, got: {reason}"
        );
    }

    #[test]
    fn the_owner_stamp_carries_the_backend_client_instance_and_purpose() {
        let stamp = OwnerStamp {
            client_instance_id: "mcp-a".to_owned(),
            purpose: "mcp-stateful-session".to_owned(),
        };
        assert_eq!(
            stamp.owner(),
            OwnerRef::ClientSession {
                client_instance_id: ClientInstanceId::new("mcp-a".to_owned()),
                purpose: "mcp-stateful-session".to_owned(),
            },
            "the owner is stamped with this server's own backend-generated id"
        );
    }

    #[test]
    fn a_command_that_is_not_sql_carrying_skips_the_guard() {
        assert!(guard_session_sql(
            McpPermissionMode::default(),
            "list_tables",
            &serde_json::json!({})
        )
        .is_ok());
        // 非 string 载荷留给驱动命令本身，守卫不猜键名。
        assert!(guard_session_sql(
            McpPermissionMode::default(),
            "execute",
            &serde_json::json!({"statements": []})
        )
        .is_ok());
    }
}
