//! 基准计划与判据常量 —— 逐字对应 `fake-runtime-fixtures.md` §11.1。
//!
//! 这些数字**没有一个是默认值**。判据把并发、预热、轮次、每轮样本数与 fake 命令
//! 耗时都写死了，基准默认值就必须是它们本身；任何「更小的默认值」都会让
//! 「跑出来的 p95 达标」这句话指向另一个实验。这里没有留缩减计划的余地：
//! 想跑小计划必须显式传 CLI 参数，而那样产物里 `conformsToSpec` 会是 `false`。

use std::time::Duration;

/// §11.3 的门禁：逐轮 p95 ≤ 10 毫秒。
pub const GATE_P95_NANOS: u64 = 10_000_000;

/// §11.1 指定的环境：4 vCPU。
///
/// **这是可复现性锚点，不是容量要求。** 文档没有为这两个数字给出任何理由；
/// 它们的约束力来自 §11.3「不删失败样本 + 逐轮判定」——基准必须能把噪声和真实回归
/// 分开，而分不开的量测没有意义。因此本 harness **不**把环境写进判定式：
/// 判定式里只有「逐轮 p95」与「失败数」两件。实测环境原样写进产物 `environment` 段，
/// 在别的机器上跑出来的数字要能被读成「这台机器上跑出来的数字」，而不是「达标」。
pub const SPEC_VCPUS: u32 = 4;

/// §11.1 指定的环境：8 GiB（以字节为单位）。
pub const SPEC_MEMORY_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// §11.1 固定的 fake 命令耗时（**虚拟时间**消费，见 [`crate::driver`]）。
pub const SPEC_FAKE_COMMAND: Duration = Duration::from_millis(10);

/// §11.1 的并发度。
pub const SPEC_CONCURRENCY: usize = 8;
/// §11.1 的预热请求数。
pub const SPEC_WARMUP: usize = 1_000;
/// §11.1 的每轮请求数（已获准且未排队）。
pub const SPEC_PER_ROUND: usize = 10_000;
/// §11.1 的轮次。
pub const SPEC_ROUNDS: usize = 5;

/// 一次基准的执行计划。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct BenchPlan {
    /// 预热请求数（§11.1：1000）。预热样本**不进**分位数，也不进产物。
    pub warmup: usize,
    /// 每轮请求数（§11.1：10000）。
    pub per_round: usize,
    /// 轮次（§11.1：5）。
    pub rounds: usize,
    /// 并发任务数（§11.1：并发 8）。
    pub concurrency: usize,
    /// fake 命令耗时（§11.1：10 毫秒，虚拟时间）。
    pub fake_command: Duration,
}

impl BenchPlan {
    /// 整个计划一共打多少发请求：预热 + 轮次 × 每轮。
    pub fn throughput(&self) -> usize {
        self.warmup
            .saturating_add(self.per_round.saturating_mul(self.rounds))
    }

    /// 逐字等于 §11.1 的规格吗？
    ///
    /// 不等就不许自称「按判据测的」——产物里的 `conformsToSpec`、stdout 的措辞、
    /// 以及退出码都挂在这个判定上。缩小计划能跑通，但会被如实标成「非规格运行」。
    pub fn is_spec_plan(&self) -> bool {
        *self == SPEC_PLAN
    }

    /// 缩小计划用的显式改写器。
    ///
    /// 只给产物自身的自测与「想快点看一眼趋势」的场景用；任何一条改写都会让
    /// [`BenchPlan::is_spec_plan`] 变成 `false`，从而让产物、stdout 与退出码一起
    /// 拒绝给出「按判据达标」的结论。**没有**绕过这个标志的口子。
    pub fn with_rounds(self, rounds: usize) -> Self {
        Self { rounds, ..self }
    }

    pub fn with_per_round(self, per_round: usize) -> Self {
        Self { per_round, ..self }
    }

    pub fn with_warmup(self, warmup: usize) -> Self {
        Self { warmup, ..self }
    }

    pub fn with_concurrency(self, concurrency: usize) -> Self {
        Self { concurrency, ..self }
    }

    pub fn with_fake_command(self, fake_command: Duration) -> Self {
        Self { fake_command, ..self }
    }
}

/// 判据 §11.1 指定的计划，逐字。
pub const SPEC_PLAN: BenchPlan = BenchPlan {
    warmup: SPEC_WARMUP,
    per_round: SPEC_PER_ROUND,
    rounds: SPEC_ROUNDS,
    concurrency: SPEC_CONCURRENCY,
    fake_command: SPEC_FAKE_COMMAND,
};

/// 默认计划 = 规格计划。没有「更保守的默认值」这种自我宽容的口子。
pub const DEFAULT_PLAN: BenchPlan = SPEC_PLAN;

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_runtime::latency::nearest_rank_percentile;

    /// 门禁常量与判据字面一致：10 毫秒；fake 命令 10 毫秒。
    #[test]
    fn gate_and_fake_command_match_the_criterion_literal() {
        assert_eq!(GATE_P95_NANOS, 10_000_000);
        assert_eq!(SPEC_FAKE_COMMAND, Duration::from_millis(10));
    }

    /// 可复现性锚点只用于披露，不参与判定：换机器不应改变判定式的形状。
    #[test]
    fn spec_anchors_are_disclosed_not_enforced() {
        assert_eq!(SPEC_VCPUS, 4);
        assert_eq!(SPEC_MEMORY_BYTES, 8_589_934_592);
    }

    /// 预热 1000 / 每轮 10000 / 5 轮 / 并发 8 —— 默认值必须逐字等于判据。
    #[test]
    fn default_plan_is_the_criterion_plan() {
        assert_eq!(SPEC_PLAN.warmup, 1_000);
        assert_eq!(SPEC_PLAN.per_round, 10_000);
        assert_eq!(SPEC_PLAN.rounds, 5);
        assert_eq!(SPEC_PLAN.concurrency, 8);
        assert_eq!(SPEC_PLAN.fake_command, Duration::from_millis(10));
        assert_eq!(DEFAULT_PLAN, SPEC_PLAN);
        assert_eq!(DEFAULT_PLAN.throughput(), 1_000 + 5 * 10_000);
        assert!(DEFAULT_PLAN.is_spec_plan());
    }

    /// 缩小计划能跑，但**必须**被标成非规格运行，否则缩小结果会被误读成按判据测的。
    #[test]
    fn a_smaller_plan_is_not_a_spec_plan() {
        let smoke = BenchPlan {
            warmup: 4,
            per_round: 8,
            rounds: 2,
            concurrency: 2,
            fake_command: SPEC_FAKE_COMMAND,
        };
        assert!(!smoke.is_spec_plan());
        assert_eq!(smoke.throughput(), 20);
    }

    /// nearest-rank 是唯一口径：N=10000 时取第 `ceil(0.95*N)=9500` 项（1-based，下标 9499），
    /// 不插值。本基准复用的是 `latency` 里那一份实现，这里只是把口径钉在判据的 N 上。
    #[test]
    fn p95_index_is_nearest_rank_not_interpolated() {
        let samples: Vec<u64> = (1..=10_000u64).collect();
        assert_eq!(nearest_rank_percentile(&samples, 0.95), Some(9_500));
    }

    /// 空样本集返回 `None`：判定的上游必须把「没测到」和「0 毫秒」分开。
    #[test]
    fn empty_samples_are_never_zero() {
        assert_eq!(nearest_rank_percentile(&[], 0.95), None);
    }
}