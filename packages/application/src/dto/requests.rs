//! 连接用例的**请求 DTO** 与其纯字段校验（连接 §4）。
//!
//! 这些形状在 TypeScript 契约里是 `OpenSessionRequest` / `ExecuteInSessionRequest` 等；本模块
//! 是 Rust 侧的逐字段镜像，外加**入参校验**——校验发生在任何端口调用之前，因此非法请求
//! 不会打到 driver，也不会消耗预算。
//!
//! ## 为什么请求体里没有组织/主体/归属的自由文本字段
//!
//! 概要 §5.1 的硬约束一：身份**只能**由 adapter 构造的 `RequestContext` 提供，入口不得
//! 从请求体读取 `organizationId` / `principalId`。所以本模块每个请求结构体都满足
//! 「不含身份字段」，用用例签名上的 `&RequestContext` 承载身份（见 [`crate::sessions`]）。
//! `owner` 是 `OwnerRef`（枚举），不是自由文本：它表达**归属**，并且必须由 adapter 校验
//! 「当前 client 确实是该 editor 的 owner」「该 job 确实属于已授权的 Job」（概要 §5.1 第三条），
//! 校验逻辑在 [`crate::identity_policy`]。
//!
//! ## 字段校验的边界
//!
//! * 字段「存在」：Rust 结构体没有 undefined，因此连接 §4 的「所有 `NamespaceTarget` 字段必传」
//!   在 Rust 侧退化为「字段不可为空字符串」，见 [`validate_namespace_target`]。
//! * 目标规范化（别名合并、driver 规范化、操作级要求）不在本模块，见 [`crate::target`]。

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::session::{SessionHandle, SessionState};
use datazen_platform_api::id::{
    AttachmentToken, ConnectionId, Counter, IdempotencyKey, RuntimeEpoch,
};
use datazen_platform_api::target::{ExecutionTarget, NamespaceTarget};

use crate::error::{ApiError, ApiErrorCode};

/// 校验命名空间目标：非空字符串、空 path 段一律拒绝（连接 §4）。
///
/// 连接 §4 的约束逐条对应：
///
/// * 「所有 `NamespaceTarget` 字段必传，空字符串非法」→ 四个字段值要么是 `Some(非空)`，
///   要么是 `None`；不允许 `Some("")`。
/// * 「`path` 保存 driver 命名空间 ID，不是文件路径」→ 这里只校验段非空，不解释语义。
/// * 对象操作必须给完整身份 → 由 [`NamespaceTarget`] 里各层的实际取值 + 操作级
///   `targetRequirements` 保证，本函数不猜默认值。
pub fn validate_namespace_target(target: &NamespaceTarget) -> Result<(), ApiError> {
    for (layer, value) in [
        (
            datazen_platform_api::target::NamespaceLayer::Database,
            target.database.as_ref(),
        ),
        (
            datazen_platform_api::target::NamespaceLayer::Catalog,
            target.catalog.as_ref(),
        ),
        (
            datazen_platform_api::target::NamespaceLayer::Schema,
            target.schema.as_ref(),
        ),
    ] {
        if let Some(value) = value {
            if value.is_empty() {
                return Err(ApiError::invalid_argument(format!(
                    "namespace field must not be an empty string: {layer:?}"
                )));
            }
        }
    }
    if let Some(index) = target.path.iter().position(|segment| segment.is_empty()) {
        return Err(ApiError::invalid_argument(format!(
            "namespace path segment {index} must not be empty"
        )));
    }
    Ok(())
}

/// 校验 driver Command 调用（连接 §4 的 `CommandCall`）。
///
/// `input` 是透传的 `unknown`：命令级参数校验属于 driver Command schema（§4.2 CommandCall），
/// 应用层**不**重新实现一套通用 JSON Schema 校验器。
pub fn validate_command_call(call: &CommandCall) -> Result<(), ApiError> {
    if call.command.is_empty() {
        return Err(ApiError::invalid_argument("command must not be empty"));
    }
    Ok(())
}

/// 校验 `connectionId`：持久化连接配置 id，非空（概要 §6.2）。
pub fn validate_connection_id(connection_id: &ConnectionId) -> Result<(), ApiError> {
    if connection_id.is_empty() {
        return Err(ApiError::invalid_argument("connectionId must not be empty"));
    }
    Ok(())
}

/// 校验会话句柄：`dbSessionId` 与 `runtimeEpoch` 缺一不可（连接 §6.3：客户端句柄两个部分
/// 必须完全匹配，runtimeEpoch 用于拒绝旧句柄）。
pub fn validate_session_handle(handle: &SessionHandle) -> Result<(), ApiError> {
    if handle.db_session_id.is_empty() {
        return Err(ApiError::invalid_argument("dbSessionId must not be empty"));
    }
    if handle.runtime_epoch.is_empty() {
        return Err(ApiError::invalid_argument("runtimeEpoch must not be empty"));
    }
    Ok(())
}

/// 校验幂等键：非空。键内容本身由 `SubmissionTokenIssuer` 签发与核验（连接 §13.1），
/// 请求体只透传 opaque 值（`Debug` 已脱敏）。
pub fn validate_idempotency_key(key: &IdempotencyKey) -> Result<(), ApiError> {
    if key.is_empty() {
        return Err(ApiError::invalid_argument(
            "idempotencyKey must not be empty",
        ));
    }
    Ok(())
}

/// 校验 `CommandCall` + 驱动 Command 的物理上限（连接 §13：`PayloadTooLarge`）。
///
/// 上限由宿主注入（不同形态的物理上限不同），因此是参数而不是常量。
pub fn validate_payload_size(
    input: &serde_json::Value,
    limit_bytes: usize,
) -> Result<(), ApiError> {
    let encoded = serde_json::to_vec(input)
        .map_err(|_| ApiError::invalid_argument("command input is not serializable"))?;
    if encoded.len() > limit_bytes {
        return Err(ApiError::new(
            ApiErrorCode::PayloadTooLarge,
            "command input exceeds the physical limit",
        ));
    }
    Ok(())
}

/// 统一网关 Command 调用（连接 §4 的 `CommandCall`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCall {
    /// 逻辑 Command 名（如 `query` / `execute` / `describe`），不是物理命令字符串。
    pub command: String,
    /// 透传的命令参数，由 driver Command schema 解释。
    pub input: serde_json::Value,
}

impl CommandCall {
    /// 构造一次 Command 调用。
    pub fn new(command: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            command: command.into(),
            input,
        }
    }

    /// 纯字段校验（见 [`validate_command_call`]）。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_command_call(self)
    }
}

/// 打开会话请求（连接 §4）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    /// 显式命名空间目标：连接 §4 禁止把空库空 schema 当默认值。
    pub initial_target: ExecutionTarget,
    /// 归属（枚举，不是自由文本）；可采信性由 adapter 用例校验。
    pub owner: OwnerRef,
    /// 幂等键，重试复用同一键。
    pub idempotency_key: IdempotencyKey,
}

impl OpenSessionRequest {
    /// 构造打开会话请求。
    pub fn new(
        initial_target: ExecutionTarget,
        owner: OwnerRef,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            initial_target,
            owner,
            idempotency_key,
        }
    }

    /// 纯字段校验：connectionId / 命名空间 / 幂等键；`owner` 的可采信性见
    /// [`crate::identity_policy::check_owner`]。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_connection_id(&self.initial_target.connection_id)?;
        validate_namespace_target(&self.initial_target.namespace)?;
        validate_idempotency_key(&self.idempotency_key)
    }
}

/// 在既有会话内执行（连接 §4）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteInSessionRequest {
    /// 会话句柄，必须与注册记录完全匹配（`dbSessionId` + `runtimeEpoch`）。
    pub handle: SessionHandle,
    /// 可选上下文版本。普通编辑器执行应带上；显式 session 脚本内部按队列自然延续，
    /// 可不传（连接 §4）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_context_revision: Option<Counter>,
    /// Command 调用。
    pub call: CommandCall,
    /// 幂等键，重试复用同一键。
    pub idempotency_key: IdempotencyKey,
}

impl ExecuteInSessionRequest {
    /// 构造会话内执行请求。
    pub fn new(handle: SessionHandle, call: CommandCall, idempotency_key: IdempotencyKey) -> Self {
        Self {
            handle,
            expected_context_revision: None,
            call,
            idempotency_key,
        }
    }

    /// 带上期望的上下文版本。
    pub fn with_expected_context_revision(mut self, revision: Counter) -> Self {
        self.expected_context_revision = Some(revision);
        self
    }

    /// 纯字段校验。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_session_handle(&self.handle)?;
        validate_command_call(&self.call)?;
        validate_idempotency_key(&self.idempotency_key)
    }
}

/// 无会话一次性执行（连接 §4）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteAtTargetRequest {
    /// 显式目标，缺层即 `targetRequired`，绝不回退默认库（连接 §4.3）。
    pub target: ExecutionTarget,
    /// 必需的期望配置版本（CAS）：profile 已变则拒绝，不静默在新配置上执行。
    pub expected_config_revision: Counter,
    /// Command 调用。
    pub call: CommandCall,
    /// 幂等键，重试复用同一键。
    pub idempotency_key: IdempotencyKey,
}

impl ExecuteAtTargetRequest {
    /// 构造一次性执行请求。
    pub fn new(
        target: ExecutionTarget,
        expected_config_revision: Counter,
        call: CommandCall,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            target,
            expected_config_revision,
            call,
            idempotency_key,
        }
    }

    /// 纯字段校验。`expected_config_revision` 的 CAS 语义由 `ProfileRepository` 端口执行，
    /// 不匹配返回 `ApiErrorCode::ConfigRevisionMismatch`（连接 §13）。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_connection_id(&self.target.connection_id)?;
        validate_namespace_target(&self.target.namespace)?;
        validate_command_call(&self.call)?;
        validate_idempotency_key(&self.idempotency_key)
    }
}

/// 切换会话上下文请求（连接 §4）。
///
/// 语义：**两阶段候选 → 服务端提交后的替换**。客户端表达的是期望目标，不是自认为的最终状态；
/// 服务端核对 `expectedContextRevision`，成功后回 `ContextChangeReceipt`
/// （连接 §7.5：不静默切换会话、不改写客户端记录、切换后先发瞬时上下文再发终态）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionContextRequest {
    /// 会话句柄。
    pub handle: SessionHandle,
    /// 必需：缺失或与当前不一致返回 `contextConflict`，客户端重读后由用户重新发送
    /// （连接 §6.3）。
    pub expected_context_revision: Counter,
    /// 期望的新命名空间目标。
    pub desired: NamespaceTarget,
    /// 幂等键，重试复用同一键。
    pub idempotency_key: IdempotencyKey,
}

impl SetSessionContextRequest {
    /// 构造上下文切换请求。
    pub fn new(
        handle: SessionHandle,
        expected_context_revision: Counter,
        desired: NamespaceTarget,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            handle,
            expected_context_revision,
            desired,
            idempotency_key,
        }
    }

    /// 纯字段校验。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_session_handle(&self.handle)?;
        validate_namespace_target(&self.desired)?;
        validate_idempotency_key(&self.idempotency_key)
    }
}

/// 关闭模式（连接 §4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseMode {
    /// 仅在无活动事务时关闭。
    RequireNoTransaction,
    /// 回滚未完成事务后关闭。
    RollbackAndClose,
}

/// 关闭会话请求（连接 §4）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionRequest {
    /// 会话句柄。
    pub handle: SessionHandle,
    /// 关闭模式。
    pub mode: CloseMode,
}

impl CloseSessionRequest {
    /// 构造关闭会话请求。
    pub fn new(handle: SessionHandle, mode: CloseMode) -> Self {
        Self { handle, mode }
    }

    /// 纯字段校验。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_session_handle(&self.handle)
    }
}

/// 重新附着请求（连接 §4）。
///
/// 浏览器/前端刷新后用 `attachmentToken` 换回原 `dbSessionId`；令牌只发给原始 owner
/// （连接 §4.4、§7.5），因此它是 opaque 类型且 `Debug` 脱敏。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRequest {
    /// 会话句柄。
    pub handle: SessionHandle,
    /// 附着令牌。
    pub attachment_token: AttachmentToken,
}

impl AttachmentRequest {
    /// 构造附着请求。
    pub fn new(handle: SessionHandle, attachment_token: AttachmentToken) -> Self {
        Self {
            handle,
            attachment_token,
        }
    }

    /// 纯字段校验。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_session_handle(&self.handle)?;
        if self.attachment_token.is_empty() {
            return Err(ApiError::invalid_argument(
                "attachmentToken must not be empty",
            ));
        }
        Ok(())
    }
}

/// 旧运行时句柄的显式判定（连接 §6.3：`runtimeEpoch` 用于拒绝旧句柄）。
///
/// epoch 不一致返回 `runtimeEpochMismatch`，让调用方重开会话而不是静默复用旧物理连接。
pub fn check_handle_epoch(
    handle: &SessionHandle,
    current_epoch: &RuntimeEpoch,
) -> Result<(), ApiError> {
    validate_session_handle(handle)?;
    if &handle.runtime_epoch != current_epoch {
        return Err(ApiError::new(
            ApiErrorCode::RuntimeEpochMismatch,
            "session handle belongs to an older runtime epoch",
        ));
    }
    Ok(())
}

/// 会话终态检查（连接 §7.6）：`closed` / `lost` 的会话不再接受执行。
pub fn ensure_session_executable(state: SessionState) -> Result<(), ApiError> {
    match state {
        SessionState::Closed => Err(ApiError::new(
            ApiErrorCode::SessionLost,
            "session is closed",
        )),
        SessionState::Lost => Err(ApiError::new(
            ApiErrorCode::SessionLost,
            "physical session was lost; open a new session explicitly",
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::context::{OwnerRef, RequestContext};
    use datazen_platform_api::id::{
        AuthenticationSessionId, ClientInstanceId, DbSessionId, DelegationId, EditorSessionId,
        OrganizationId, PrincipalId, RequestId,
    };
    use datazen_platform_api::target::NamespaceLayer;
    use std::collections::BTreeMap;

    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn namespace() -> NamespaceTarget {
        NamespaceTarget::new(Some("app".into()), None, Some("public".into()), Vec::new())
    }

    fn target() -> ExecutionTarget {
        ExecutionTarget::new(ConnectionId::new("conn-1"), namespace())
    }

    fn editor_owner() -> OwnerRef {
        OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("editor-1"),
        }
    }

    fn handle() -> SessionHandle {
        SessionHandle::new(DbSessionId::new("sess-1"), RuntimeEpoch::new("epoch-1"))
    }

    fn call() -> CommandCall {
        CommandCall::new("query", serde_json::json!({"sql": "select 1"}))
    }

    #[test]
    fn namespace_validation_rejects_empty_strings() {
        let mut target = NamespaceTarget::default();
        target.set(NamespaceLayer::Database, Some(String::new()));
        let error = validate_namespace_target(&target).expect_err("empty database");
        assert_eq!(error.code, ApiErrorCode::InvalidArgument);

        let mut path_target = NamespaceTarget::default();
        path_target.path = vec!["ok".into(), String::new()];
        let error = validate_namespace_target(&path_target).expect_err("empty path segment");
        assert_eq!(error.code, ApiErrorCode::InvalidArgument);
    }

    #[test]
    fn namespace_validation_accepts_absent_layers() {
        assert!(validate_namespace_target(&NamespaceTarget::default()).is_ok());
        assert!(validate_namespace_target(&namespace()).is_ok());
    }

    #[test]
    fn handle_validation_requires_both_parts() {
        assert!(validate_session_handle(&handle()).is_ok());
        let empty_id = SessionHandle::new(DbSessionId::new(""), RuntimeEpoch::new("epoch-1"));
        assert!(validate_session_handle(&empty_id).is_err());
        let empty_epoch = SessionHandle::new(DbSessionId::new("sess-1"), RuntimeEpoch::new(""));
        assert!(validate_session_handle(&empty_epoch).is_err());
    }

    #[test]
    fn stale_epoch_is_reported_as_runtime_epoch_mismatch() {
        let current = RuntimeEpoch::new("epoch-2");
        let error = check_handle_epoch(&handle(), &current).expect_err("stale handle");
        assert_eq!(error.code, ApiErrorCode::RuntimeEpochMismatch);
        assert_eq!(
            error.retry_disposition,
            crate::error::RetryDisposition::Never
        );
    }

    #[test]
    fn closed_and_lost_sessions_are_not_executable() {
        assert!(ensure_session_executable(SessionState::Ready).is_ok());
        assert!(ensure_session_executable(SessionState::Executing).is_ok());
        for state in [SessionState::Closed, SessionState::Lost] {
            let error = ensure_session_executable(state).expect_err("not executable");
            assert_eq!(error.code, ApiErrorCode::SessionLost);
        }
    }

    #[test]
    fn open_session_request_validates_target_and_key() {
        let request =
            OpenSessionRequest::new(target(), editor_owner(), IdempotencyKey::new("idem-1"));
        assert!(request.validate().is_ok());

        let mut bad_target = target();
        bad_target.namespace.schema = Some(String::new());
        let request =
            OpenSessionRequest::new(bad_target, editor_owner(), IdempotencyKey::new("idem-1"));
        assert_eq!(
            request.validate().expect_err("empty schema").code,
            ApiErrorCode::InvalidArgument
        );

        let request = OpenSessionRequest::new(target(), editor_owner(), IdempotencyKey::new(""));
        assert!(request.validate().is_err());
    }

    #[test]
    fn execute_requests_validate_their_calls_and_handles() {
        let in_session =
            ExecuteInSessionRequest::new(handle(), call(), IdempotencyKey::new("idem-2"));
        assert!(in_session.validate().is_ok());
        let with_revision = in_session
            .clone()
            .with_expected_context_revision(Counter::new(7));
        assert_eq!(
            with_revision.expected_context_revision,
            Some(Counter::new(7))
        );
        assert!(with_revision.validate().is_ok());

        let empty_command = ExecuteInSessionRequest::new(
            handle(),
            CommandCall::new("", serde_json::Value::Null),
            IdempotencyKey::new("idem-2"),
        );
        assert!(empty_command.validate().is_err());

        let at_target = ExecuteAtTargetRequest::new(
            target(),
            Counter::new(3),
            call(),
            IdempotencyKey::new("idem-3"),
        );
        assert!(at_target.validate().is_ok());
    }

    #[test]
    fn set_context_request_requires_a_revision_and_valid_namespace() {
        let request = SetSessionContextRequest::new(
            handle(),
            Counter::new(2),
            namespace(),
            IdempotencyKey::new("idem-4"),
        );
        assert!(request.validate().is_ok());

        let mut broken = namespace();
        broken.catalog = Some(String::new());
        let request = SetSessionContextRequest::new(
            handle(),
            Counter::new(2),
            broken,
            IdempotencyKey::new("idem-4"),
        );
        assert!(request.validate().is_err());
    }

    #[test]
    fn close_and_attachment_requests_validate() {
        assert!(
            CloseSessionRequest::new(handle(), CloseMode::RequireNoTransaction)
                .validate()
                .is_ok()
        );
        assert!(
            CloseSessionRequest::new(handle(), CloseMode::RollbackAndClose)
                .validate()
                .is_ok()
        );
        assert!(
            AttachmentRequest::new(handle(), AttachmentToken::new("token-1"))
                .validate()
                .is_ok()
        );
        assert!(AttachmentRequest::new(handle(), AttachmentToken::new(""))
            .validate()
            .is_err());
    }

    #[test]
    fn payload_limit_is_reported_as_payload_too_large() {
        let big = serde_json::json!({"sql": "x".repeat(64)});
        assert!(validate_payload_size(&big, 1024).is_ok());
        let error = validate_payload_size(&big, 8).expect_err("over limit");
        assert_eq!(error.code, ApiErrorCode::PayloadTooLarge);
    }

    #[test]
    fn requests_round_trip_through_serde() {
        let context = RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("principal-1"),
            Some(AuthenticationSessionId::new("auth-1")),
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        );
        assert!(!context.is_delegated());
        assert_eq!(context.delegation_id, None);

        let requests = vec![
            serde_json::to_value(OpenSessionRequest::new(
                target(),
                editor_owner(),
                IdempotencyKey::new("idem-1"),
            ))
            .expect("openSession"),
            serde_json::to_value(ExecuteInSessionRequest::new(
                handle(),
                call(),
                IdempotencyKey::new("idem-2"),
            ))
            .expect("executeInSession"),
            serde_json::to_value(ExecuteAtTargetRequest::new(
                target(),
                Counter::new(3),
                call(),
                IdempotencyKey::new("idem-3"),
            ))
            .expect("executeAtTarget"),
            serde_json::to_value(SetSessionContextRequest::new(
                handle(),
                Counter::new(2),
                namespace(),
                IdempotencyKey::new("idem-4"),
            ))
            .expect("setSessionContext"),
            serde_json::to_value(CloseSessionRequest::new(
                handle(),
                CloseMode::RollbackAndClose,
            ))
            .expect("closeSession"),
            serde_json::to_value(AttachmentRequest::new(
                handle(),
                AttachmentToken::new("token-1"),
            ))
            .expect("attach"),
        ];
        assert_eq!(requests[0]["initialTarget"]["connectionId"], "conn-1");
        assert_eq!(requests[0]["idempotencyKey"], "idem-1");
        assert_eq!(requests[1]["handle"]["dbSessionId"], "sess-1");
        assert_eq!(requests[1]["handle"]["runtimeEpoch"], "epoch-1");
        assert_eq!(requests[1]["call"]["command"], "query");
        assert_eq!(requests[2]["expectedConfigRevision"], "3");
        assert_eq!(requests[3]["expectedContextRevision"], "2");
        assert_eq!(requests[4]["mode"], "rollbackAndClose");
        assert_eq!(requests[5]["attachmentToken"], "token-1");

        let decoded: Vec<serde_json::Value> = requests;
        // 反序列化回各请求类型，验证形状与字面值一致。
        let open: OpenSessionRequest =
            serde_json::from_value(decoded[0].clone()).expect("openSession");
        assert_eq!(open.validate(), Ok(()));
        let in_session: ExecuteInSessionRequest =
            serde_json::from_value(decoded[1].clone()).expect("executeInSession");
        assert_eq!(in_session.validate(), Ok(()));
        let at_target: ExecuteAtTargetRequest =
            serde_json::from_value(decoded[2].clone()).expect("executeAtTarget");
        assert_eq!(at_target.validate(), Ok(()));
        let set_context: SetSessionContextRequest =
            serde_json::from_value(decoded[3].clone()).expect("setSessionContext");
        assert_eq!(set_context.validate(), Ok(()));
        let close: CloseSessionRequest = serde_json::from_value(decoded[4].clone()).expect("close");
        assert_eq!(close.mode, CloseMode::RollbackAndClose);
        let attach: AttachmentRequest = serde_json::from_value(decoded[5].clone()).expect("attach");
        assert_eq!(attach.validate(), Ok(()));
    }

    #[test]
    fn delegated_context_and_job_owner_round_trip() {
        let job_owner = OwnerRef::Job {
            job_id: datazen_platform_api::id::JobId::new("job-1"),
            stage_id: datazen_platform_api::id::StageId::new("stage-1"),
        };
        let request = OpenSessionRequest::new(target(), job_owner, IdempotencyKey::new("idem-5"));
        let encoded = serde_json::to_string(&request).expect("encode");
        let decoded: OpenSessionRequest = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded, request);

        let context = RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("service"),
            None,
            ClientInstanceId::new("worker"),
            RequestId::new("req-2"),
            Some(DelegationId::new("delegation-1")),
        );
        assert!(context.is_delegated());
    }

    #[test]
    fn command_input_is_passed_through_untouched() {
        let mut options = BTreeMap::new();
        options.insert("k".to_string(), serde_json::Value::Null);
        let call = CommandCall::new("describe", serde_json::json!({ "options": options }));
        assert!(call.validate().is_ok());
        let encoded = serde_json::to_string(&call).expect("encode");
        let decoded: CommandCall = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded, call);
    }

    #[test]
    fn no_request_carries_identity_free_text_fields() {
        // 概要 §5.1：请求体不得携带 organizationId / principalId 供入口采信。
        let source = code_only(include_str!("requests.rs"));
        let body = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string();
        for forbidden in [
            "pub organization_id",
            "pub principal_id",
            "pub client_instance_id",
            "pub authentication_session_id",
        ] {
            assert!(!body.contains(forbidden), "请求 DTO 不得出现 {forbidden}");
        }
    }
}
