//! FakeScript —— 故障与延迟注入（fake-runtime-fixtures.md §4.1 F1–F12）。
//!
//! **归属说明（R4）**：`fake-runtime-fixtures.md` §2 的模块表里**没有** `faults.rs` 这一行，
//! 而 §3.1 把 `FakeScript` 定义为 `FakeResourceProvider` 的一个**字段**
//! （`FakeResourceProvider { script, clock, ids, resources, journal, id_counter, worker_id }`），
//! §2 的 DAG 边也只有 `H → P` 与 `P → S[FakeScript faults & delays]`。
//! 因此故障注入**结构性地归属于 `fake_resource`**，不存在需要另行归并的 `faults.rs`。
//! 本文件就是那张目录里 `S` 节点的落点：`ops.rs` 的九个操作每次都先问 `FakeScript` 要不要故障。
//!
//! §5.3 / §6 的断言之所以仍然可表达，是因为脚本只影响**操作的返回值**，
//! 而 §5.3 的变化点断言与 §4.3 的 I1–I8 全部读 journal —— journal 由 `ops.rs`
//! 在**注入故障的同时**照常写入，于是「故障下的台账」和「基线下的台账」走同一条断言路径。
//! `#[test] the_change_point_rules_stay_expressible_under_injected_faults` 正是这条性质的守门测试。
//!
//! §13 日志脱敏：脚本里只有**脚本 id**（`&'static str`，如 `F9/commitUnknown`），
//! 没有任何字面量凭据入口；`attachmentToken` / 幂等 nonce 不经过本模块。

use std::sync::Mutex;
use std::time::Duration;

use crate::connection::execution::ExecutionErrorCode;
use crate::connection::port::ResetDiscardReason;

/// 九个资源层操作中可被脚本命中的一种（fake-runtime-fixtures.md §3.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceOp {
    Describe,
    Acquire,
    Execute,
    Observe,
    ChangeContext,
    Transaction,
    RequestCancel,
    Reset,
    Close,
}

impl ResourceOp {
    pub const ALL: [ResourceOp; 9] = [
        ResourceOp::Describe,
        ResourceOp::Acquire,
        ResourceOp::Execute,
        ResourceOp::Observe,
        ResourceOp::ChangeContext,
        ResourceOp::Transaction,
        ResourceOp::RequestCancel,
        ResourceOp::Reset,
        ResourceOp::Close,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ResourceOp::Describe => "describeResource",
            ResourceOp::Acquire => "acquireResource",
            ResourceOp::Execute => "executeOnResource",
            ResourceOp::Observe => "observeSession",
            ResourceOp::ChangeContext => "changeContext",
            ResourceOp::Transaction => "transactionOperation",
            ResourceOp::RequestCancel => "requestCancel",
            ResourceOp::Reset => "resetResource",
            ResourceOp::Close => "closeResource",
        }
    }
}

/// 故障种类，**按 `docs/architecture/platform/fake-runtime-fixtures.md` §4.1 的 F1–F12 归位**。
///
/// 每个变体带两行机器可读的属性，缺一不可：
/// - 首行 `/// F<n>：…` 给出 §4.1 的编号；
/// - 另起一行 `/// 目录归属：<阶段>` 给出 §4.1 表格「阶段」单元格的原文
///   （在第一个全角 `（` 处截断：F1「描述（`describeResource`）」→「描述」）。
///
/// §4.1 是唯一真源，编号与阶段都由
/// `fake_resource::catalog_guard` 在测试运行时**解析文档表格**逐条对撞，
/// 任何一侧单独改动都会打红。
#[derive(Debug, Clone, PartialEq)]
pub enum FaultKind {
    /// F1：执行计划不支持，**派发前**拒绝（`ApiErrorCode::is_pre_dispatch_rejection`）。
    ///
    /// 目录归属：描述
    UnsupportedPlan,
    /// F2：预算占满。`acquireResource` 报 `ResourceBusy`，超过给定 TTL 后 `Timeout`。
    ///
    /// 目录归属：预算申请
    BudgetBusy {
        busy_for: Duration,
        reason: &'static str,
    },
    /// F3：连接或初始化失败（`ConnectAndInit` / `SqlError` 二选一由 `code` 决定）。
    ///
    /// 目录归属：建连与初始化
    ConnectAndInit { code: &'static str },
    /// F4：语句**派发之后**失败。
    ///
    /// 目录归属：语句派发
    StatementDispatch { code: &'static str },
    /// F5：结果传输中断 —— 协议未排空，必须上报 `protocol_drained = false`。
    ///
    /// 目录归属：结果传输
    ResultTransport { code: &'static str },
    /// F6：观测不可判定 —— `observeSession` 三个字段全 `unknown`。
    ///
    /// 目录归属：观察
    ObserveUnknown,
    /// F7：上下文切换冲突 —— 期望重读后由用户重发（§4.2 F7）。
    ///
    /// 目录归属：上下文切换
    ContextConflict,
    /// F7：上下文切换要求替换资源。这是 `Ok` 值，不是错误（§3.2 L138）。
    ///
    /// 目录归属：上下文切换
    RequiresReplacement { reason: String },
    /// F8：提交结果未知。§3.2：此时 `effectOutcome` **必须**是 `unknown`。
    ///
    /// 目录归属：事务
    CommitUnknown { code: &'static str },
    /// F8：回滚失败 —— 资源隔离，预算占用**保留**。
    ///
    /// 目录归属：事务
    RollbackFailed { reason: String },
    /// F9：取消被拒。`CancelReceipt.disposition=unsupported` 是**正常返回值**（§4.2 F9）。
    ///
    /// 目录归属：取消
    CancelRejected { code: &'static str },
    /// F10：driver 报 `Clean`，但宿主前置条件（句柄登记 / 事务未终结 / 协议未排空）并不满足。
    ///
    /// 目录归属：重置归池
    CleanButPreconditionUnmet,
    /// F10：driver 报 `Discard`，`reason` 由用例指定（§4.2：归池前置不满足只能丢，不能报 `Clean`）。
    ///
    /// 目录归属：重置归池
    ResetDiscard { reason: ResetDiscardReason },
    /// F10：reset 在期限内不返回 —— 宿主只能看到「归池未完成」，**不得**当成 `Clean`。
    ///
    /// 目录归属：重置归池
    ResetTimeout,
    /// F11：关闭未确认 —— 预算占用**保留**，资源仍占用。
    ///
    /// 目录归属：关闭
    CloseUnconfirmed { reason: &'static str },
    /// F11：关闭期间资源丢失 —— 资源进 `Lost`，后续执行一律被拒。
    ///
    /// 目录归属：关闭
    LostDuringClose { reason: &'static str },
    /// F12：句柄造出来了，runtime **拒绝**把它交给宿主；fake 侧标记 `orphaned`。
    /// §4.3 的 I7 在关闭回收之前必然不成立（§4.2 F12）。
    ///
    /// 目录归属：句柄登记
    HandleNotReturned { reason: &'static str },
    /// F12：把上一个 epoch 的句柄拿到当前资源上用 —— 必须先在 epoch 门闸上失败，
    /// 而不是放行或在资源 id 上乱报错。
    ///
    /// 目录归属：句柄登记
    CrossEpochHandleReuse,
}

impl FaultKind {
    /// §4.1 的编号，供测试与报告直接引用。
    ///
    /// 取值以 `catalog_guard` 解析出来的 §4.1 表格为准：这里只放编号，
    /// 编号与阶段由测试逐条对撞文档。
    pub const fn catalog_id(&self) -> &'static str {
        match self {
            // F1 描述 → F4 语句派发
            FaultKind::UnsupportedPlan => "F1",
            FaultKind::BudgetBusy { .. } => "F2",
            FaultKind::ConnectAndInit { .. } => "F3",
            FaultKind::StatementDispatch { .. } => "F4",
            FaultKind::ResultTransport { .. } => "F5",
            FaultKind::ObserveUnknown => "F6",
            // F7 上下文切换：`ContextConflict` 与 `requiresReplacement` 同属一行
            FaultKind::ContextConflict => "F7",
            FaultKind::RequiresReplacement { .. } => "F7",
            // F8 事务：`commit` 返回 `Unknown` 与 `rollback` 失败同属一行
            FaultKind::CommitUnknown { .. } => "F8",
            FaultKind::RollbackFailed { .. } => "F8",
            FaultKind::CancelRejected { .. } => "F9",
            // F10 重置归池
            FaultKind::CleanButPreconditionUnmet => "F10",
            FaultKind::ResetDiscard { .. } => "F10",
            FaultKind::ResetTimeout => "F10",
            // F11 关闭
            FaultKind::CloseUnconfirmed { .. } => "F11",
            FaultKind::LostDuringClose { .. } => "F11",
            // F12 句柄登记
            FaultKind::HandleNotReturned { .. } => "F12",
            FaultKind::CrossEpochHandleReuse => "F12",
        }
    }
}

/// 一条已排队的注入步骤。`id` 是**脚本 id**（允许进 journal），不是字面量凭据（§13）。
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptStep {
    pub id: String,
    pub op: ResourceOp,
    pub kind: FaultKind,
    pub remaining: usize,
}

/// §9.3 CM-73「先挂起被驱逐的资源、再在其上开事务」竞态脚本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictionRace {
    /// 驱逐流程在**预关闭检查点**停住时到达的 barrier tag。
    pub pre_close: String,
    /// 放行关闭的 barrier tag。
    pub release_close: String,
    /// 为 `true` 时，`closeResource` 必须**先**在 R1 上回滚并注销会话句柄，然后才释放 R1。
    /// 这就是 §9.3 第 (4) 步要求的正确行为。
    pub rollback_before_release: bool,
}

impl EvictionRace {
    /// CM-73 固定使用的两个 barrier tag。写死成常量，避免测试与实现各写一份字符串。
    pub const PRE_CLOSE: &'static str = "cm73/pre-close";
    pub const RELEASE_CLOSE: &'static str = "cm73/release-close";
}

/// 故障脚本。
#[derive(Debug, Default)]
pub struct FakeScript {
    steps: Mutex<Vec<ScriptStep>>,
    /// 关闭路径是否必须先回滚 + 注销句柄（§9.3 第 (4) 步）。
    rollback_before_release: Mutex<bool>,
}

impl FakeScript {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ScriptStep>> {
        self.steps
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn flag(&self) -> std::sync::MutexGuard<'_, bool> {
        self.rollback_before_release
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 排入一个命中 `n` 次的步骤。`id` 只作为脚本 id 使用，惯例形如 `F9/commitUnknown`。
    pub fn push(&self, op: ResourceOp, kind: FaultKind, id: impl Into<String>, n: usize) {
        if n == 0 {
            return;
        }
        self.lock().push(ScriptStep {
            id: id.into(),
            op,
            kind,
            remaining: n,
        });
    }

    /// 排入一个只命中一次的步骤。
    pub fn once(&self, op: ResourceOp, kind: FaultKind) {
        // `id` 里要用到 `kind`，所以先取出 catalog_id 再把 `kind` 整体移交给 `push`。
        let id = format!("{}/{}", kind.catalog_id(), op.as_str());
        self.push(op, kind, id, 1);
    }

    /// 是否要求关闭前先回滚 + 注销（§9.3）。
    pub fn rollback_before_release(&self) -> bool {
        *self.flag()
    }

    pub fn set_rollback_before_release(&self, value: bool) {
        *self.flag() = value;
    }

    /// 取下一个命中 `op` 的故障并消耗一次额度。没有则返回 `None`（基线路径）。
    pub fn take(&self, op: ResourceOp) -> Option<(String, FaultKind)> {
        let mut steps = self.lock();
        let step = steps
            .iter_mut()
            .find(|step| step.op == op && step.remaining > 0)?;
        step.remaining -= 1;
        let kind = step.kind.clone();
        let id = step.id.clone();
        steps.retain(|step| step.remaining > 0);
        Some((id, kind))
    }

    /// `op` 上还剩多少次故障，用于断言脚本被完整消费。
    pub fn pending(&self, op: ResourceOp) -> usize {
        self.lock()
            .iter()
            .filter(|step| step.op == op)
            .map(|step| step.remaining)
            .sum()
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    // ---- §9.3 CM-73 ----

    /// `script_hold_for_eviction_then_begin_commit()` 的脚本侧：置位「关闭前先回滚注销」，
    /// 并返回驱动整个竞态的两个 barrier tag。
    pub fn hold_for_eviction_then_begin_commit(&self) -> EvictionRace {
        self.set_rollback_before_release(true);
        EvictionRace {
            pre_close: EvictionRace::PRE_CLOSE.to_owned(),
            release_close: EvictionRace::RELEASE_CLOSE.to_owned(),
            rollback_before_release: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fault_catalogue_collides_with_the_parsed_f1_through_f12_table() {
        crate::connection::testing::fake_resource::catalog_guard::assert_catalogue_matches_doc();
    }

    #[test]
    fn a_step_is_consumed_exactly_as_many_times_as_it_was_queued() {
        let script = FakeScript::new();
        script.push(
            ResourceOp::Execute,
            FaultKind::StatementDispatch { code: "SqlError" },
            "F4/statement",
            2,
        );
        assert_eq!(script.pending(ResourceOp::Execute), 2);
        assert!(script.take(ResourceOp::Execute).is_some());
        assert!(script.take(ResourceOp::Execute).is_some());
        assert!(script.take(ResourceOp::Execute).is_none());
        assert_eq!(script.pending(ResourceOp::Execute), 0);
    }

    #[test]
    fn a_fault_only_fires_on_its_own_operation() {
        let script = FakeScript::new();
        script.once(
            ResourceOp::Close,
            FaultKind::CloseUnconfirmed {
                reason: "关闭握手超时",
            },
        );
        assert!(script.take(ResourceOp::Execute).is_none());
        assert!(script.take(ResourceOp::Close).is_some());
    }

    #[test]
    fn the_cm73_script_requires_rollback_before_release() {
        let script = FakeScript::new();
        assert!(!script.rollback_before_release());
        let race = script.hold_for_eviction_then_begin_commit();
        assert!(race.rollback_before_release);
        assert!(script.rollback_before_release());
        assert_eq!(race.pre_close, EvictionRace::PRE_CLOSE);
        assert_eq!(race.release_close, EvictionRace::RELEASE_CLOSE);
    }

    #[test]
    fn every_operation_has_a_stable_name() {
        for op in ResourceOp::ALL {
            assert!(!op.as_str().is_empty());
        }
    }
}

/// 把脚本里的 `&'static str` 错误码 id 映射到 [`ExecutionErrorCode`]。
///
/// `FaultKind` 的 `code` 存的是**稳定 id**（报告与断言里要出现稳定字符串），
/// 而 `ExecutionErrorCode::as_str()` 给的是 camelCase，两种写法都收。
pub(super) fn execution_error_code(id: &str) -> ExecutionErrorCode {
    match id {
        "SqlError" | "sqlError" => ExecutionErrorCode::SqlError,
        "ProtocolError" | "protocolError" => ExecutionErrorCode::ProtocolError,
        "Cancelled" | "cancelled" => ExecutionErrorCode::Cancelled,
        "Timeout" | "timeout" => ExecutionErrorCode::Timeout,
        "ResourceLost" | "resourceLost" => ExecutionErrorCode::ResourceLost,
        "PipelineAborted" | "pipelineAborted" => ExecutionErrorCode::PipelineAborted,
        "HostRejected" | "hostRejected" => ExecutionErrorCode::HostRejected,
        // 夹具作者写错了 id。整棵 fake 模块都被
        // `#[cfg(any(test, feature = "test-harness"))]` 门控，不是生产路径，
        // 所以让错误在源头炸掉，而不是悄悄降级成一个错误的错误码。
        other => panic!("FakeScript 注入的未知错误码 id：{other}"),
    }
}
