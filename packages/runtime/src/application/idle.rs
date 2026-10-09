//! 空闲回收：把「开了却不设期限」这件事从**建好就废**改成**接线即生效**。
//!
//! 基线上 [`crate::registry::OpenRequest::idle_deadline_ms`] 在应用层恒为 `None`，
//! 于是 [`crate::registry::registry::SessionRegistry::evict_idle_at`] 的第二道闸门
//! （未设期限 ⇒ `Ok(None)`）永远成立：回收器**建好了，却一次也够不着会话**。
//! 本模块是那条缺失的接线。
//!
//! # 三件事必须一起做，缺一件就是半成品
//!
//! 1. **设期限**：[`RuntimeConnectionUseCases::create_session`] 在建会话时按
//!    [`IdlePolicy`] 把绝对毫秒写进 `OpenRequest.idle_deadline_ms`。
//! 2. **驱动扫描**：[`RuntimeConnectionUseCases::start_idle_reclaimer`] 起一个周期任务。
//!    这是**应用层自己拥有**的驱动器，宿主不需要（也不应该）再起一个定时器去驱动它。
//! 3. **清应用层台账**：registry 驱逐完成后，目录附件、`sessions` 行、结果 sink、活连接借用
//!    这四样都还在应用层手里。只做 1+2 会留下一批「registry 里没了、目录里还在」的僵尸登记——
//!    比不回收更坏，因为它既占额度又查得到。
//!
//! # 一处已知缺口（不掩饰）
//!
//! 文档 §6.4 要求「执行中 ⇒ 暂停空闲判定」。执行中这一条**由 registry 侧结构性满足**：
//! `ExecCommand::Evict` 与 `Execute` 走同一条 FIFO 命令通道，执行在飞时 `Evict` 进
//! `deferred` 队列（`registry/actor.rs` 的 `handle_exec_while_flying`），因此驱逐不会与
//! 执行并发。
//!
//! 但「暂停后**顺延**」做不到：应用层**只能在建会话时设一次期限**，因为
//! `SessionRegistry` 没有暴露 `set_idle_deadline` 的公开包装（`registry/actor.rs` 里的
//! `fn set_idle_deadline` 是私有的，唯一调用者是它自己的命令分发）。要补齐这一格需要
//! registry 侧加一个公开包装——`registry/**` 不在本轨可写范围，故此处如实记录而非变通。
//! 同一缺口也让「有活连接借用在外的会话」只能事后作废（[`crate::application::live`]），
//! 而不是事前阻止驱逐。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use datazen_platform_api::ports::session_directory::{CloseDisposition, SessionDirectory};

use super::{results::lock, RuntimeConnectionUseCases};
use crate::connection::DbSessionId;

/// §9.2：业务空闲 30 分钟。扫描间隔取其 1/10，让最坏多留 3 分钟。
const DEFAULT_DEADLINE: Duration = Duration::from_secs(30 * 60);
const DEFAULT_SCAN_INTERVAL: Duration = Duration::from_secs(3 * 60);

/// 空闲回收策略。
///
/// `deadline == None` 是**配置事实**（本部署不做空闲回收），不是「计数器为 0」：
/// 两种 `None` 语义不混同，是 [`crate::budget`] / [`crate::resource`] 一直在守的规矩。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdlePolicy {
    deadline: Option<Duration>,
    scan_interval: Duration,
}

impl Default for IdlePolicy {
    fn default() -> Self {
        Self {
            deadline: Some(DEFAULT_DEADLINE),
            scan_interval: DEFAULT_SCAN_INTERVAL,
        }
    }
}

impl IdlePolicy {
    /// 不做空闲回收：`idle_deadline_ms` 恒为 `None`，任何扫描都不会驱逐任何会话。
    pub fn disabled() -> Self {
        Self {
            deadline: None,
            scan_interval: DEFAULT_SCAN_INTERVAL,
        }
    }

    /// 自定义期限与扫描间隔。`Duration::ZERO` 的期限会让会话一开就到期，由调用方自负。
    pub fn with_deadline(deadline: Duration, scan_interval: Duration) -> Self {
        Self {
            deadline: Some(deadline),
            scan_interval,
        }
    }

    /// 建会话时写入 `OpenRequest.idle_deadline_ms` 的毫秒数；`None` = 不设期限。
    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline
            .map(|deadline| u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX))
    }

    pub fn scan_interval(&self) -> Duration {
        self.scan_interval
    }

    pub fn is_enabled(&self) -> bool {
        self.deadline.is_some()
    }
}

/// 一次回收扫描的结果投影。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdleReclamation {
    /// 本次真正被驱逐的会话。
    pub reclaimed: Vec<DbSessionId>,
    /// 被驱逐时**仍有活连接借用在外**的会话：引用已被作废，但宿主当时并不知道。
    /// 这是 §6.4「顺延」缺口的可观测痕迹，不是错误。
    pub interrupted_borrows: Vec<DbSessionId>,
}

/// 周期扫描器句柄。`stop` 与 `Drop` 都停它，且都幂等。
pub struct IdleReclaimer {
    stop: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl IdleReclaimer {
    /// 请求停止并在下一轮循环边界退出。
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for IdleReclaimer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.task.abort();
    }
}

impl RuntimeConnectionUseCases {
    /// 一次空闲扫描。
    ///
    /// 时间由 [`super::RuntimeClock`] 注入，与写入 `OpenRequest.idle_deadline_ms` 的是
    /// **同一个**时钟、**同一个**单位（单调纳秒截成毫秒），因此到期判定在测试里完全确定。
    ///
    /// 本方法**不做**任何跳过：registry 的驱逐是全表扫描，本层没有「只驱逐某几个」的接口，
    /// 也没有权限替它加。逐会话的差别只能体现在 `physical.is_none()` / 未设期限 / 未到期
    /// 这三道闸门里，而它们都在 registry 内部，本层不需要重复判断。
    pub async fn reclaim_idle(&self) -> IdleReclamation {
        let at_ms = self.inner.clock.now_ms();
        let evicted = self.inner.registry.evict_idle_at(at_ms).await;
        let mut result = IdleReclamation::default();
        let live = lock(&self.inner.live).clone();
        for view in evicted {
            let db_session_id = view.handle.db_session_id.clone();
            // 顺序要紧：先作废借用，再摘应用层登记。反过来的话，
            // 在两次语句之间完成的归还会漏掉作废，持有者就拿着一份「没被作废」的引用去
            // 驱动上打请求。
            let interrupted = live.revoke(&db_session_id);
            let entry = lock(&self.inner.sessions).remove(&db_session_id);
            if let Some(entry) = entry.as_ref() {
                let _ = self
                    .inner
                    .directory
                    .release(&entry.handle, CloseDisposition::Closed)
                    .await;
                self.inner.sink.invalidate(&entry.handle);
            }
            if interrupted > 0 {
                tracing::warn!(
                    db_session_id = db_session_id.as_str(),
                    borrows = interrupted,
                    "空闲回收打断了仍在外的活连接借用：引用已作废，宿主必须重新走执行路径"
                );
                result.interrupted_borrows.push(db_session_id.clone());
            }
            result.reclaimed.push(db_session_id);
        }
        result
    }

    /// 起一个周期扫描器。
    ///
    /// 驱动器归应用层所有：**宿主不需要**再为它挂一个定时器，也就没有「两个驱动器
    /// 抢同一个 registry 表」的形状。
    pub fn start_idle_reclaimer(self: &Arc<Self>) -> IdleReclaimer {
        let interval = self.idle_policy().scan_interval();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let service = Arc::clone(self);
        let task = tokio::spawn(async move {
            while !flag.load(Ordering::SeqCst) {
                tokio::time::sleep(interval).await;
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                let reclaimed = service.reclaim_idle().await;
                if !reclaimed.reclaimed.is_empty() {
                    tracing::info!(count = reclaimed.reclaimed.len(), "空闲回收：驱逐到期会话");
                }
            }
        });
        IdleReclaimer { stop, task }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 默认策略是接线的且期限为三十分钟() {
        let policy = IdlePolicy::default();
        assert!(policy.is_enabled());
        assert_eq!(
            policy.deadline_ms(),
            Some(u64::try_from(DEFAULT_DEADLINE.as_millis()).expect("在 u64 范围内"))
        );
        assert_eq!(policy.scan_interval(), DEFAULT_SCAN_INTERVAL);
    }

    #[test]
    fn 关闭策略写不出期限因此永不触发() {
        let policy = IdlePolicy::disabled();
        assert!(!policy.is_enabled());
        assert_eq!(policy.deadline_ms(), None);
    }

    #[test]
    fn 自定义策略同时改期限与扫描间隔() {
        let policy = IdlePolicy::with_deadline(Duration::from_millis(30), Duration::from_millis(5));
        assert_eq!(policy.deadline_ms(), Some(30));
        assert_eq!(policy.scan_interval(), Duration::from_millis(5));
    }
}
