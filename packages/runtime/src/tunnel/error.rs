//! `tunnel` 模块的唯一拒绝出口 —— 与 `resource/mod.rs:89-124` 同一纪律：
//! 一个模块一套错误、一个稳定字面码集，且全部映射到**既有**的 [`RuntimeError`]，
//! 不新增线路可见的拒绝形状。
//!
//! **为什么不是 `PortError`**：与 `resource::ResourceError` 同一个理由 ——
//! `PortError` 只有 7 个变体且刻意不实现 `Serialize`，而 platform-api 是冻结上游、
//! 禁止加变体；会话面则已经有 `RuntimeError`。所以本模块给出自己精确的原因集，
//! 再用 [`TunnelError::as_runtime_error`] 把每一个变体映射到**既有的** `RuntimeError` 变体上。

use datazen_platform_api::ports::network::TunnelSpec;

use super::ledger::TunnelState;
use super::transport::TunnelFault;
use crate::connection::RuntimeError;

/// 隧道层的错误。
#[derive(Debug, thiserror::Error)]
pub enum TunnelError {
    /// 物理接缝拒绝了这次开/关隧道。
    #[error("tunnel transport fault: {fault:?} (spec={})", spec.via_host)]
    Transport {
        spec: TunnelSpec,
        fault: TunnelFault,
    },

    /// 这条隧道当前不接受新的引用（正在拆除 / 拆除结果不明 / 已失败）。
    ///
    /// 绝不把调用方塞进一条正在拆除或已失败的隧道 —— 那是 CM-28
    /// 「隧道不多减引用」失效的前置状态。
    #[error("tunnel is not shareable in state {}", state.as_str())]
    NotShareable {
        spec: TunnelSpec,
        state: TunnelState,
    },

    /// 同一条租约**已经**引用着这条隧道。
    ///
    /// 引用必须与依赖清单一一对应，否则一次归还只减 1 而计数里躺着 2，
    /// 剩下的那个引用**永远没人还**，隧道泄漏。这是调用方的 bug，必须当场拒绝。
    #[error("this resource already holds a reference to the tunnel via {}", spec.via_host)]
    AlreadyHeld { spec: TunnelSpec },

    /// 取不到路由/隧道配置版本。**不得**当成「与上一次相同」。
    #[error("tunnel route revision unavailable")]
    RevisionUnavailable,
}

impl TunnelError {
    /// 稳定字面码 —— 与 `resource::ResourceError::reason` 同一形态。
    ///
    /// `TunnelSpec` 的 `Debug` 会经 `NetworkRouteRef` 打印成 `NetworkRouteRef(<redacted>)`，
    /// 因此这里的错误文案不会带出凭据。
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Transport { fault, .. } => match fault {
                TunnelFault::Open => "tunnelTransportOpenFailed",
                TunnelFault::Close => "tunnelTransportCloseFailed",
            },
            Self::NotShareable { .. } => "tunnelNotShareable",
            Self::AlreadyHeld { .. } => "tunnelAlreadyHeld",
            Self::RevisionUnavailable => "tunnelRevisionUnavailable",
        }
    }
}

/// 映射到**既有**的 `RuntimeError` 变体。本模块**不新增**任何线路可见的拒绝形状，
/// 与 `resource::ResourceError::as_runtime_error` 同一纪律。
impl TunnelError {
    pub fn as_runtime_error(&self) -> RuntimeError {
        match self {
            // 基础设施能力失败 ⇒ 隔离，绝不放行一条指向死端口的隧道。
            Self::Transport { .. } | Self::RevisionUnavailable => {
                RuntimeError::SessionQuarantined(self.reason())
            }
            // 正在拆除 / 拆除结果不明 / 已失败 ⇒ 同上，绝不新发引用。
            // 重复引用 ⇒ 调用方的账已经乱了，同样不能悄悄放行（放行＝泄漏）。
            Self::NotShareable { .. } | Self::AlreadyHeld { .. } => {
                RuntimeError::SessionQuarantined(self.reason())
            }
        }
    }

    /// 唯一的拒绝出口：记日志 + 转成既有 `RuntimeError`。**任何拒绝都不允许静默**。
    pub fn rejected<T>(self, operation: &'static str) -> Result<T, RuntimeError> {
        tracing::warn!(
            target: "datazen_runtime::tunnel",
            operation,
            reason = self.reason(),
            "tunnel ledger refused the request"
        );
        Err(self.as_runtime_error())
    }
}
