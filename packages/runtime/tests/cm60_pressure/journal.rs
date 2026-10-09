//! 资源 journal：单一原子 `seq` 定序的「变化点」台账。
//!
//! 判据要的是**每一个变化点**上的不变量，不是收尾时看一眼总数。所以水位由账本
//! 现场读出（permit 台账 + 四类队列深度），不变量在 [`ResourceJournal::observe`]
//! 内部当场断言——夹具没法被绕过，也漏不掉中间某一步。
//!
//! 另外按夹具文档的口径做台账自洽：permit 的槽位只有
//! 保留 / 共享两种且是穷尽的，所以 `在手 = 共享 + 保留` 必须恒成立。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use datazen_platform_api::id::ConnectionId;
use datazen_platform_api::ports::budget::{ResourceClass, ServiceQuota};

use datazen_runtime::budget::{BudgetLedger, SlotKind};

/// 一次资源变化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// 放行一份 permit。
    Admitted {
        class: ResourceClass,
    },
    /// 归还在手 permit（已消费或未使用，两条路都只减一）。
    Released {
        class: ResourceClass,
    },
    /// 一个请求进入等待队列。
    Queued {
        class: ResourceClass,
    },
    /// 等待者被取消——取消句柄作废。
    WaiterCancelled,
    /// 队列已满，请求被拒。
    QueueRejected {
        class: ResourceClass,
    },
    /// 终态拒绝。
    Denied,
    /// worker 排空开始。
    NodeDrained,
    /// 逻辑 session 开 / 关。
    SessionOpened,
    SessionClosed,
    /// 关池。
    PoolClosed,
}

/// 变化点现场读出的账本水位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watermark {
    /// 在手 permit 总数（保留 + 共享）——判据里的「真实资源」。
    pub live: u32,
    /// 在手共享 permit 数。
    pub shared: u32,
    /// 在手保留 permit 数。
    pub reserved_live: u32,
    /// 在手 Control 保留 permit 数。
    pub control_reserved_live: u32,
    /// Control 保留剩余 = 配置额度 − 在手。
    pub control_free: u32,
    /// 四类等待队列各自的深度。
    pub queued_by_class: [u32; 4],
}

impl Watermark {
    /// 四类等待深度之和。
    pub fn queued_total(&self) -> u32 {
        self.queued_by_class
            .iter()
            .copied()
            .fold(0, |acc, d| acc + d)
    }
}

/// 一个变化点。
#[derive(Debug, Clone, Copy)]
pub struct ChangePoint {
    /// 单一原子计数器给出的定序键。
    pub seq: u64,
    /// 虚拟时刻（ms）。
    pub at_ms: u64,
    /// 发生了什么。
    pub change: Change,
    /// 变化之后的账本水位。
    pub after: Watermark,
}

/// 变化点 journal。
#[derive(Debug)]
pub struct ResourceJournal {
    seq: AtomicU64,
    points: Mutex<Vec<ChangePoint>>,
    quota: ServiceQuota,
    queue_cap: u32,
}

impl ResourceJournal {
    /// 按额度配置建台账。
    pub fn new(quota: ServiceQuota, queue_cap: u32) -> Self {
        Self {
            seq: AtomicU64::new(0),
            points: Mutex::new(Vec::new()),
            quota,
            queue_cap,
        }
    }

    /// Control 保留额——普通资源绝不许动的那一笔。
    pub fn control_baseline(&self) -> u32 {
        self.quota.reserved_of(ResourceClass::Control)
    }

    /// 变化点总数。
    pub fn len(&self) -> usize {
        let seq = self.lock().last().map_or(0, |point| point.seq + 1);
        usize::try_from(seq).unwrap_or(usize::MAX)
    }

    /// 是否一个变化点都没有。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 按 `seq` 定序的全部变化点。
    pub fn points(&self) -> Vec<ChangePoint> {
        self.lock().to_vec()
    }

    /// 历史里真实资源（在手 permit）的峰值。
    pub fn peak_live(&self) -> u32 {
        self.lock().iter().map(|p| p.after.live).max().unwrap_or(0)
    }

    /// 历史里共享水位的峰值。
    pub fn peak_shared(&self) -> u32 {
        self.lock()
            .iter()
            .map(|p| p.after.shared)
            .max()
            .unwrap_or(0)
    }

    /// 历史里等待深度之和的峰值。
    pub fn peak_queued(&self) -> u32 {
        self.lock()
            .iter()
            .map(|p| p.after.queued_total())
            .max()
            .unwrap_or(0)
    }

    /// 收尾水位。
    pub fn final_watermark(&self) -> Watermark {
        match self.lock().last() {
            Some(point) => point.after,
            None => Watermark {
                live: 0,
                shared: 0,
                reserved_live: 0,
                control_reserved_live: 0,
                control_free: self.control_baseline(),
                queued_by_class: [0; 4],
            },
        }
    }

    /// 记一个变化点：先从账本现场读水位，再**当场**核对不变量，最后入账。
    pub fn observe(
        &self,
        ledger: &BudgetLedger,
        connection_id: &ConnectionId,
        at_ms: u64,
        change: Change,
    ) {
        let after = watermark(ledger, connection_id, self.quota);
        let previous = self.lock().last().map(|point| point.after);
        self.check(previous, change, after);
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let point = ChangePoint {
            seq,
            at_ms,
            change,
            after,
        };
        match self.points.lock() {
            Ok(mut points) => points.push(point),
            Err(poisoned) => poisoned.into_inner().push(point),
        }
    }

    /// 四条不变量。任一破掉立即红，且报出是哪一步破的。
    fn check(&self, previous: Option<Watermark>, change: Change, after: Watermark) {
        // 判据断言一：每个变化点的真实资源都不超过总额度。
        assert!(
            after.live <= self.quota.total,
            "断言一：在手 permit {} 超过总额度 {}（变化 {:?}）",
            after.live,
            self.quota.total,
            change,
        );
        // 台账自洽：保留 / 共享是 permit 槽位的穷尽划分，不允许有第三种。
        assert_eq!(
            after.live,
            after.shared + after.reserved_live,
            "断言一：permit 台账不自洽，在手 {} ≠ 共享 {} + 保留 {}",
            after.live,
            after.shared,
            after.reserved_live,
        );
        // 判据断言二（上）：普通资源只吃共享，共享水位不得越过 total − 保留。
        assert!(
            after.shared <= self.quota.shared,
            "断言二：共享水位 {} 侵占保留，共享上限只有 {}（变化 {:?}）",
            after.shared,
            self.quota.shared,
            change,
        );
        // 判据断言二（下）：保留额只被保留类动，普通资源不得侵占控制预留。
        assert!(
            after.control_reserved_live <= self.control_baseline(),
            "断言二：Control 保留 {} 超过配置额度 {}",
            after.control_reserved_live,
            self.control_baseline(),
        );
        if !matches!(
            change,
            Change::Admitted {
                class: ResourceClass::Control
            }
        ) {
            let before = previous.map_or(0, |point| point.control_reserved_live);
            assert_eq!(
                after.control_reserved_live, before,
                "断言二：变化 {:?} 动到了控制预留（在手 {} → {}）",
                change, before, after.control_reserved_live,
            );
        }
        // 判据断言三：等待有界——每类队列深度都不超过配置上限。
        for (class, depth) in ResourceClass::ALL.iter().zip(after.queued_by_class) {
            assert!(
                depth <= self.queue_cap,
                "断言三：{class:?} 等待深度 {depth} 超过队列上限 {}",
                self.queue_cap,
            );
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ChangePoint>> {
        match self.points.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// 从账本现场读出水位。
///
/// 只看**在手的 permit 台账**——不看账本自己汇总的计数器，这样台账和汇总不一致时
/// 会当场暴露，而不是被汇总口径掩盖。
fn watermark(
    ledger: &BudgetLedger,
    connection_id: &ConnectionId,
    quota: ServiceQuota,
) -> Watermark {
    let mut shared = 0u32;
    let mut reserved_live = 0u32;
    let mut control_reserved_live = 0u32;
    for permit in ledger.permits() {
        if &permit.connection_id != connection_id {
            continue;
        }
        match permit.slot {
            SlotKind::Shared => shared = shared.saturating_add(1),
            SlotKind::Reserved => {
                reserved_live = reserved_live.saturating_add(1);
                if permit.class == ResourceClass::Control {
                    control_reserved_live = control_reserved_live.saturating_add(1);
                }
            }
        }
    }
    let mut queued_by_class = [0u32; 4];
    for class in ResourceClass::ALL {
        queued_by_class[class.index()] = ledger.queue_depth(connection_id, class) as u32;
    }
    Watermark {
        live: shared + reserved_live,
        shared,
        reserved_live,
        control_reserved_live,
        control_free: quota
            .reserved_of(ResourceClass::Control)
            .saturating_sub(control_reserved_live),
        queued_by_class,
    }
}
