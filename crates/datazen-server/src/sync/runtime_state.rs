//! Data Sync 进程级运行态（W8 刀3e-3 立仓储；3e-4 的执行宿主是唯一生产者）。
//!
//! 桌面对照 `commands/sync/host/state.rs` 的进程内静态表（ARTIFACTS /
//! SELECTIONS / PENDING_PREVIEWS / FAILURES——单租户进程全局）。服务端
//! 投影为**装配态实例**（composition root 注入，测试可换实例；桌面静态
//! 表的 Rust 对应物，语义逐条对齐）：
//!
//! - **PENDING_PREVIEWS**：一次对比的 durable 计划已受理、但行级复核
//!   投影（plan_store 的行负荷）还在物化中——`preview` 读在 60s 窗口内
//!   等待生产者完成，而不是发布陈旧的「计划不可用」。
//! - **FAILURES**：宿主侧最后观测到的失败消息。durable 记录只落安全
//!   原因码（`JobView.error`），真消息（连接串、驱动原文）由宿主侧表
//!   补——`get` / `details` 读回时对 Failed 态覆写。
//!
//! ChangeSet Artifact 与已确认 selection 的进程内注册表**不在本件**：
//! 它们挂在共享层 [`SyncPlanRepository`]（组织键隔离的端口，装配态注入
//! ——刀3e-4 接线），不是宿主私产。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

/// 装配态运行态（克隆共享内部表）。
#[derive(Clone, Default)]
pub struct SyncRuntimeState {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    pending_previews: HashSet<String>,
    failures: HashMap<String, String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl SyncRuntimeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 对比复核投影开始物化（生产者：3e-4 的对比受理）。
    pub fn preview_started(&self, job_id: &str) {
        lock(&self.inner).pending_previews.insert(job_id.to_owned());
    }

    /// 对比复核投影物化完成（无论成败都摘挂起——读侧不再等待）。
    pub fn preview_finished(&self, job_id: &str) {
        lock(&self.inner).pending_previews.remove(job_id);
    }

    /// `preview` 读侧的等待判定（桌面 `preview_pending` 同语义）。
    pub fn preview_pending(&self, job_id: &str) -> bool {
        lock(&self.inner).pending_previews.contains(job_id)
    }

    /// 宿主侧记录最后观测到的失败消息（生产者：3e-4 的执行宿主）。
    pub fn record_failure(&self, job_id: &str, message: String) {
        lock(&self.inner)
            .failures
            .insert(job_id.to_owned(), message);
    }

    /// Failed 态读回的安全消息覆写（桌面 `safe_failure_of` 同语义；
    /// 无记录 → `None`，`JobView.error` 原样）。
    pub fn safe_failure_of(&self, job_id: &str) -> Option<String> {
        lock(&self.inner).failures.get(job_id).cloned()
    }
}
