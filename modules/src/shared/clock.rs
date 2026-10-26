//! `Clock`：时基端口。
//!
//! **平台包层**的墙上时刻端口：需要墙上时刻的组件收 [`Clock`]（`Arc<dyn Clock>` 或
//! `&dyn Clock`），生产装配注入 [`SystemClock`]，测试注入 [`FixedClock`]。`platform-api`
//! 里其余与时间有关的公开类型只有 [`Timestamp`]（只读字符串）与预算账本内部注入的时间闭包。
//!
//! 措辞刻意收窄，`grep -rn "fn now" packages --include=*.rs` 就能看到它**不是**「全仓唯一的
//! 时间契约」：`packages/` 下还有 runtime 的 `JobClock`（UTC `Timestamp`，见 `runtime::job::time`）、
//! `DirectoryClock` / `BudgetClock` / gateway timing（量的是**单调经过时间**，不是墙上时刻）、
//! 以及 data-sync host 端口的 `now`（ISO-8601 字符串）。它们面向各自用例的词汇，本端口不取代；
//! `packages/` 下另有 6 处生产路径直读墙上时钟，逐条登记在
//! `docs/architecture/platform/shared-boundaries-and-ports.md` §4.9。
//!
//! ## 为什么是同步方法、不走 `#[async_trait]`
//!
//! §4.1 的约定（`#[async_trait]` + `Send + Sync + 'static`）针对**会等待、会失败**的
//! 端口；读一次系统时间既不等待也不失败，因此 [`Clock::now`] 是同步方法且不返回
//! `Result`：没有哪个 `PortError` 取值能表示「时钟坏了」，硬造一个只会让每个调用点
//! 写一次无意义的 `?`。对象安全与 `Arc<dyn Clock>` 注入不受影响。
//!
//! ## 返回值只有一种形状
//!
//! [`Clock::now`] 返回平台 ID newtype [`Timestamp`]，写成**定宽 RFC3339（UTC，毫秒）**
//! 的 `2026-01-01T00:00:00.000Z`。`Timestamp` 的比较是字符串序，所以同一条时间线
//! 上的生产方必须落同一种形状：秒级（`…T00:00:00Z`）与毫秒级（`…T00:00:00.000Z`）
//! 混在一列里会让字典序与真实先后不一致（同一整秒内 `'.'` 0x2E < `'Z'` 0x5A，
//! 即 `…T00:00:00Z` 排在 `…T00:00:00.500Z` 之后），排序与时序判定随之不可靠；某条既有比较
//! 是否因此翻转取决于比较式两侧的形状（`expires_at < now` 这种「左秒级、右毫秒级」在整秒处
//! **不**翻转，实测见 §4.9）。本端口只**生产**时刻、不解析时刻——需要算术的调用方自己解析。
//!
//! ## 为什么实现体随端口一起放在契约层
//!
//! §4 的口径是「端口 trait 只定义签名，实现由提供方承担」。时基本身是**平台中立的
//! 原语**：不存在「服务端时钟」与「桌面时钟」两种语义，也不持有任何传输或存储，
//! 因此本文件随端口给出两个实现——[`SystemClock`]（生产）与 [`FixedClock`]（测试替身），
//! 与 `ports::budget::pool_ledger` 的进程内参考账本同一处理方式（先例见
//! `InMemoryDriverPoolBudget`，本条是第二处）：省掉每个形态各写一份格式化的机会，也就省掉了
//! 各写一种形状的机会。这是 §4 开头那条规则的**例外**（规则约束的是带传输 / 存储 / 副作用的
//! 适配器实现）；任何**带副作用或带传输**的端口实现仍然必须留在提供方 crate。

use std::sync::{Arc, Mutex};

use crate::shared::id::Timestamp;

/// 时间读取端口。
pub trait Clock: Send + Sync + 'static {
    /// 当前时刻，定宽 RFC3339（UTC，毫秒精度）。
    fn now(&self) -> Timestamp;
}

/// 系统时钟：平台包层**经 [`Clock`] 端口**读取系统时间的唯一实现。
///
/// 措辞刻意收窄——**不是**「全仓唯一读系统时间的实现」：`packages/` 下仍有 6 处
/// 生产路径直读墙上时钟未收口（driver-api 转储头、runtime 的 application/operations、
/// application/tokens、artifact、directory 生产时钟、redis 驱动 pubsub），
/// 宿主 `src-tauri/` 另有约 40 处；逐条登记在
/// `docs/architecture/platform/shared-boundaries-and-ports.md` §4.9。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::new(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    }
}

/// 固定时钟：测试替身，时刻由测试显式给定，可再 [`FixedClock::set`] 推进。
///
/// 它**不解析**时刻：传进来什么字符串就回什么字符串，与 `Timestamp` 的契约一致。
#[derive(Debug, Clone)]
pub struct FixedClock {
    now: Arc<Mutex<Timestamp>>,
}

impl FixedClock {
    /// 固定到某个时刻。
    pub fn at(ts: impl Into<String>) -> Self {
        Self {
            now: Arc::new(Mutex::new(Timestamp::new(ts))),
        }
    }

    /// 推进到下一个时刻。克隆共享同一份状态，推进后所有持有者一起看到新值。
    pub fn set(&self, ts: impl Into<String>) {
        let mut guard = self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Timestamp::new(ts);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 调用点只认 `&dyn Clock`：换成哪个实现是装配层的事。
    fn stamp(clock: &dyn Clock) -> String {
        clock.now().as_str().to_string()
    }

    #[test]
    fn a_fixed_clock_replaces_the_system_clock_behind_the_same_call() {
        let fixed = FixedClock::at("2026-01-01T00:00:00.000Z");
        assert_eq!(stamp(&fixed), "2026-01-01T00:00:00.000Z");

        // 固定时钟的每一步都在测试的掌控内：同一个调用点第二次取到的是推进后的值。
        fixed.set("2026-01-01T00:01:30.000Z");
        assert_eq!(stamp(&fixed), "2026-01-01T00:01:30.000Z");
    }

    #[test]
    fn a_clone_of_a_fixed_clock_observes_the_same_advance() {
        let fixed = FixedClock::at("2026-01-01T00:00:00.000Z");
        let shared = fixed.clone();
        fixed.set("2026-01-01T00:02:00.000Z");
        assert_eq!(shared.now().as_str(), "2026-01-01T00:02:00.000Z");
    }

    #[test]
    fn the_system_clock_writes_the_single_fixed_width_shape() {
        let now = SystemClock.now();
        // 定宽是字典序比较的前提：毫秒精度、UTC、长度不随时刻变化。
        assert_eq!(now.as_str().len(), "2026-01-01T00:00:00.000Z".len());
        assert!(now.as_str().ends_with('Z'), "{}", now.as_str());
        assert_eq!(&now.as_str()[19..20], ".");
        let parsed =
            chrono::DateTime::parse_from_rfc3339(now.as_str()).expect("system clock is RFC3339");
        assert_eq!(parsed.offset().local_minus_utc(), 0);
        // 与真实时间同源：不是某个固定常量。
        let drift = chrono::Utc::now()
            .signed_duration_since(parsed)
            .num_seconds();
        assert!(drift.abs() <= 5, "system clock drifted by {drift}s");
    }

    #[test]
    fn the_port_is_injectable_as_a_trait_object() {
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::at("2026-02-03T04:05:06.000Z"));
        assert_eq!(clock.now().as_str(), "2026-02-03T04:05:06.000Z");
    }
}
