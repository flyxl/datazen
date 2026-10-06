//! `ResourceManager` ⇄ `TunnelLedger` 的接线层。
//!
//! 回答一个此前没人回答的问题：**资源层怎么知道隧道存在？** 答案是
//! [`ResourceManager::with_tunnel_transport`] 接进来的那份台账，加上
//! [`LeaseRequest::via_tunnel`] 在申请上声明的隧道身份。本文件就是这两者之间那几根线。
//!
//! ## 唯一计数器铁律（CM-32）
//!
//! 本文件**不持有任何隧道引用计数**：没有 `AtomicU32`、没有 `Mutex<usize>`、
//! 没有 `Cell<u32>`，也没有「顺手存一份」的 `u32` 字段。
//! 唯一的权威计数是 `TunnelLedger::ref_count`，本文件**只读**它；
//! 引用归零**只能**经由 `TunnelLedger::return_resource` → `drain` 这一条路
//! （`drain` 是私有的，资源层连它的门都摸不到，更谈不上绕过它）。
//!
//! ## 归属配对（本模块要闭合的第二格）
//!
//! 「隧道引用」与「物理预算」是同一笔账的两半：**物理连接还占着预算，隧道就还占着引用**。
//! 判据只有一个 —— [`CleanupDisposition::releases_physical_budget`]：
//!
//! | 处置 | 物理预算 | 隧道引用 | 本文件的行为 |
//! |------|---------|---------|-------------|
//! | `ReturnedToPool` | 仍占用 | **保留** | 不碰台账 |
//! | `Quarantined` | 仍占用到强制关闭 | **保留** | 不碰台账 |
//! | `Closed` | 此刻释放 | **归还** | `return_resource` |
//!
//! 两条腿由**同一个布尔**同时驱动，因此不存在「预算还占着但引用已还」或反之的状态。
//! 这一点不靠自觉，靠 [`TunnelDisposition::pairs_with_budget_release`]：
//! 任何一次处置后，该方法的结果**恒等于** `CleanupReport::physical_budget_released`。
//!
//! ## 建连阶段序（CM-27 六个注入点）
//!
//! CM-27 在 permit / 隧道 / socket / 握手 / 初始化 / 注册六个阶段分别注入失败。
//! 本模块负责其中**隧道那一格及其之后的补偿**：
//!
//! ```text
//!   permit ──▶ socket ──▶ tunnel ──▶ handshake ──▶ init ──▶ register
//!              │          │            │
//!              │          ▼            ▼
//!              │       建不成⇒不落账   roll_back_unpublished ⇒ force_close(close + 归还引用)
//!              ▼
//!           失败时什么都没拿，无需回滚
//! ```
//!
//! **资源层的 socket 在 tunnel 之前**，与文档里部署序相反，这是被冻结的端口面逼出来的：
//! `LeaseId` 由 `PhysicalTransport::open` 返回的 `ResourceId` 派生
//! （`manager.rs` 建行处），而 `TunnelLedger::acquire` 的依赖方身份就是 `LeaseId` ——
//! 依赖方身份在 socket 之前**不存在**，隧道引用无从登记。
//! 这不是设计取舍，是端口面上的事实；回滚义务两向对称，矩阵六个阶段一个不漏。

use std::sync::Arc;

use datazen_platform_api::ports::network::TunnelSpec;

use crate::connection::LeaseId;
use crate::resource::cleanup::CleanupDisposition;
use crate::resource::{LeaseRecord, LeaseState, ResourceError, ResourceManager};
use crate::tunnel::{TunnelLedger, TunnelTransport};

/// 一次归还/隔离之后，隧道引用的处置结果。
///
/// 这是「隧道引用 ⇄ 物理预算」归属配对的**可断言形状**：调用方拿它和
/// [`crate::resource::CleanupReport::physical_budget_released`] 一比就知��两侧同没同步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelDisposition {
    /// 这条租约**从不**持有隧道引用（直连，或归还早已发生）。
    ///
    /// 两侧仍然自洽：没有隧道可还，预算该释放就释放。
    NoReference,
    /// 引用**随租约保留** —— 物理预算还在占用，隔离中的连接仍可能要走这条隧道。
    Retained,
    /// 引用已归还；`tunnel_closed` 表示隧道是否**被确认**拆除。
    ///
    /// `false` 不是「没还」，而是隧道侧的 `close` 结果不明：引用已经摘掉，
    /// 但台账按 CM-28「结果不明不静默丢弃」把条目留在 `Unconfirmed` 态。
    Released { tunnel_closed: bool },
}

impl TunnelDisposition {
    /// 本次处置是否释放了隧道侧的占用，**且**释放的是物理预算那一半。
    ///
    /// 配对不变式就写在这一行上：任何处置之后它都恒等于
    /// `CleanupDisposition::releases_physical_budget()`，因为驱动两者的
    /// 是同一个布尔（见模块头）。
    pub const fn pairs_with_budget_release(self) -> bool {
        match self {
            // 从不持有引用 ⇒ 无论预算放不放，两侧都不构成矛盾。
            Self::NoReference => true,
            // 预算还占着 ⇒ 引用必须也还占着。
            Self::Retained => false,
            // 预算此刻核销 ⇒ 引用同一刻归还。
            Self::Released { .. } => true,
        }
    }
}

impl ResourceManager {
    /// 接上隧道端口。之后带 [`LeaseRequest::via_tunnel`] 的申请会在台账里落一份引用，
    /// 不带的申请（直连）完全不碰隧道。
    ///
    /// 没接端口却声明了隧道的申请会被**拒绝**（[`ResourceError::TunnelPortNotWired`]），
    /// 绝不悄悄降级成直连 —— 悄悄直连等于在堡垒机的位置凭空开一条明文通路。
    pub fn with_tunnel_transport(mut self, transport: Arc<dyn TunnelTransport>) -> Self {
        self.tunnels = Some(TunnelLedger::new(transport));
        self
    }

    /// 是否接了隧道端口。**配置事实**，不是计数。
    pub fn has_tunnel_port(&self) -> bool {
        self.tunnels.is_some()
    }

    /// 隧道引用的权威计数快照 —— [`TunnelLedger::ref_count`] 的**直通投影**。
    ///
    /// 本方法**不存储**任何东西：返回值在调用瞬间从台账读出，调用后即丢弃。
    /// 台账是唯一权威；这里存在的意义是让上层能**观测**，而不是让它能**判断**
    /// （判断能否释放以 [`TunnelLedger::return_resource`] 的调用为准）。
    pub fn tunnel_refs(&self, spec: &TunnelSpec) -> Option<u32> {
        self.tunnels
            .as_ref()
            .and_then(|ledger| ledger.ref_count(spec))
    }

    /// 台账里尚未被彻底拆除的隧道条数（`live_tunnels` 的直通投影）。
    pub fn live_tunnels(&self) -> usize {
        self.tunnels.as_ref().map_or(0, TunnelLedger::live_tunnels)
    }

    /// 隧道侧累计 `close` 次数（`close_calls` 的直通投影，观测用）。
    pub fn tunnel_close_calls(&self) -> u64 {
        self.tunnels.as_ref().map_or(0, TunnelLedger::close_calls)
    }

    /// 隧道阶段：给这条租约落一份隧道引用。
    ///
    /// **失败不落账**：`TunnelLedger::acquire` 在 `open` 失败时既不建条目也不加计数
    /// （CM-27「建隧道失败不落账」），本方法原样上抛，绝不「先记上回头再补」。
    pub(super) fn acquire_tunnel_reference(
        &mut self,
        lease_id: &LeaseId,
        spec: &TunnelSpec,
    ) -> Result<(), ResourceError> {
        let ledger = self.tunnels.as_mut().ok_or_else(|| {
            ResourceError::TunnelPortNotWired
                .logged("lease declares a tunnel but no tunnel port is wired")
        })?;
        ledger
            .acquire(Some(spec), lease_id)
            .map(|_| ())
            .map_err(|error| {
                ResourceError::TunnelRefused(error.to_string()).logged(error_detail(&error))
            })
    }

    /// **归属配对的唯一执行点。**
    ///
    /// 判据只有 [`CleanupDisposition::releases_physical_budget`] 一个布尔：
    /// 它为假（`ReturnedToPool` / `Quarantined`）⇒ 预算还占着，隧道引用**一并保留**，
    /// 连台账都不碰；它为真（`Closed`）⇒ 预算此刻核销，隧道引用在**同一刻**归还。
    ///
    /// 全模块只有**两个调用方法**：`release` 与 `force_close`。方法是两个，但
    /// **调用表达式有三处**：`release` 内一处、`force_close` 的 `Closed` 分支一处、
    /// `force_close` 的 `Quarantined` 分支一处 —— 处置为 `Quarantined`，`settle` 首行
    /// 就按 `releases_physical_budget()` 为假原样返回 `Retained`）。
    /// 三处读的是同一个判据，所以不存在某条路径上两侧错拍的可能。
    pub(super) fn settle_tunnel_reference(
        &mut self,
        lease_id: &LeaseId,
        disposition: CleanupDisposition,
    ) -> TunnelDisposition {
        // 预算还占着 ⇒ 隧道引用也还占着。隔离不是释放，是「先把账留着，等确认」。
        if !disposition.releases_physical_budget() {
            return TunnelDisposition::Retained;
        }
        let Some(ledger) = self.tunnels.as_mut() else {
            return TunnelDisposition::NoReference;
        };
        // 一条资源归还 ⇒ 恰好一次释放。重复调用返回 `None`，零副作用（幂等）。
        match ledger.return_resource(lease_id) {
            Some(release) => TunnelDisposition::Released {
                tunnel_closed: release.closed,
            },
            None => TunnelDisposition::NoReference,
        }
    }

    /// CM-27 的握手 / 初始化 / 注册三个阶段失败时的补偿。
    ///
    /// 物理资源已经开出来了、隧道引用也已经落账了，但这三步任何一步没过，
    /// 连接就**从来没能对外可用**。此时要做的不是「留着等复位」，而是把已经建立的
    /// 东西按建立顺序的逆序拆掉：关掉物理连接，再归还它那份隧道引用
    /// （CM-27「隧道开成后回滚释放」）。
    ///
    /// 关闭本身若未确认，`force_close` 会把租约留在 `Quarantined`，此时引用**保留** ——
    /// 隔离中的连接可能仍在用这条隧道，配对不许塌。
    ///
    /// 隧道引用由 `force_close` 内部的两个分支结算，本函数**不再重复结算**：
    /// `TunnelLedger::return_resource` 虽然幂等，但「释放点字面数得清」这条纪律
    /// 靠的是数得清（两个调用方法、三处调用表达式），不是靠幂等兜底。变异实测（删掉 `force_close` 的结算）会直接
    /// 让握手/初始化/注册回滚用例转红，证明回滚路径确实由 `force_close` 兜着。
    pub fn roll_back_unpublished(&mut self, lease_id: &LeaseId) -> Result<(), ResourceError> {
        self.table.lease(lease_id).ok_or_else(|| {
            ResourceError::UnknownResource(lease_id.as_str().to_owned())
                .logged("rollback targets a lease the resource table does not hold")
        })?;
        // 关闭未确认时 `force_close` 记成 `Quarantined` 并 `Err`，但那是**处置**不是错误：
        // 预算未核销、隧道引用由它一并保留，回滚义务已经落到那一行租约上了。
        let _unconfirmed = self.force_close(lease_id);
        Ok(())
    }

    /// 隧道阶段失败后的补偿：把刚开出来的 socket 关掉。
    ///
    /// 此时隧道引用**压根没落账**（[`Self::acquire_tunnel_reference`] 失败即不落账），
    /// 所以补偿里没有任何台账动作可做 —— 这正是「建隧道失败不落账」的可观察面。
    ///
    /// 关闭未确认时把租约**隔离留存**：预算占用与关闭义务都不能凭空消失
    /// （connection-management.md §9.4「任一失败都关闭」、§10.1.1「确认关闭或节点隔离后才核销」），
    /// 留一行 `Quarantined` 才能让后续 `retire` 重试这次关闭。
    pub(super) fn compensate_tunnel_stage_failure(
        &mut self,
        lease: &LeaseRecord,
        cause: ResourceError,
    ) -> ResourceError {
        match self.transport.close(&lease.resource_id) {
            Ok(()) => cause,
            Err(error) => {
                let mut retained = lease.clone();
                retained.state = LeaseState::Quarantined;
                self.table.insert(lease.resource_id.clone(), retained);
                tracing::warn!(
                    target: "datazen_runtime::resource",
                    lease = %lease.lease_id.as_str(),
                    reason = error.reason(),
                    "tunnel stage failed and the compensating close is unconfirmed; \
                     the physical resource is retained quarantined so the close obligation survives"
                );
                cause.logged("tunnel stage failed and its compensating close is unconfirmed")
            }
        }
    }
}

/// 把隧道侧的失败原因压成一条稳定的日志尾巴（不外泄 spec 明文之外的任何东西）。
fn error_detail(error: &crate::tunnel::TunnelError) -> &'static str {
    match error {
        crate::tunnel::TunnelError::Transport { .. } => "tunnel transport refused the open",
        crate::tunnel::TunnelError::NotShareable { .. } => "tunnel is not shareable",
        crate::tunnel::TunnelError::AlreadyHeld { .. } => "lease already holds this tunnel",
        crate::tunnel::TunnelError::RevisionUnavailable => "tunnel route revision unavailable",
    }
}
