//! 门禁的两段单调计时。
//!
//! 规定的口径是**两段**加法：
//!
//! - **段①** gateway 完成鉴权 / 参数校验结束 → 派发 driver；
//! - **段②** driver 完成 → receipt / 事件状态登记完成。
//!
//! 明确**不计入**的：`fake` SQL 实际执行、budget/actor 排队、网络传输。
//! 这三段都不是 gateway 的开销，把它们混进来会让 p95 变成端到端延迟，
//! 于是「网关变慢」和「库变慢」再也分不开。
//!
//! 百分位口径复用 [`crate::latency`]：**先逐条求和、再取 nearest-rank p95**，
//! 绝不能把两段的 p95 相加（那样是 p95(p95(a) + p95(b))，与 p95(a + b) 不是同一个量，
//! 且在重尾分布下系统性偏高）。本模块不重定义任何百分位函数。
//!
//! # 为什么这里没有 `std::time::Instant`
//!
//! 时钟是**注入**的（[`MonotonicClock`］）。原因有二：
//!
//! 1. 要求确定性时间——单测必须能在 `start_paused` 或手动推进下得到确定值，
//!    而 `Instant::now()` 会让「段① == 2ms」这种断言变成概率事件；
//! 2. `latency.rs` 的口径说明写的是「两段都用 `Instant` 采样」，那是**调用方**
//!    的选型，不是网关内核的义务。把取时点从内核里拿走，宿主层换任何时钟源
//!    都不会改动本模块的行为。
//!
//! [`FixedClock`] 是一个由调用方驱动的单调时钟：只有 [`FixedClock::advance`]
//! 会让读数前进，因此「fake SQL 花了 10ms」这种假设可以**精确地**从测量区间里排除。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::latency::RequestOverhead;

/// 单调时钟。实现方必须保证读数**单调不减**且单位为纳秒。
pub trait MonotonicClock: Send + Sync + 'static {
    fn now_nanos(&self) -> u64;
}

/// 由调用方驱动的单调时钟（`T0 = 0ns`）。
///
/// 名字里没有 `test`：它是本模块**唯一**提供的实现，也是确定性夹具与
/// 可注入生产时钟共用的实现——宿主层也可以提供一个由虚拟时间源驱动的同名语义。
#[derive(Debug, Default)]
pub struct FixedClock {
    now_nanos: AtomicU64,
}

impl FixedClock {
    /// 起始于 `0ns`，与 `connection::testing` 的 `T0_NANOS` 对齐。
    pub fn new() -> Self {
        Self::default()
    }

    /// 前进 `nanos`。时钟永不后退，因此 `now_nanos()` 单调不减。
    pub fn advance(&self, nanos: u64) {
        self.now_nanos.fetch_add(nanos, Ordering::SeqCst);
    }

    /// 供夹具复用同一把时钟句柄。
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl MonotonicClock for FixedClock {
    fn now_nanos(&self) -> u64 {
        self.now_nanos.load(Ordering::SeqCst)
    }
}

/// 一次受理-派发-登记走完留下的计时痕迹。
///
/// 只保留**边界读数**，不预先求差值：差值在 [`OverheadProbe::sample`] 里
/// 用 `saturating_sub` 计算，因此即便某个读数来源略微回退，也只会得到 0
/// 而不会得到一个巨大的负数溢出值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadProbe {
    /// 段①起点：鉴权与参数校验完成、受理回执已生成的那一刻。
    accepted_at_nanos: u64,
    /// 段①终点：即将向 driver 派发的那一刻（**在** await 之前打点）。
    dispatch_issued_nanos: Option<u64>,
    /// 段②起点：driver 调用返回的那一刻（**在** await 返回之后打点）。
    driver_completed_nanos: Option<u64>,
}

impl OverheadProbe {
    /// 打开一次探针。`clock` 是注入的单调时钟。
    pub fn accepted(clock: &dyn MonotonicClock) -> Self {
        Self {
            accepted_at_nanos: clock.now_nanos(),
            dispatch_issued_nanos: None,
            driver_completed_nanos: None,
        }
    }

    /// 段①终点打点。重复打点以第一次为准——覆盖写会让「区间」在诊断里变得不可解释。
    pub fn mark_dispatch_issued(&mut self, clock: &dyn MonotonicClock) {
        if self.dispatch_issued_nanos.is_none() {
            self.dispatch_issued_nanos = Some(clock.now_nanos());
        }
    }

    /// 段②起点打点。同样只认第一次。
    pub fn mark_driver_completed(&mut self, clock: &dyn MonotonicClock) {
        if self.driver_completed_nanos.is_none() {
            self.driver_completed_nanos = Some(clock.now_nanos());
        }
    }

    /// 段①是否已经封闭（受理与派发都打过点）。
    pub fn has_gateway_segment(&self) -> bool {
        self.dispatch_issued_nanos.is_some()
    }

    /// 段②是否已经封闭（driver 完成与登记都打过点由调用方保证）。
    pub fn has_registration_segment(&self) -> bool {
        self.driver_completed_nanos.is_some()
    }

    /// 段①起点读数（纳秒）。
    pub fn accepted_at_nanos(&self) -> u64 {
        self.accepted_at_nanos
    }

    /// 求出两段耗时。任一段尚未封闭则返回 `None`。
    ///
    /// `None` 表示「这次测量不成立」，**不是**「耗时为 0」。
    pub fn sample(&self, registration_completed_nanos: u64) -> Option<RequestOverhead> {
        let dispatch_issued = self.dispatch_issued_nanos?;
        let driver_completed = self.driver_completed_nanos?;
        Some(RequestOverhead::new(
            dispatch_issued.saturating_sub(self.accepted_at_nanos),
            registration_completed_nanos.saturating_sub(driver_completed),
        ))
    }
}

/// 样本仓库。
///
/// 语义上就是「一段插值序列」，所以直接透传 `latency::nearest_rank_percentile`，
/// 避免在网关里长出第二套分位数定义（历史上两套定义漂移过，见上面的口径说明）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OverheadSamples {
    values: Vec<RequestOverhead>,
}

impl OverheadSamples {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, sample: RequestOverhead) {
        self.values.push(sample);
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn as_slice(&self) -> &[RequestOverhead] {
        &self.values
    }

    /// 两段合计的 nearest-rank p95。
    pub fn overhead_p95(&self) -> Option<u64> {
        crate::latency::overhead_p95(&self.values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_only_moves_when_advanced() {
        let clock = FixedClock::new();
        assert_eq!(clock.now_nanos(), 0);
        clock.advance(10);
        assert_eq!(clock.now_nanos(), 10);
        clock.advance(0);
        assert_eq!(clock.now_nanos(), 10);
    }

    #[test]
    fn sample_is_none_until_both_boundaries_are_marked() {
        let clock = FixedClock::new();
        let mut probe = OverheadProbe::accepted(&clock);
        assert!(!probe.has_gateway_segment());
        assert!(!probe.has_registration_segment());
        assert!(probe.sample(0).is_none(), "未打点时必须返回 None 而不是 0");

        clock.advance(3);
        probe.mark_dispatch_issued(&clock);
        assert!(probe.has_gateway_segment());
        assert!(
            probe.sample(0).is_none(),
            "段①封闭但段②未封闭时仍然不能出样本"
        );
    }

    #[test]
    fn fake_sql_time_is_excluded_from_both_segments() {
        let clock = FixedClock::new();
        let mut probe = OverheadProbe::accepted(&clock);

        clock.advance(2);
        probe.mark_dispatch_issued(&clock);
        // fake SQL：10ms，属于 driver，不属于网关。
        clock.advance(10_000_000);
        probe.mark_driver_completed(&clock);
        clock.advance(4);
        // 登记完成。

        let sample = probe.sample(clock.now_nanos());
        let sample = match sample {
            Some(value) => value,
            None => panic!("两段都已封闭，应当出样本"),
        };
        assert_eq!(sample.gateway_nanos, 2, "段① = 鉴权/校验完成 → 派发");
        assert_eq!(sample.registration_nanos, 4, "段② = driver 完成 → 登记完成");
        assert_eq!(sample.total_nanos(), 6, "fake SQL 的 10ms 绝不能进合计");
    }

    #[test]
    fn queueing_before_acceptance_is_excluded() {
        let clock = FixedClock::new();
        // budget/actor 排队发生在受理**之前**，探针根本没打开。
        clock.advance(500);
        let mut probe = OverheadProbe::accepted(&clock);
        clock.advance(1);
        probe.mark_dispatch_issued(&clock);
        clock.advance(7);
        probe.mark_driver_completed(&clock);
        let sample = probe.sample(clock.now_nanos());
        let sample = match sample {
            Some(value) => value,
            None => panic!("两段都已封闭，应当出样本"),
        };
        assert_eq!(sample.gateway_nanos, 1);
        assert_eq!(sample.registration_nanos, 0);
    }

    #[test]
    fn repeated_marks_keep_the_first_boundary() {
        let clock = FixedClock::new();
        let mut probe = OverheadProbe::accepted(&clock);
        clock.advance(5);
        probe.mark_dispatch_issued(&clock);
        clock.advance(9);
        probe.mark_dispatch_issued(&clock);
        clock.advance(1);
        probe.mark_driver_completed(&clock);
        clock.advance(2);
        probe.mark_driver_completed(&clock);
        let sample = probe.sample(clock.now_nanos());
        let sample = match sample {
            Some(value) => value,
            None => panic!("两段都已封闭，应当出样本"),
        };
        assert_eq!(sample.gateway_nanos, 5);
        // 段②起点在第一次 mark（t=15），终点是取样时刻（t=17），二次 mark 被忽略。
        assert_eq!(sample.registration_nanos, 2);
    }

    #[test]
    fn a_backwards_reading_degrades_to_zero_instead_of_wrapping() {
        let mut probe = OverheadProbe {
            accepted_at_nanos: 100,
            dispatch_issued_nanos: Some(90),
            driver_completed_nanos: Some(80),
        };
        // 人为构造回退读数。
        probe.mark_dispatch_issued(&FixedClock::new());
        probe.mark_driver_completed(&FixedClock::new());
        let sample = probe.sample(70);
        let sample = match sample {
            Some(value) => value,
            None => panic!("两段都已封闭，应当出样本"),
        };
        assert_eq!(sample.gateway_nanos, 0);
        assert_eq!(sample.registration_nanos, 0);
    }

    #[test]
    fn p95_is_computed_over_per_request_totals() {
        let mut samples = OverheadSamples::new();
        for i in 0..100u64 {
            samples.push(RequestOverhead::new(i * 1_000, 0));
        }
        assert_eq!(samples.len(), 100);
        // nearest-rank p95 over totals (0,1000,...,99000) => rank ceil(0.95*100)=95 => 94000.
        assert_eq!(samples.overhead_p95(), Some(94_000));
    }

    #[test]
    fn empty_samples_have_no_percentile() {
        let samples = OverheadSamples::new();
        assert!(samples.is_empty());
        assert_eq!(samples.overhead_p95(), None);
    }
}
