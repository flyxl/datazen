//! §7.6 取消路径 + D-01。
//!
//! ## D-01 我需要 registry 的 `CancelReceipt`，协调者需裁定
//!
//! 架构文档 §7.6 要求 `cancelExecution` 返回 `CancelReceipt { executionId, disposition, state }`，
//! 其中 `disposition` 是三态 `requested` / `unsupported` / `alreadyFinished`。
//! 但 `registry/port.rs` 当前的 `cancel_execution` 仍返回 `ExecutionState`：
//!
//! ```text
//! async fn cancel_execution(&self, handle: &SessionHandle, execution_id: &ExecutionId)
//!     -> Result<ExecutionState, RuntimeError>;
//! ```
//!
//! 契约文档已经预告了这次改签（原文：「Wave 1 落地 `CancelReceipt` 后应把本方法返回值
//! 换成它，**不要**在 Wave 2 里就地私造一个结构体绕过去」）。三条轨道并行，
//! registry 轨道的 `CancelReceipt` 不在本工作树里。
//!
//! 因此本模块采取的是**既不越界、也不私造同名结构**的做法：
//!
//! - **不改** `registry/port.rs`（§1 禁止修改 `registry/**`）；
//! - **不**在本模块定义任何名为 `CancelReceipt` 的类型——同名会与未来 registry 的
//!   正式类型撞车，让协调者合并时出现两个「官方的」取消回执；
//! - 网关侧类型一律以 `Gateway`/`Cancel` 前缀命名（[`CancelDisposition`]、
//!   [`CancelOutcome`]），与 registry 的正式类型在名字上就可区分；
//! - 三态映射收敛在唯一一处 [`disposition_from_port_state`]，
//!   registry 落地 `CancelReceipt` 后，只需把这里换成对返回值的透传，
//!   上层 [`CancelOutcome`] 的形状不用动。
//!
//! 名字里带上 `Gateway` 前缀不是洁癖：两条轨道合并后，
//! 谁能一眼分清「谁定义的类型」比省三个字符重要得多。

use crate::connection::capability::PreciseCancel;
use crate::connection::{ExecutionId, ExecutionState, ResourceId, RuntimeError, SessionHandle};
use crate::gateway::provenance::{ExecutionSource, RequestPrincipal};

/// §7.6 取消绑定：三方必须一致。
///
/// `resourceBindingId` 单列是有代价的：只校验 `executionId` + `runtimeEpoch` 时，
/// 「用事务句柄 A 发起的执行」可以被「游标句柄 B」取消掉，
/// 而两者恰好共享同一个 `dbSessionId` 与 epoch。少这一项就是一条真实的越权路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelBinding {
    pub execution_id: ExecutionId,
    pub handle: SessionHandle,
    pub resource_binding_id: Option<ResourceId>,
}

impl CancelBinding {
    pub fn new(execution_id: ExecutionId, handle: SessionHandle) -> Self {
        Self {
            execution_id,
            handle,
            resource_binding_id: None,
        }
    }

    pub fn with_resource_binding(mut self, resource_binding_id: ResourceId) -> Self {
        self.resource_binding_id = Some(resource_binding_id);
        self
    }
}

/// 取消请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelRequest {
    pub binding: CancelBinding,
    /// 驱动是否支持精确取消。
    ///
    /// `SessionPort` 没有能力查询方法，网关拿不到驱动的 `PreciseCancel`，
    /// 所以这个值由**持有 `CapabilitySnapshot` 的调用方**传进来。
    /// 它决定网关是否允许触达驱动：见 [`CancelDisposition::Unsupported`]。
    pub precise_cancel: PreciseCancel,
}

impl CancelRequest {
    pub fn new(binding: CancelBinding, precise_cancel: PreciseCancel) -> Self {
        Self {
            binding,
            precise_cancel,
        }
    }
}

/// 取消处置三态（与 registry 的 `disposition` 语义一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelDisposition {
    /// 已下发取消请求。
    Requested,
    /// 驱动不支持精确取消。
    ///
    /// **绝不**降级成「会话级取消」——那会停掉同一会话里所有无关的执行。
    Unsupported,
    /// 该执行已是终态，运行时没有再做任何事。
    AlreadyFinished,
}

impl CancelDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            CancelDisposition::Requested => "requested",
            CancelDisposition::Unsupported => "unsupported",
            CancelDisposition::AlreadyFinished => "alreadyFinished",
        }
    }

    /// 是否真的下发了取消。
    pub fn is_requested(self) -> bool {
        matches!(self, CancelDisposition::Requested)
    }
}

/// 取消结果。
///
/// `state` 是**驱动侧观测到的**执行状态，不是「取消成功」的证明。
/// `disposition == requested` 且 `state == Running` 是完全正常的：
/// 取消是异步的，终态要等事件流。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelOutcome {
    pub execution_id: ExecutionId,
    pub disposition: CancelDisposition,
    pub state: ExecutionState,
    pub observed_at_nanos: u64,
}

impl CancelOutcome {
    pub fn new(
        execution_id: ExecutionId,
        disposition: CancelDisposition,
        state: ExecutionState,
        observed_at_nanos: u64,
    ) -> Self {
        Self {
            execution_id,
            disposition,
            state,
            observed_at_nanos,
        }
    }

    /// 是否已达终态。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
        )
    }

    /// 审计形态：不含任何句柄。
    pub fn to_persistable_json(&self) -> serde_json::Value {
        serde_json::json!({
            "executionId": self.execution_id.as_str(),
            "disposition": self.disposition.as_str(),
            "state": state_literal(self.state),
            "observedAtNanos": self.observed_at_nanos,
        })
    }
}

/// 端口状态字面量。
pub fn state_literal(state: ExecutionState) -> &'static str {
    match state {
        ExecutionState::Queued => "queued",
        ExecutionState::Running => "running",
        ExecutionState::CancelRequested => "cancelRequested",
        ExecutionState::Succeeded => "succeeded",
        ExecutionState::Failed => "failed",
        ExecutionState::Cancelled => "cancelled",
    }
}

/// 由端口返回的 `ExecutionState` 推导三态处置。
///
/// registry 落地 `CancelReceipt` 后，这里退化成一行透传；在那之前它是
/// 「端口还没给出 disposition」的唯一补偿点。三态映射必须完整：
///
/// - `Queued` / `Running` / `CancelRequested` → [`CancelDisposition::Requested`]，
///   因为取消请求已被接受，尚未见终态；
/// - `Succeeded` / `Failed` / `Cancelled` → [`CancelDisposition::AlreadyFinished`]。
///
/// 这个函数**不可能**返回 [`CancelDisposition::Unsupported`]：
/// 端口只会在「已经把取消交给驱动」之后才返回 `Ok`，既然交给了驱动，
/// 驱动就不属于「不支持精确取消」那一类。`Unsupported` 只在
/// [`CancelRequest::precise_cancel`] 为 `PreciseCancel::Unsupported`、
/// 网关因此**根本没有调用端口**时产生。
pub fn disposition_from_port_state(state: ExecutionState) -> CancelDisposition {
    match state {
        ExecutionState::Queued | ExecutionState::Running | ExecutionState::CancelRequested => {
            CancelDisposition::Requested
        }
        ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled => {
            CancelDisposition::AlreadyFinished
        }
    }
}

/// 绑定核验失败的原因。
pub mod binding {
    pub const UNKNOWN_EXECUTION: &str = "unknownExecution";
    pub const SESSION_MISMATCH: &str = "cancelSessionMismatch";
    pub const EPOCH_MISMATCH: &str = "cancelEpochMismatch";
    pub const RESOURCE_BINDING_MISMATCH: &str = "cancelResourceBindingMismatch";
}

/// 核验取消绑定的三方一致性。
///
/// 返回 `Err(&'static str)` 时调用方**不得**触达端口：一个绑定不符的取消
/// 如果照样下发，受害的是另一个执行。
pub fn verify_binding(
    request: &CancelRequest,
    bound_handle: &SessionHandle,
    bound_resource_binding: Option<&ResourceId>,
) -> Result<(), &'static str> {
    let wanted = &request.binding.handle;
    if wanted.db_session_id != bound_handle.db_session_id {
        return Err(binding::SESSION_MISMATCH);
    }
    if wanted.runtime_epoch != bound_handle.runtime_epoch {
        return Err(binding::EPOCH_MISMATCH);
    }
    match (
        request.binding.resource_binding_id.as_ref(),
        bound_resource_binding,
    ) {
        (Some(wanted_id), Some(bound_id)) if wanted_id == bound_id => Ok(()),
        (None, None) => Ok(()),
        _ => Err(binding::RESOURCE_BINDING_MISMATCH),
    }
}

/// 把端口错误翻成网关错误时用的常量。
pub const UNKNOWN_EXECUTION_REASON: &str = binding::UNKNOWN_EXECUTION;

/// 驱动不支持精确取消时的结果构造。
///
/// **不**调用端口、不落任何取消记录——因为没有取消发生。
pub fn unsupported_outcome(
    execution_id: &ExecutionId,
    last_known_state: ExecutionState,
    observed_at_nanos: u64,
) -> CancelOutcome {
    CancelOutcome::new(
        execution_id.clone(),
        CancelDisposition::Unsupported,
        last_known_state,
        observed_at_nanos,
    )
}

/// 取消失败 → 错误。**永远不**降级成「已取消」。
pub fn cancel_failed(reason: &'static str) -> RuntimeError {
    RuntimeError::CancelFailed(reason)
}

/// 授权视图：取消也需要一次权限判定（CM-62）。
pub struct CancelAuthorization<'a> {
    pub principal: &'a RequestPrincipal,
    pub source: &'a ExecutionSource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::testing_support;

    fn binding(epoch: u64) -> CancelBinding {
        CancelBinding::new(
            ExecutionId::new("exe_a_1"),
            testing_support::handle("dbse_a", epoch),
        )
    }

    #[test]
    fn disposition_literals_match_the_architecture_map() {
        assert_eq!(CancelDisposition::Requested.as_str(), "requested");
        assert_eq!(CancelDisposition::Unsupported.as_str(), "unsupported");
        assert_eq!(
            CancelDisposition::AlreadyFinished.as_str(),
            "alreadyFinished"
        );
    }

    #[test]
    fn a_live_state_maps_to_requested_and_a_terminal_one_to_already_finished() {
        assert_eq!(
            disposition_from_port_state(ExecutionState::Queued),
            CancelDisposition::Requested
        );
        assert_eq!(
            disposition_from_port_state(ExecutionState::Running),
            CancelDisposition::Requested
        );
        assert_eq!(
            disposition_from_port_state(ExecutionState::CancelRequested),
            CancelDisposition::Requested
        );
        assert_eq!(
            disposition_from_port_state(ExecutionState::Succeeded),
            CancelDisposition::AlreadyFinished
        );
        assert_eq!(
            disposition_from_port_state(ExecutionState::Failed),
            CancelDisposition::AlreadyFinished
        );
        assert_eq!(
            disposition_from_port_state(ExecutionState::Cancelled),
            CancelDisposition::AlreadyFinished
        );
    }

    #[test]
    fn the_port_mapping_can_never_produce_unsupported() {
        let all = [
            ExecutionState::Queued,
            ExecutionState::Running,
            ExecutionState::CancelRequested,
            ExecutionState::Succeeded,
            ExecutionState::Failed,
            ExecutionState::Cancelled,
        ];
        for state in all {
            assert_ne!(
                disposition_from_port_state(state),
                CancelDisposition::Unsupported,
                "unsupported 只能来自网关拒绝下发，不能来自端口状态"
            );
        }
    }

    #[test]
    fn an_epoch_mismatch_is_refused_before_touching_the_port() {
        let request = CancelRequest::new(binding(4), PreciseCancel::Supported);
        let bound = testing_support::handle("dbse_a", 5);
        assert_eq!(
            verify_binding(&request, &bound, None),
            Err(binding::EPOCH_MISMATCH)
        );
    }

    #[test]
    fn a_session_mismatch_is_refused_before_touching_the_port() {
        let request = CancelRequest::new(binding(5), PreciseCancel::Supported);
        let bound = testing_support::handle("dbse_other", 5);
        assert_eq!(
            verify_binding(&request, &bound, None),
            Err(binding::SESSION_MISMATCH)
        );
    }

    #[test]
    fn a_resource_binding_mismatch_is_refused_on_both_sides() {
        let request = CancelRequest::new(
            CancelBinding::new(
                ExecutionId::new("exe_a_1"),
                testing_support::handle("dbse_a", 5),
            )
            .with_resource_binding(ResourceId::new("res_a")),
            PreciseCancel::Supported,
        );
        let bound = testing_support::handle("dbse_a", 5);
        assert_eq!(
            verify_binding(&request, &bound, Some(&ResourceId::new("res_a"))),
            Ok(())
        );
        assert_eq!(
            verify_binding(&request, &bound, Some(&ResourceId::new("res_b"))),
            Err(binding::RESOURCE_BINDING_MISMATCH)
        );
        assert_eq!(
            verify_binding(&request, &bound, None),
            Err(binding::RESOURCE_BINDING_MISMATCH)
        );
    }

    #[test]
    fn an_unknown_execution_never_becomes_a_cancel_error_free_success() {
        let err = cancel_failed(UNKNOWN_EXECUTION_REASON);
        assert_eq!(err.reason(), "cancelFailed");
    }

    #[test]
    fn an_unsupported_driver_is_not_downgraded_to_a_session_wide_cancel() {
        let outcome =
            unsupported_outcome(&ExecutionId::new("exe_a_1"), ExecutionState::Running, 77);
        assert_eq!(outcome.disposition, CancelDisposition::Unsupported);
        assert!(!outcome.disposition.is_requested());
        assert_eq!(outcome.state, ExecutionState::Running);
        assert_eq!(outcome.observed_at_nanos, 77);
    }

    #[test]
    fn a_cancel_outcome_never_carries_a_runtime_handle() {
        let outcome =
            unsupported_outcome(&ExecutionId::new("exe_a_1"), ExecutionState::Running, 77);
        let json = outcome.to_persistable_json();
        for forbidden in ExecutionSource::forbidden_persistable_fields() {
            assert!(
                json.get(*forbidden).is_none(),
                "{forbidden} 不得进入取消记录"
            );
        }
        assert_eq!(
            json.get("disposition").and_then(serde_json::Value::as_str),
            Some("unsupported")
        );
    }

    #[test]
    fn only_succeeded_failed_and_cancelled_count_as_terminal() {
        let live = CancelOutcome::new(
            ExecutionId::new("e"),
            CancelDisposition::Requested,
            ExecutionState::Running,
            1,
        );
        assert!(!live.is_terminal());
        let done = CancelOutcome::new(
            ExecutionId::new("e"),
            CancelDisposition::AlreadyFinished,
            ExecutionState::Cancelled,
            1,
        );
        assert!(done.is_terminal());
    }

    #[test]
    fn cancel_authorization_carries_the_frozen_source() {
        let principal = RequestPrincipal::new(
            crate::connection::PrincipalId::new("principal_a"),
            crate::connection::OrganizationId::new("org_a"),
            crate::connection::DbSessionId::new("dbse_a"),
        );
        let source = ExecutionSource::new(
            crate::gateway::provenance::SourceKind::Editor,
            "edt_a",
            None,
            None,
        );
        let view = CancelAuthorization {
            principal: &principal,
            source: &source,
        };
        assert_eq!(view.source.source_id(), "edt_a");
    }
}
