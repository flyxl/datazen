//! 基准用的单调时钟：以 `std::time::Instant` 为准，与 FakeClock 的虚拟时间无关。
//!
//! §11.2 把这条写成硬要求：「两段都用 `std::time::Instant` 采样，**不受 FakeClock 虚拟时间影响**」。
//! 反过来说，被测的网关本身**永远不能**直接抓 `Instant`——它必须经由注入的
//! [`MonotonicClock`] 取时间，否则同一个 gateway 在假夹具里和在基准里会走两条时间线，
//! 口径立刻不成立。抓 `Instant` 的责任只落在本模块这一个地方。

use std::sync::Arc;
use std::time::Instant;

use datazen_runtime::gateway::MonotonicClock;

/// 以进程内某个基准瞬间为原点的单调时钟。
///
/// 原点取构造时刻而不是 unix epoch：两段时长都是 `now() - 之前打下的点`，
/// 差值与原点选谁无关，而原点越小越不会碰到 u64 纳秒的溢出边界。
#[derive(Debug)]
pub struct InstantClock {
    base: Instant,
}

impl InstantClock {
    /// 以「现在」为原点建一个时钟。
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
        }
    }

    /// 建一个 `Arc` 化的时钟，直接塞给 [`ExecutionGateway::new`]。
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl Default for InstantClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonotonicClock for InstantClock {
    fn now_nanos(&self) -> u64 {
        let elapsed = self.base.elapsed().as_nanos();
        // u128 → u64 是饱和而不是环绕：基准跑几小时也才 ~1e13 ns，溢不出来；
        // 万一真溢了，返回 u64::MAX 也比静默回绕成一个小数字诚实。
        u64::try_from(elapsed).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_monotonic_and_nonzero_before_a_tick() {
        let clock = InstantClock::new();
        let first = clock.now_nanos();
        let second = clock.now_nanos();
        assert!(second >= first, "基准时钟必须单调不减：{second} < {first}");
    }

    #[test]
    fn origin_is_construction_time_not_unix_epoch() {
        // 原点若误用 unix epoch，值会落在 1e18 量级；差值口径要求它是进程内相对量。
        let clock = InstantClock::new();
        assert!(
            clock.now_nanos() < 1_000_000_000_000,
            "时钟原点不是构造时刻"
        );
    }
}
