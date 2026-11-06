//! server 形态的空闲回收驱动器（IdleReclaimer 武装刀，设计档
//! `docs/development/w8-idle-reclaimer-arming-design.md` §3.4/§4.2 落码）。
//!
//! ## 语义参照与唯一差别
//!
//! 语义参照共享内核 `idle.rs` 的桌面驱动器（`start_idle_reclaimer`）：
//! 回收前的一切安全检查（dispatch hold 暂停期限、替换屏障、活句柄拒绝、
//! 保守处置）全部已在内核 `reclaim_idle()`，适配器**零额外闸**。唯一差别：
//! **不绑死实例**——每 tick 经 `RuntimeLeaseSource::resolve()` 现取**当前**
//! 实例。直接用共享驱动器会（§3.4 的两个后果）：①吊销后 cell 摘除、
//! 实例重建，闭包里的旧 `Arc` 继续扫旧实例 =「扫了个寂寞」；②旧实例被
//! 闭包钉死在空中永不析构。
//!
//! ## 生命周期
//!
//! bootstrap 在租约源与全部消费者装配**之后** spawn（无反序窗口）；
//! `axum::serve` 的 graceful drain 完成后 `stop()`（幂等）；`Drop` 兜底
//! （stop + abort，与共享 `IdleReclaimer` 同款）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use datazen::execution::application::orchestration::IdlePolicy;

use crate::execution::lease::{RuntimeLeaseSource, ServerRuntimeLeaseSource};

/// server 空闲回收驱动器句柄。形状镜像共享 `IdleReclaimer`（字段私有、
/// `stop`/`is_finished`/`Drop` 三件套）——不自建共享抽象（共享句柄无公开
/// 构造器，第三形态出现再议，设计档 §4.2）。
pub struct ServerIdleReclaimer {
    stop: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl ServerIdleReclaimer {
    /// 起跑驱动循环。`policy` 是**装配期常量**：间隔取
    /// `policy.scan_interval()`，循环内不重读（重读语义归下一实例的
    /// `with_idle_policy`——驱动器只负责「现取实例」，不负责「现读配置」）。
    pub fn spawn(lease: Arc<ServerRuntimeLeaseSource>, policy: IdlePolicy) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let task = {
            let stop = Arc::clone(&stop);
            let interval = policy.scan_interval();
            tokio::spawn(async move {
                loop {
                    // 先睡后扫：启动即扫一轮的窗口收益为负（实例出生时
                    // deadline 未到），与共享驱动器同款。
                    tokio::time::sleep(interval).await;
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    // 现取当前实例（R5 护栏）：吊销→重建后天然跟随新实例；
                    // 旧实例不被本循环钉死。resolve 失败不致死循环（当前
                    // 实现不会失败；保守 continue 等下一 tick）。
                    let runtime = match lease.resolve().await {
                        Ok(runtime) => runtime,
                        Err(source) => {
                            tracing::warn!(source = %source, "idle reclaimer resolve failed; skipping tick");
                            continue;
                        }
                    };
                    let reclaimed = runtime.reclaim_idle().await;
                    if !reclaimed.reclaimed.is_empty() {
                        tracing::info!(
                            count = reclaimed.reclaimed.len(),
                            "idle reclaimer evicted idle runtime sessions"
                        );
                    }
                    for id in &reclaimed.interrupted_borrows {
                        tracing::warn!(
                            db_session_id = id.as_str(),
                            "idle eviction interrupted live connection borrows"
                        );
                    }
                }
            })
        };
        Self { stop, task }
    }

    /// 请求停止并在下一轮循环边界退出（幂等）。
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// 扫描任务是否已经退出（不阻塞、不 join；`JoinHandle::is_finished`
    /// 的可测纪律，与共享句柄同款）。
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl Drop for ServerIdleReclaimer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.task.abort();
    }
}
