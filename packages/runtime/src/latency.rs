//! 门禁开销的**测量口径**（CM-60，[夹具 §11.3](../../architecture/platform/fake-runtime-fixtures.md#113-统计口径)）。
//!
//! 这里是口径本身，不是基准工具：它只回答「一组样本怎么变成一个门禁数」。
//! 跑基准的是 `src/bin/cm60-bench`，§11.5 的产物输出也在那个入口里。
//!
//! ## 口径（不可协商）
//!
//! 1. **先逐请求求和，再取分位数**：`overhead = gateway + registration`，
//!    然后对 `overhead` 样本取 p95。**不得**把两段的 p95 相加代替，
//!    **也不得**让两段各自达标就判整体通过。
//! 2. 取 **nearest-rank**：升序排序后取第 `ceil(q * N)` 项（1-based）。
//! 3. 本模块**没有「失败」这个概念**：它只对交给它的向量排序。凡是打不出两段完整
//!    时长的请求（派发失败、超时、被取消）根本进不了这个向量——harness 把它们记在
//!    `unmeasured_failures` 里并因此把门禁判红，**不允许**拿 0、超时值、上一段的值
//!    或任何构造值补进来占位。被拒与排队同理：它们从未获准，不进 N。
//!
//! 这条边界划清了本模块能证明什么、不能证明什么：它能证明「**已经在向量里**」的样本
//! 一个都不被丢弃；它证不了「harness 真的把失败请求算进了 N」——那要拿真实 harness
//! 验，见 `src/bin/cm60-bench/runner/tests.rs`。把后者交给本模块证明，等于用一条恒真的
//! 测试给记账开绿灯。
//!
//! 两段都用 `std::time::Instant` 采样（§11.2），不受 FakeClock 虚拟时间影响，
//! 因此本模块只处理纳秒整数，不引入任何时钟。

/// 单个请求的两段单调耗时。`u64` 纳秒，和 `std::time::Instant` 的差值同量纲。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestOverhead {
    pub gateway_nanos: u64,
    pub registration_nanos: u64,
}

impl RequestOverhead {
    pub const fn new(gateway_nanos: u64, registration_nanos: u64) -> Self {
        Self {
            gateway_nanos,
            registration_nanos,
        }
    }

    /// 该请求的门禁开销。**逐请求求和发生在取分位数之前**（§11.3）。
    pub const fn total_nanos(self) -> u64 {
        self.gateway_nanos + self.registration_nanos
    }
}

/// nearest-rank 分位数：`ceil(q * N)` 项（1-based），升序。
///
/// 空样本返回 `None`——门禁**不把「没测到」当成 0 通过**。
pub fn nearest_rank_percentile(samples: &[u64], quantile: f64) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    debug_assert!(
        (0.0..=1.0).contains(&quantile),
        "quantile out of range: {quantile}"
    );
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    // ceil(q * N)，1-based，最小为 1。
    let rank = (quantile * n as f64).ceil().max(1.0) as usize;
    sorted.get(rank - 1).copied()
}

/// 按 §11.3 口径求门禁 p95：先逐请求求和，再取 p95。
pub fn overhead_p95(samples: &[RequestOverhead]) -> Option<u64> {
    let totals: Vec<u64> = samples.iter().map(|sample| sample.total_nanos()).collect();
    nearest_rank_percentile(&totals, 0.95)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEN_MS: u64 = 10_000_000;

    #[test]
    fn nearest_rank_uses_ceil_of_q_times_n_one_based() {
        // N=10, q=0.95 → ceil(9.5)=10 → 第 10 项（最大值）。
        let samples: Vec<u64> = (1..=10).collect();
        assert_eq!(nearest_rank_percentile(&samples, 0.95), Some(10));
        // N=100, q=0.95 → ceil(95)=95 → 第 95 项。
        let samples: Vec<u64> = (1..=100).collect();
        assert_eq!(nearest_rank_percentile(&samples, 0.95), Some(95));
        // 与输入顺序无关：分位数读的是集合，不是时序。
        let shuffled: Vec<u64> = vec![9, 1, 7, 3, 5];
        assert_eq!(nearest_rank_percentile(&shuffled, 0.5), Some(5));
    }

    #[test]
    fn an_empty_sample_set_yields_no_verdict_rather_than_zero() {
        // 「没测到」不等于「0 毫秒通过」。
        assert_eq!(nearest_rank_percentile(&[], 0.95), None);
        assert_eq!(overhead_p95(&[]), None);
    }

    #[test]
    fn summing_two_p95s_is_not_the_same_number_as_p95_of_per_request_sums() {
        // 两段的大开销落在**不同请求**上：只有逐请求求和才看得出真实分布。
        let samples: Vec<RequestOverhead> = (0..10)
            .map(|i| match i {
                0 => RequestOverhead::new(100, 1),
                9 => RequestOverhead::new(1, 100),
                _ => RequestOverhead::new(1, 1),
            })
            .collect();
        let gateway_p95 = nearest_rank_percentile(
            &samples.iter().map(|s| s.gateway_nanos).collect::<Vec<_>>(),
            0.95,
        )
        .unwrap_or_default();
        let registration_p95 = nearest_rank_percentile(
            &samples
                .iter()
                .map(|s| s.registration_nanos)
                .collect::<Vec<_>>(),
            0.95,
        )
        .unwrap_or_default();
        let naive_sum = gateway_p95 + registration_p95;

        // 逐请求求和后的 p95 是 101；两段 p95 相加得到 200，是错的。
        assert_eq!(overhead_p95(&samples), Some(101));
        assert_eq!(naive_sum, 200);
        assert_ne!(overhead_p95(&samples), Some(naive_sum));
    }

    #[test]
    fn both_segments_passing_is_not_enough_for_the_combined_gate() {
        // 20 个请求，rank = ceil(0.95*20) = 19。
        // 各自的大开销落在**不同请求**上：第 0 个只有网关段贵，第 19 个只有登记段贵。
        let mut samples = vec![RequestOverhead::new(1_000_000, 1_000_000); 20];
        samples[0] = RequestOverhead::new(9_500_000, 1_000_000);
        samples[19] = RequestOverhead::new(1_000_000, 9_500_000);

        // 两段单独的分位数都只有 1ms，**各自都低于 10ms 门槛**。
        let gateway_p95 = nearest_rank_percentile(
            &samples.iter().map(|s| s.gateway_nanos).collect::<Vec<_>>(),
            0.95,
        )
        .unwrap_or_default();
        let registration_p95 = nearest_rank_percentile(
            &samples
                .iter()
                .map(|s| s.registration_nanos)
                .collect::<Vec<_>>(),
            0.95,
        )
        .unwrap_or_default();
        assert!(gateway_p95 <= TEN_MS);
        assert!(registration_p95 <= TEN_MS);

        // 但整体 p95 是 10.5ms，**不达标**。
        let combined = overhead_p95(&samples).unwrap_or_default();
        assert_eq!(combined, 10_500_000);
        assert!(combined > TEN_MS, "combined p95 must fail the gate");

        // 关键：把两段 p95 相加只得到 2ms，会把这次不合格判成合格。
        assert_eq!(
            gateway_p95 + registration_p95,
            2_000_000,
            "两段 p95 相加会洗白整体"
        );
    }

    #[test]
    fn the_percentile_keeps_every_sample_it_is_given() {
        // 100 个样本：大开销交替落在网关段 / 登记段上，因此两段的 p95 各自
        // 都只有 1ms，只有逐请求求和才看得出整体已经超标（N=100 → rank 95）。
        // 这条只证明「交给分位数的样本一条都不被丢」——向量里有什么就是算什么。
        let mut samples: Vec<RequestOverhead> = (0..94)
            .map(|_| RequestOverhead::new(1_000_000, 1_000_000))
            .collect();
        for i in 0..6 {
            samples.push(if i % 2 == 0 {
                RequestOverhead::new(900_000_000, 1_000_000)
            } else {
                RequestOverhead::new(1_000_000, 900_000_000)
            });
        }
        let p95 = overhead_p95(&samples).unwrap_or_default();
        assert_eq!(p95, 901_000_000);
        assert!(p95 > TEN_MS);
    }

    #[test]
    fn a_shortened_input_really_does_wash_the_percentile_down() {
        // 同上那 100 个样本，但只把前 94 个交给分位数。**这不是本仓库允许的口径**：
        // 它之所以被写成测试，是为了让「缩短输入会洗白 p95」这件事有数字，而不是
        // 一句提醒——提醒会随迭代蒸发，这个数不会。
        //
        // 谁都不许拿它当许可：失败的请求**留在 N 里**（见
        // `cm60-bench::outcome::RoundOutcome::n`），分位数则只对真的打完了两段计时的
        // 那批算；这条边界的**执行**由真实 harness 守住，见
        // `cm60-bench/runner/tests.rs` 的
        // `the_recorded_p95_is_the_percentile_of_the_samples_this_run_measured`。
        let samples: Vec<RequestOverhead> = (0..94)
            .map(|_| RequestOverhead::new(1_000_000, 1_000_000))
            .collect();
        let washed = overhead_p95(&samples).unwrap_or_default();
        assert_eq!(washed, 2_000_000);
        assert!(washed <= TEN_MS, "这就是被禁止的洗白形状");
    }
}
