//! §7.2 请求受理：校验 handle → 授权重校验 → 幂等查重 → 生成 executionId → 入队 → 回执。
//!
//! ## 回执是「受理回执」，不是「执行结果」
//!
//! [`GatewayAcceptance`] 返回的 [`ExecutionReceipt`] 的 `state` **恒为 `Queued`**。
//! 拿到它只说明这条请求被网关收下了，**不代表 SQL 跑过，更不代表成功**。
//! 把 `Queued` 当成 `Succeeded` 读，是这条链路上最容易犯也最贵的错误——
//! 调用方会对着一个从未执行的 ID 去轮询结果、去做乐观 UI 回填。
//! 因此本模块把 `state` 的取值写进类型系统：
//! [`GatewayAcceptance::receipt`] 的构造只在这里发生，且只构造 `Queued`。
//!
//! ## 乐观并发闸门为什么查两次
//!
//! §7.2 第 4 步会核对 `expectedContextRevision`，但受理与真正派发之间隔着排队、
//! 可能还隔着用户确认弹窗。这段时间里任何 `SET` / 事务提交都会推高 `contextRevision`。
//! 只在受理时核对一次，等于让一条**基于过期上下文**的语句在旧语义下跑完整条链路。
//! 闸门因此在两处生效：
//!
//! - **受理时**（本模块 [`ExecutionRequest::check_context_revision`]）：快速失败，
//!   让调用方不必等一次完整的派发往返；
//! - **派发前**（`ExecutionGateway::dispatch`）：真正的闸门。
//!
//! 两处抛的是**同一个** [`RuntimeError::ContextRevisionMismatch`]，且 `actual`
//! 永远是服务端当前值，调用方据此重读会话即可重试。

use serde_json::json;

use crate::connection::RuntimeError;
use crate::connection::{
    CommandCall, Counter, ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState,
    ResourceId, SessionHandle, StreamId,
};
use crate::gateway::idempotency::{IdempotencyScope, RequestFingerprint};
use crate::gateway::provenance::{ExecutionSource, GatewayAction};

/// 一次执行请求。
///
/// ## `Debug` 是手写的：只脱敏凭据字段
///
/// 装了令牌层（CM-70）之后 `idempotency_key` 就是那把**可重放**的签名提交令牌，
/// 而 [`ExecutionRequest`] 是公开构造、公开持有的输入 DTO——派生 `Debug` 一路走到
/// 这里就把一份凭据抄进了日志、`panic` 文本与 `assert_eq!` 的失败输出。
/// 因此下面**不派生** `Debug`，改为手写一份只把 `idempotency_key` 打成 `<redacted>`
/// 的实现（紧随本结构其后）。
///
/// **脱敏范围只限凭据字段**：`call` 原样输出。它是负载，而排查幂等问题**恰恰要看
/// payload**（同一个键配了哪条 SQL、参数差在哪），把它抹掉会让这条诊断日志失去意义。
/// ⚠️ 将来若某条驱动命令把凭据放进了 `call` 的参数里，此处的脱敏口径需要重新评估。
#[derive(Clone, PartialEq)]
pub struct ExecutionRequest {
    pub handle: SessionHandle,
    pub expected_context_revision: Counter,
    pub call: CommandCall,
    pub idempotency_key: String,
    /// CM-61：来源来自**请求**。受理后即冻结，后续事件不得改写。
    pub source: ExecutionSource,
    /// §7.6 取消绑定用的资源绑定 id。
    ///
    /// 取消必须核验 `executionId` / `runtimeEpoch` / `resourceBindingId` 三者一致，
    /// 少了第三个就会出现「用 A 执行的句柄去取消 B」这种跨资源取消。
    pub resource_binding_id: Option<ResourceId>,
}

/// CM-70：输入侧同一把令牌同样走派生 `Debug` 就是明文泄漏——`format!("{req:?}")`
/// 会连 MAC 段一起逐字打印。这层与 [`super::ExecutionRecord`] 的输出侧脱敏是**两个
/// 独立缺口**，必须各自堵：只挡最外层，派生 `Debug` 照样会一路走到叶子字段。
impl std::fmt::Debug for ExecutionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionRequest")
            .field("handle", &self.handle)
            .field("expected_context_revision", &self.expected_context_revision)
            .field("call", &self.call)
            .field("idempotency_key", &"<redacted>")
            .field("source", &self.source)
            .field("resource_binding_id", &self.resource_binding_id)
            .finish()
    }
}

impl ExecutionRequest {
    pub fn new(
        handle: SessionHandle,
        expected_context_revision: Counter,
        call: CommandCall,
        idempotency_key: impl Into<String>,
        source: ExecutionSource,
    ) -> Self {
        Self {
            handle,
            expected_context_revision,
            call,
            idempotency_key: idempotency_key.into(),
            source,
            resource_binding_id: None,
        }
    }

    pub fn with_resource_binding(mut self, resource_binding_id: ResourceId) -> Self {
        self.resource_binding_id = Some(resource_binding_id);
        self
    }

    /// §7.2 第 1 步：句柄与参数校验。
    ///
    /// 这一步**不**碰端口、不生成任何 id，因此它的失败不会留下任何痕迹——
    /// 调用方可以直接用同个幂等键重试。
    pub fn validate(&self) -> Result<(), GatewayError> {
        if self.handle.db_session_id.is_empty() {
            return Err(GatewayError::InvalidRequest {
                reason: "dbSessionIdRequired",
            });
        }
        if self.call.command.trim().is_empty() {
            return Err(GatewayError::InvalidRequest {
                reason: "commandRequired",
            });
        }
        let scope = self.scope();
        if !scope.is_usable() {
            return Err(GatewayError::InvalidRequest {
                reason: "idempotencyKeyRequired",
            });
        }
        if !self.source.is_persistable() {
            return Err(GatewayError::InvalidRequest {
                reason: "sourceNotPersistable",
            });
        }
        Ok(())
    }

    /// 幂等作用域。
    pub fn scope(&self) -> IdempotencyScope {
        IdempotencyScope::from_handle(&self.handle, self.idempotency_key.clone())
    }

    /// 请求指纹。**来源参与指纹**（CM-61）：否则同一个 key 换一个来源重发
    /// 会被判成重发，回执里的 `source` 就会跟着重发请求走，而不是留在第一次。
    pub fn fingerprint(&self) -> RequestFingerprint {
        RequestFingerprint::of(&self.call, self.expected_context_revision, &self.source)
    }

    /// 乐观并发闸门（受理时的那一次）。
    pub fn check_context_revision(&self, actual: Counter) -> Result<(), RuntimeError> {
        if self.expected_context_revision == actual {
            Ok(())
        } else {
            Err(RuntimeError::ContextRevisionMismatch {
                expected: self.expected_context_revision.get(),
                actual: actual.get(),
            })
        }
    }

    /// 端口请求体。
    ///
    /// `idempotencyKey` 原样透传：registry 侧若也维护幂等，两边看到的是同一个键。
    pub fn to_port_request(&self) -> crate::connection::ExecuteInSessionRequest {
        crate::connection::ExecuteInSessionRequest {
            handle: self.handle.clone(),
            expected_context_revision: self.expected_context_revision,
            call: self.call.clone(),
            idempotency_key: self.idempotency_key.clone(),
        }
    }
}

/// 受理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceDisposition {
    /// 首次受理。
    Accepted,
    /// 幂等重发：返回**同一个** executionId，没有产生新执行。
    Replayed { first_seen_at_nanos: u64 },
}

impl AcceptanceDisposition {
    pub fn is_replay(self) -> bool {
        matches!(self, AcceptanceDisposition::Replayed { .. })
    }
}

/// 受理回执。
///
/// 注意 `receipt.state` 恒为 [`ExecutionState::Queued`]——它是「已受理」，
/// 不是「已完成」。真正的终态要等事件流或派发回执。
#[derive(Debug, Clone, PartialEq)]
pub struct GatewayAcceptance {
    pub receipt: ExecutionReceipt,
    pub disposition: AcceptanceDisposition,
    /// CM-61：受理时冻结的来源。
    pub source: ExecutionSource,
    /// 首次受理时刻（单调纳秒）。重发时是**第一次**的时刻，不是本次的时刻。
    pub accepted_at_nanos: u64,
}

impl GatewayAcceptance {
    /// 构造首次受理的回执。`state` 在这里被钉死为 `Queued`。
    pub(crate) fn accepted(
        execution_id: &ExecutionId,
        source: &ExecutionSource,
        accepted_at_nanos: u64,
    ) -> Self {
        Self {
            receipt: ExecutionReceipt {
                execution_id: execution_id.clone(),
                stream_id: StreamId::new(format!("strm_{}", execution_id.as_str())),
                state: ExecutionState::Queued,
            },
            disposition: AcceptanceDisposition::Accepted,
            source: source.clone(),
            accepted_at_nanos,
        }
    }

    /// 构造幂等重发的回执。
    ///
    /// 状态保持首次的 `Queued`：受理那一刻它就是 `Queued`，
    /// 把「重发时它已经跑完了」写进受理回执会让回执不再是受理回执。
    pub(crate) fn replayed(
        execution_id: &ExecutionId,
        source: &ExecutionSource,
        first_seen_at_nanos: u64,
    ) -> Self {
        Self {
            receipt: ExecutionReceipt {
                execution_id: execution_id.clone(),
                stream_id: StreamId::new(format!("strm_{}", execution_id.as_str())),
                state: ExecutionState::Queued,
            },
            disposition: AcceptanceDisposition::Replayed {
                first_seen_at_nanos,
            },
            source: source.clone(),
            accepted_at_nanos: first_seen_at_nanos,
        }
    }

    pub fn execution_id(&self) -> &ExecutionId {
        &self.receipt.execution_id
    }

    pub fn is_replay(&self) -> bool {
        self.disposition.is_replay()
    }

    /// 断言这**不是**执行结果。供测试与调用方自查使用。
    pub fn is_queued_not_finished(&self) -> bool {
        self.receipt.state == ExecutionState::Queued
    }
}

/// 网关自身的错误。
///
/// [`GatewayError::Runtime`] 是**透明**的：端口给出的 `UnknownSession` / `SessionLost` /
/// `SessionClosed` / `BudgetExhausted` / `SessionQuarantined` / `CancelFailed`
/// 一律原样上抛，绝不被包装成「已受理」或降级成别的语义。
/// 其余变体是网关**自己**的判定（权限、幂等、参数），因为 `RuntimeError`
/// 是冻结 DTO（§4 不可为其新增变体）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GatewayError {
    #[error("runtime: {0}")]
    Runtime(#[from] RuntimeError),

    #[error("permission denied for {action:?}: {reason}")]
    PermissionDenied {
        action: GatewayAction,
        reason: &'static str,
    },

    /// CM-54：既有记录读不出来。**要求核验**，不得自动换键重写。
    #[error("idempotency record unreadable, verification required: {message}")]
    IdempotencyVerificationRequired { message: String },

    /// CM-54：同一个键被复用于另一个语义请求。
    ///
    /// **刻意没有键本身**（CM-70 高危缺陷 1 的修复）：装了令牌层之后键就是那把
    /// 签名提交令牌，回显它等于把一份可重放的凭据抄进错误消息和审计落盘。
    /// 而且冲突**只可能**发生在与本次受理完全相同的作用域上——账本按
    /// `(dbSessionId, runtimeEpoch, 键)` 查记录，读到记录才是冲突——
    /// 所以这里的键恒等于调用方刚递上来的那把，回显给提交者恒为零信息。
    /// `existing` 已经指明账本里那条记录（它的作用域就是这把键），
    /// `incoming` 是语义指纹，两者合起来足以定位冲突，不需要凭据。
    #[error(
        "idempotency key already bound to execution {existing}, incoming fingerprint {incoming}"
    )]
    IdempotencyConflict {
        existing: ExecutionId,
        incoming: String,
    },

    /// 幂等记录写不下去：请求**没有**被受理，可以原样重试。
    #[error("idempotency record could not be persisted: {message}")]
    IdempotencyPersistFailed { message: String },

    /// CM-70：提交令牌在第 0 步被拒（验签 / 版本 / 过期 / 退役）。
    ///
    /// 刻意**不带**令牌本身，也不带 key：审计落盘里留一份令牌摘要就等于
    /// 给伪造者一个 oracle 去试。只留机器可读的 `reason`。
    #[error("submission token rejected: {reason}")]
    SubmissionTokenRejected { reason: &'static str },

    #[error("invalid request: {reason}")]
    InvalidRequest { reason: &'static str },
}

/// 权限拒绝是网关自己的判定，转换是无损的直通。
impl From<crate::gateway::provenance::AuthorizationDenial> for GatewayError {
    fn from(denial: crate::gateway::provenance::AuthorizationDenial) -> Self {
        GatewayError::PermissionDenied {
            action: denial.action,
            reason: denial.reason,
        }
    }
}

impl GatewayError {
    /// 是否是「端口/运行时故障」类。测试用来确认**没有**被降级成受理成功。
    pub fn runtime(&self) -> Option<&RuntimeError> {
        match self {
            GatewayError::Runtime(err) => Some(err),
            _ => None,
        }
    }

    /// 审计落盘形态。刻意**不**包含任何句柄。
    pub fn to_persistable_json(&self) -> serde_json::Value {
        match self {
            GatewayError::Runtime(err) => json!({
                "kind": "runtime",
                "reason": err.reason(),
                "apiCode": err.api_code(),
            }),
            GatewayError::PermissionDenied { action, reason } => json!({
                "kind": "permissionDenied",
                "action": action.as_str(),
                "reason": reason,
            }),
            GatewayError::IdempotencyVerificationRequired { message } => json!({
                "kind": "idempotencyVerificationRequired",
                "message": message,
            }),
            GatewayError::IdempotencyConflict { existing, incoming } => json!({
                "kind": "idempotencyConflict",
                "existing": existing.as_str(),
                "incoming": incoming,
            }),
            GatewayError::IdempotencyPersistFailed { message } => json!({
                "kind": "idempotencyPersistFailed",
                "message": message,
            }),
            GatewayError::SubmissionTokenRejected { reason } => json!({
                "kind": "submissionTokenRejected",
                "reason": reason,
            }),
            GatewayError::InvalidRequest { reason } => json!({
                "kind": "invalidRequest",
                "reason": reason,
            }),
        }
    }
}

/// `ExecuteInSessionRequest` 的**脱敏** `Debug` 视图（只在这里用）。
///
/// 装了令牌层之后 `idempotency_key` 就是那把签名提交令牌，而 [`super::ExecutionRecord`]
/// 是公开可取的（`ExecutionGateway::execution` 直接把记录交给调用方），派生 `Debug`
/// 一路走到这里就把一份**可重放**的凭据抄进了日志与审计。只在网关这一侧拦一道：
/// `connection/session.rs` 是 CM-54 的冻结面，那边一个字节都不动。
///
/// **脱敏范围只限凭据字段**：`call` 原样输出。令牌是凭据、`call` 是负载，而排查幂等
/// 问题**恰恰要看 payload**（同一个键配了哪条 SQL、参数差在哪），抹掉它会让这条
/// 诊断日志失去意义——这是裁定，不是疏漏。
/// ⚠️ 将来若某条驱动命令把凭据放进了 `call` 的参数里，此处的脱敏口径需要重新评估。
pub(crate) struct RedactedExecuteRequest<'a>(pub &'a ExecuteInSessionRequest);

impl std::fmt::Debug for RedactedExecuteRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecuteInSessionRequest")
            .field("handle", &self.0.handle)
            .field(
                "expected_context_revision",
                &self.0.expected_context_revision,
            )
            .field("call", &self.0.call)
            .field("idempotency_key", &"<redacted>")
            .finish()
    }
}

// CM-06 的编译期 / 反序列化期负例：请求 DTO 上没有身份字段可伪造。
//
// 放在独立文件是单文件 800 行惯例所迫；用 `#[path]` 而非 `mod` 内联，
// 是为了让负例文件的模块级 `//!` 文档挂在正确的模块上。
#[cfg(doctest)]
#[path = "request_cm06_negatives.rs"]
mod cm06_identity_fields_cannot_enter_the_request;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::provenance::SourceKind;
    use crate::gateway::testing_support;

    fn source() -> ExecutionSource {
        ExecutionSource::new(SourceKind::Editor, "edt_a", None, None)
    }

    fn request() -> ExecutionRequest {
        ExecutionRequest::new(
            testing_support::handle("dbse_a", 5),
            Counter::new(3),
            CommandCall {
                command: "query".to_owned(),
                input: json!({"sql": "select 1"}),
            },
            "idem-1",
            source(),
        )
    }

    #[test]
    fn a_blank_command_is_rejected_before_any_id_is_allocated() {
        let mut req = request();
        req.call.command = "   ".to_owned();
        assert_eq!(
            req.validate(),
            Err(GatewayError::InvalidRequest {
                reason: "commandRequired"
            })
        );
        assert!(req.validate().is_err());
    }

    #[test]
    fn a_blank_idempotency_key_is_rejected() {
        let mut req = request();
        req.idempotency_key = String::new();
        assert_eq!(
            req.validate(),
            Err(GatewayError::InvalidRequest {
                reason: "idempotencyKeyRequired"
            })
        );
    }

    #[test]
    fn a_source_that_cannot_be_persisted_is_rejected() {
        let mut req = request();
        req.source = ExecutionSource::new(SourceKind::Editor, "", None, None);
        assert_eq!(
            req.validate(),
            Err(GatewayError::InvalidRequest {
                reason: "sourceNotPersistable"
            })
        );
    }

    #[test]
    fn the_optimistic_gate_reports_the_servers_actual_revision() {
        let req = request();
        assert!(req.check_context_revision(Counter::new(3)).is_ok());
        let err = req
            .check_context_revision(Counter::new(9))
            .err()
            .unwrap_or_else(|| panic!("修订号不同必须报错"));
        assert_eq!(
            err,
            RuntimeError::ContextRevisionMismatch {
                expected: 3,
                actual: 9
            }
        );
    }

    #[test]
    fn an_acceptance_receipt_is_queued_not_finished() {
        let id = ExecutionId::new("exe_a_1");
        let acceptance = GatewayAcceptance::accepted(&id, &source(), 42);
        assert_eq!(acceptance.receipt.state, ExecutionState::Queued);
        assert!(acceptance.is_queued_not_finished());
        assert!(!acceptance.is_replay());
        assert_eq!(acceptance.receipt.stream_id, StreamId::new("strm_exe_a_1"));
    }

    #[test]
    fn a_replay_returns_the_original_receipt_and_first_seen_time() {
        let id = ExecutionId::new("exe_a_1");
        let first = GatewayAcceptance::accepted(&id, &source(), 42);
        let replay = GatewayAcceptance::replayed(&id, &source(), 42);
        assert_eq!(replay.receipt, first.receipt);
        assert_eq!(replay.execution_id(), &id);
        assert_eq!(replay.accepted_at_nanos, 42);
        assert!(replay.is_replay());
        assert!(replay.is_queued_not_finished());
    }

    #[test]
    fn the_port_request_keeps_the_call_and_the_key_verbatim() {
        let req = request();
        let port_request = req.to_port_request();
        assert_eq!(port_request.idempotency_key, "idem-1");
        assert_eq!(port_request.expected_context_revision, Counter::new(3));
        assert_eq!(port_request.handle, testing_support::handle("dbse_a", 5));
        assert_eq!(port_request.call.command, "query");
    }

    #[test]
    fn a_runtime_error_is_transparent_and_never_looks_accepted() {
        let err: GatewayError = RuntimeError::BudgetExhausted("sessionBudgetExhausted").into();
        assert_eq!(
            err.runtime(),
            Some(&RuntimeError::BudgetExhausted("sessionBudgetExhausted"))
        );
        assert_eq!(
            err.to_persistable_json()
                .get("kind")
                .and_then(serde_json::Value::as_str),
            Some("runtime")
        );
    }

    #[test]
    fn every_gateway_error_has_a_machine_readable_kind() {
        let cases = vec![
            GatewayError::Runtime(RuntimeError::UnknownSession("s".to_owned())),
            GatewayError::PermissionDenied {
                action: GatewayAction::Execute,
                reason: "permissionDenied",
            },
            GatewayError::IdempotencyVerificationRequired {
                message: "reset".to_owned(),
            },
            GatewayError::IdempotencyConflict {
                existing: ExecutionId::new("e"),
                incoming: "f".to_owned(),
            },
            GatewayError::IdempotencyPersistFailed {
                message: "disk".to_owned(),
            },
            GatewayError::SubmissionTokenRejected {
                reason: "submissionTokenSignatureInvalid",
            },
            GatewayError::InvalidRequest {
                reason: "commandRequired",
            },
        ];
        for case in cases {
            assert!(!case.to_string().is_empty());
            assert!(
                case.to_persistable_json().get("kind").is_some(),
                "每种错误都必须有 kind"
            );
        }
    }

    /// 冲突记录只许有标识符，**不许**多出一个字段。
    ///
    /// 曾经有一版把幂等键原样写回落盘（CM-70 高危缺陷 1）：键在装了令牌层之后
    /// 就是那把签名提交令牌，落盘一份就等于给伪造者一份可重放的凭据。
    /// 这里用「字段全集相等」而不是逐个检查缺失，把这个口子钉死在映射表上——
    /// 往回加 `key` 会立刻转红。
    #[test]
    fn the_conflict_record_carries_only_identifiers() {
        let err = GatewayError::IdempotencyConflict {
            existing: ExecutionId::new("exe_conflict_1"),
            incoming: "fingerprint".to_owned(),
        };
        let json = err.to_persistable_json();
        let fields = json
            .as_object()
            .expect("错误记录必须是对象")
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            fields,
            ["existing", "incoming", "kind"]
                .iter()
                .map(|f| (*f).to_owned())
                .collect::<std::collections::BTreeSet<_>>(),
            "冲突记录里不得出现凭据或任何附加字段：{json}"
        );
    }

    #[test]
    fn forbidden_handles_never_leak_into_the_error_record() {
        let err = GatewayError::Runtime(RuntimeError::UnknownSession("dbse_a".to_owned()));
        let json = err.to_persistable_json();
        for forbidden in crate::gateway::provenance::ExecutionSource::forbidden_persistable_fields()
        {
            assert!(
                json.get(*forbidden).is_none(),
                "{forbidden} 不得进入错误记录"
            );
        }
    }
}
