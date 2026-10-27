//! 命令级 barrier 与协议 drain barrier。
//!
//! 依赖方向：本模块只向下依赖 `clock`（FakeClock）与非夹具的 `connection::execution`，
//! 不依赖 `fake_resource`，也不被 `journal` 依赖。
//!
//! 顺序断言的纪律：**只能**基于 journal `seq` 或本模块的 `Barrier`；
//! 「裸 `sleep(...)` 后直接断言顺序」在本模块不存在，也不应被引入。
//!
//! 按职责拆成四个文件，避免单文件超过 800 行：
//!
//! - 本文件（`Barrier` 语义）：到达凭证、序号分配、到达/等待/释放。
//! - [`drain`]：协议 drain barrier（无消费者等待、截断、期限）。
//! - [`sink`]：供 provider / 测试使用的 `ResultSink` 载体。
//! - `tests`：本模块的单测（只在 `#[cfg(test)]` 下编译）。

use std::collections::{BTreeMap, BTreeSet};
use std::panic::Location;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

mod drain;
mod sink;
#[cfg(test)]
mod tests;

pub use drain::{DrainBarrier, DrainLimits};
pub use sink::{CollectingSink, FailingSink};

// ---------------------------------------------------------------------------
// 有界等待 —— 把「挂死」变成「指名道姓的失败」
// ---------------------------------------------------------------------------

/// 一次阻塞等待允许消耗的**真实时间**上限。
///
/// `Condvar::wait` 没有超时参数：只要唤醒方永远不来（线程没被调度、某个中间人
/// 被掏成空操作、或 `FakeClock::advance` 不再推进单调时刻），整套测试就**永远不会
/// 退出** —— 那不是一次失败，而是一种无法定位的沉默。因此所有阻塞等待一律走
/// [`wait_until`]：预算耗尽就把 `MutexGuard` 原样交还，由调用点带着自己的
/// `file:line` panic。
const BLOCK_REAL_TIME_BUDGET: Duration = Duration::from_secs(30);

/// 等待 `done(state)` 成立。`Err(guard)` 表示真实时间预算已耗尽。
///
/// 预算用**真实**单调时刻计，与 `FakeClock` 完全无关：夹具自己的时钟坏了正是
/// 最需要这条兜底的情形，若预算挂在被测时钟上就会跟着一起坏掉。
fn wait_until<'g, T, F>(
    condvar: &Condvar,
    guard: MutexGuard<'g, T>,
    mut done: F,
) -> Result<MutexGuard<'g, T>, MutexGuard<'g, T>>
where
    F: FnMut(&T) -> bool,
{
    let mut state = guard;
    let deadline = Instant::now() + BLOCK_REAL_TIME_BUDGET;
    loop {
        if done(&state) {
            return Ok(state);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        // 毒化不改变语义：状态仍然是可读的，只是没有 `UnwindSafe` 保证。
        let (next, wait) = match condvar.wait_timeout(state, remaining) {
            Ok(pair) => pair,
            Err(poisoned) => poisoned.into_inner(),
        };
        state = next;
        if wait.timed_out() && Instant::now() >= deadline && !done(&state) {
            return Err(state);
        }
    }
}

// ---------------------------------------------------------------------------
// Barrier —— 命令级同步点
// ---------------------------------------------------------------------------

/// 到达凭证。`seq` 来自单一原子计数器，因此并发写入的**相对顺序是确定的**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarrierToken {
    pub seq: u64,
    pub tag: String,
}

/// 一次到达。顺序断言只读它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrival {
    pub seq: u64,
    pub tag: String,
}

#[derive(Debug, Default)]
struct BarrierState {
    arrivals: Vec<Arrival>,
    released: BTreeSet<String>,
    /// 此刻**真的**阻塞在 [`Barrier::wait_released`] 里的线程：tag -> 线程数。
    ///
    /// 只记到达是不够的：`arrive` 之后线程仍可能一路跑完，编排侧据此断言
    /// 「它停在那儿」就成了撞运气。`wait_released` 进入时登记、退出时销账，
    /// [`Barrier::wait_for_waiters`] 因此能把「阻塞生效」变成**可断言的事实**。
    waiters: BTreeMap<String, usize>,
}

struct BarrierShared {
    next_seq: AtomicU64,
    state: Mutex<BarrierState>,
    /// 每个 tag 一个条件变量唤醒集合；用单个 Condvar + 谓词重检即可，避免漏唤醒。
    condvar: Condvar,
}

impl BarrierShared {
    fn lock(&self) -> MutexGuard<'_, BarrierState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 命令级 barrier。
///
/// 语义：某条执行路径 `arrive("eviction-close-precheck")` 后**停住**，
/// 测试在拿到 `wait_for` 确认后再 `release` 它。这替代「睡一会儿猜对方跑到哪了」。
#[derive(Clone)]
pub struct Barrier {
    inner: Arc<BarrierShared>,
}

impl Barrier {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BarrierShared {
                next_seq: AtomicU64::new(1),
                state: Mutex::new(BarrierState::default()),
                condvar: Condvar::new(),
            }),
        }
    }

    /// 到达某个同步点。
    pub fn arrive(&self, tag: &str) -> BarrierToken {
        let seq = self.inner.next_seq.fetch_add(1, Ordering::SeqCst);
        {
            let mut state = self.inner.lock();
            state.arrivals.push(Arrival {
                seq,
                tag: tag.to_string(),
            });
        }
        self.inner.condvar.notify_all();
        BarrierToken {
            seq,
            tag: tag.to_string(),
        }
    }

    /// 阻塞到 `tag` 至少被到达一次。
    #[track_caller]
    pub fn wait_for(&self, tag: &str) {
        let arrived = self.inner.lock();
        let waited = wait_until(&self.inner.condvar, arrived, |s| {
            s.arrivals.iter().any(|a| a.tag == tag)
        });
        if waited.is_err() {
            let here = Location::caller();
            panic!(
                "`Barrier::wait_for({tag:?})` 在真实时间 {BLOCK_REAL_TIME_BUDGET:?} 内没有等到 `arrive`：\
                 等不到就是编排缺了一步，不是慢。调用方 {}:{}",
                here.file(),
                here.line()
            );
        }
    }

    /// 阻塞到 `tag` 被到达 `count` 次。
    #[track_caller]
    pub fn wait_for_count(&self, tag: &str, count: usize) {
        let arrived = self.inner.lock();
        let waited = wait_until(&self.inner.condvar, arrived, |s| {
            s.arrivals.iter().filter(|a| a.tag == tag).count() >= count
        });
        // 预算耗尽时 `waited` **还握着状态锁**：必须先从手里这份状态读出实际次数、
        // 放锁，再 panic。拿着锁去调 `arrival_count` 会自死锁，
        // 把本该「指名道姓的失败」变成「整套测试挂住」（实测 120s 被 SIGKILL）。
        if let Err(state) = waited {
            let actual = state.arrivals.iter().filter(|a| a.tag == tag).count();
            drop(state);
            let here = Location::caller();
            panic!(
                "`Barrier::wait_for_count({tag:?}, {count})` 在真实时间 {BLOCK_REAL_TIME_BUDGET:?} 内等不到：\
                 实际到达 {actual} 次。调用方 {}:{}",
                here.file(),
                here.line()
            );
        }
    }

    /// 释放某个同步点上阻塞的路径。
    pub fn release(&self, tag: &str) {
        {
            let mut state = self.inner.lock();
            state.released.insert(tag.to_string());
        }
        self.inner.condvar.notify_all();
    }

    /// 阻塞到 `tag` 被释放。
    #[track_caller]
    pub fn wait_released(&self, tag: &str) {
        // 登记「我停在这个 tag 上了」：编排侧据此证明阻塞真的生效。
        {
            let mut state = self.inner.lock();
            *state.waiters.entry(tag.to_string()).or_default() += 1;
        }
        self.inner.condvar.notify_all();

        let locked = self.inner.lock();
        let waited = wait_until(&self.inner.condvar, locked, |s| s.released.contains(tag));
        match waited {
            // 销账：离开等待区就不再算「停着」。
            Ok(mut state) => {
                if let Some(count) = state.waiters.get_mut(tag) {
                    *count = count.saturating_sub(1);
                }
                drop(state);
                self.inner.condvar.notify_all();
            }
            // 同样先放锁再 panic（此处没再取锁，但形状统一，免得以后有人顺手加一句读锁）。
            Err(state) => {
                drop(state);
                let here = Location::caller();
                panic!(
                    "`Barrier::wait_released({tag:?})` 在真实时间 {BLOCK_REAL_TIME_BUDGET:?} 内没有等到 `release`：\
                     释放这一侧丢了。调用方 {}:{}",
                    here.file(),
                    here.line()
                );
            }
        }
    }

    /// 此刻**真的**阻塞在 `tag` 上的线程数。
    ///
    /// 编排侧要证明「那个线程被我按在这儿了」时读它；只看 `arrival_count`
    /// 证明不了阻塞 —— 到达与停住是两件事。
    pub fn waiter_count(&self, tag: &str) -> usize {
        self.inner.lock().waiters.get(tag).copied().unwrap_or(0)
    }

    /// 阻塞到 `tag` 上至少 `count` 个线程**停在 [`Self::wait_released`] 里**。
    #[track_caller]
    pub fn wait_for_waiters(&self, tag: &str, count: usize) {
        let locked = self.inner.lock();
        let waited = wait_until(&self.inner.condvar, locked, |s| {
            s.waiters.get(tag).copied().unwrap_or(0) >= count
        });
        // 同 [`Self::wait_for_count`]：先读、先放锁、再 panic，否则 panic 里会自死锁。
        if let Err(state) = waited {
            let actual = state.waiters.get(tag).copied().unwrap_or(0);
            drop(state);
            let here = Location::caller();
            panic!(
                "`Barrier::wait_for_waiters({tag:?}, {count})` 在真实时间 {BLOCK_REAL_TIME_BUDGET:?} 内等不到：\
                 实际停在 `wait_released` 上的是 {actual} 个线程 —— 被等的那个没有真的阻塞，\
                 编排顺序就没有被钉住（`wait_released` 被架空？）。调用方 {}:{}",
                here.file(),
                here.line()
            );
        }
    }

    pub fn is_released(&self, tag: &str) -> bool {
        self.inner.lock().released.contains(tag)
    }

    pub fn arrival_count(&self, tag: &str) -> usize {
        self.inner
            .lock()
            .arrivals
            .iter()
            .filter(|a| a.tag == tag)
            .count()
    }

    /// 全部到达，按 `seq` 升序。并发写入的确定性顺序就靠它断言。
    pub fn order(&self) -> Vec<Arrival> {
        let state = self.inner.lock();
        let mut arrivals = state.arrivals.clone();
        arrivals.sort_by_key(|a| a.seq);
        arrivals
    }

    /// 断言 `a` 先于 `b` 到达。顺序只来自 `seq`，不来自挂钟。
    pub fn assert_arrived_before(&self, a: &str, b: &str) {
        let state = self.inner.lock();
        let first = state.arrivals.iter().find(|x| x.tag == a).map(|x| x.seq);
        let second = state.arrivals.iter().find(|x| x.tag == b).map(|x| x.seq);
        match (first, second) {
            (Some(x), Some(y)) => assert!(
                x < y,
                "barrier 顺序断言失败：{a}(seq={x}) 必须先于 {b}(seq={y})"
            ),
            _ => {
                panic!("barrier 顺序断言失败：{a}/{b} 未全部到达（{a}={first:?}, {b}={second:?}）")
            }
        }
    }
}

impl Default for Barrier {
    fn default() -> Self {
        Self::new()
    }
}
