//! 基准的产物数据结构：逐请求原始样本、逐轮结论、事件/许可对账、以及最终判定。
//!
//! ## 为什么这些字段是「少的几个」不行
//!
//! `fake-runtime-fixtures.md` §11.3 把每轮必须输出的东西列全了：N、p50、p90、p95、p99、
//! 最大值、失败数、排队数。少输出任何一项，读者就没法判断这一轮是不是在同一个实验里。
//! 同一节还钉死了两条口径，结构上体现为字段的分离：
//!
//! - **排队与拒绝的请求不进 p95 样本**。所以「样本集合」（[`RawSample`]）与
//!   「计数」（[`SampleOutcome`]）必须是两套东西，不能拿 `samples.len()` 当 N、
//!   也不能把失败的请求从 `samples` 里剔掉。
//! - **失败样本不删除，失败数单列**。[`SampleOutcome::failures`] 就是那一列。
//!   分位数算的是「真实跑过的那部分请求」的中位偏上水平，失败率由它自己说话。

use std::collections::BTreeMap;

use datazen_runtime::latency::nearest_rank_percentile;
use serde::Serialize;

use crate::driver::{DriverRoundTrip, ProjectionReport};
use crate::plan::{BenchPlan, GATE_P95_NANOS};

/// 一组分位数（纳秒）。
///
/// 全部是 `Option`：**空样本集是 `None`，不是 0。** 「没测到」和「0 纳秒」是两件事，
/// 写成 0 就会让判定式在一轮完全失败（所有请求都被拒）的情况下给出「通过」。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Percentiles {
    pub p50_nanos: Option<u64>,
    pub p90_nanos: Option<u64>,
    pub p95_nanos: Option<u64>,
    pub p99_nanos: Option<u64>,
    pub max_nanos: Option<u64>,
}

impl Percentiles {
    /// 对**已排序无关**的原始样本集合取 nearest-rank 分位数。
    ///
    /// 算法只有一份实现（`latency::nearest_rank_percentile`，第 `ceil(q*N)` 项、1-based、
    /// 不插值）。这里不做任何二次实现，也不缓存排序结果去算第二个口径。
    pub fn of(samples: &[u64]) -> Self {
        Self {
            p50_nanos: nearest_rank_percentile(samples, 0.50),
            p90_nanos: nearest_rank_percentile(samples, 0.90),
            p95_nanos: nearest_rank_percentile(samples, 0.95),
            p99_nanos: nearest_rank_percentile(samples, 0.99),
            max_nanos: samples.iter().copied().max(),
        }
    }

    /// 样本为空（没测到）。
    pub fn is_empty(&self) -> bool {
        self.max_nanos.is_none()
    }
}

/// 逐请求原始计时：两段纳秒值 + 轮次 + 并发度（§11.5 要求的 `raw` 段）。
///
/// 这是产物里唯一能复算分位数的材料。汇总报告可以丢，`raw` 丢了就只能重跑整个基准。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RawSample {
    pub round: usize,
    /// 该轮内的请求序号（1-based），便于把一条样本对回它的请求。
    pub ordinal: usize,
    pub gateway_nanos: u64,
    pub registration_nanos: u64,
}

/// 两段之和。**逐请求先求和，再对和取分位数**——不是两个分位数相加（§11.3）。
pub fn total_nanos(sample: &RawSample) -> u64 {
    sample
        .gateway_nanos
        .saturating_add(sample.registration_nanos)
}

/// 一轮的计数账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SampleOutcome {
    /// 本轮发出的请求数。
    pub requested: usize,
    /// 走完「受理 → 派发 → 登记」全流程的请求数。**分位数样本数应当等于它**。
    pub completed: usize,
    /// 受理被拒（含队列已满）。不进 p95 样本。
    pub rejected: usize,
    /// 受理成功但派发/登记失败。失败样本**保留**在 p95 里，失败数在这里单列。
    pub dispatch_failed: usize,
    /// 进入排队等待的请求数。**不进 p95 样本**，等待时长另报分位数。
    pub queued: usize,
    /// 幂等重发数。规格基准里恒为 0：非 0 说明幂等键构造有问题，
    /// 两次请求会落在同一个 executionId 上，事件流随即被判重复，整轮数据不可用。
    pub replays: usize,
}

impl SampleOutcome {
    /// 失败数（§11.3「失败数单列」）。判定式要求它为 0。
    pub fn failures(&self) -> usize {
        self.rejected + self.dispatch_failed
    }

    pub fn merge(&mut self, other: &SampleOutcome) {
        self.requested += other.requested;
        self.completed += other.completed;
        self.rejected += other.rejected;
        self.dispatch_failed += other.dispatch_failed;
        self.queued += other.queued;
        self.replays += other.replays;
    }
}

/// 一轮的完整结论。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoundOutcome {
    /// 1-based 轮次号。
    pub round: usize,
    /// 本轮并发度（写进 raw 样本里的那份，产物自解释）。
    pub concurrency: usize,
    /// 进 p95 样本的逐请求两段计时。
    pub samples: Vec<RawSample>,
    pub outcome: SampleOutcome,
    /// 受理被拒的原因计数（按 `GatewayError` 的分类字符串）。
    pub rejections: BTreeMap<String, usize>,
    /// 排队等待时长的分位数。空集合 → 全 `None`。
    pub queued_wait: Percentiles,
    /// 本轮样本的分位数（N = `samples.len()`）。
    pub percentiles: Percentiles,
    /// 本轮真实墙钟耗时（纳秒）。**不参与门禁**：它含 fake 命令的虚拟 10 毫秒与
    /// 事件投影，只用来解释量级，不是被测的「网关附加耗时」。
    pub wall_time_nanos: u64,
    /// 本轮事件投影（重复/丢失必须为 0，§11.1 性能门槛）。
    pub event_projection: ProjectionReport,
}

impl RoundOutcome {
    /// 进入 p95 样本的请求数，即 §11.3 要求输出的 N。
    pub fn n(&self) -> usize {
        self.samples.len()
    }

    pub fn p95_nanos(&self) -> Option<u64> {
        self.percentiles.p95_nanos
    }

    pub fn failures(&self) -> usize {
        self.outcome.failures()
    }

    /// 逐轮门禁：p95 ≤ 10 毫秒（§11.3），**且**失败数为 0（§11.6）。
    ///
    /// 「没测到」返回 false：空样本集的 p95 是 `None`，不是 0，不会被当成通过。
    pub fn passes_gate(&self) -> bool {
        match self.p95_nanos() {
            Some(p95) => p95 <= GATE_P95_NANOS && self.failures() == 0,
            None => false,
        }
    }

    /// 完成数与样本数不一致，或出现重发 → 本轮数据不可用。
    pub fn sample_count_mismatch(&self) -> bool {
        self.outcome.completed != self.n() || self.outcome.replays != 0
    }
}

/// 网关侧的执行台账对账（`applied == admitted − completed`）。
///
/// §11.5 要求 journal 摘要包含 permit 收支对账（§5.3）使基准兼作泄漏检测。
/// **诚实的边界**：网关路径上没有预算台账（`ExecutionGateway` 不持有 `BudgetLedger`），
/// 因此基准自己算不了 permit 收支；它能算的是执行台账的收支。
/// 真正的 permit 对账在压力半（`tests/cm60_pressure_drain.rs` 的 `ResourceJournal`，
/// 在**每一个** connect/close/permit 变化点断言），§11.4 要求压力与延迟分开跑。
/// 这一块把那个位置原样记进产物，让读产物的人不必再回去搜。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermitReconciliation {
    /// 网关执行台账的收支是否平（终态 outstanding 必须为 0）。
    pub gateway_ledger_balanced: bool,
    /// 压力半的 permit 对账由谁承担、怎么复现（§11.4 + §5.3）。
    pub budget_permit_ledger_carrier: String,
    pub budget_permit_ledger_command: String,
}

/// 事件与执行台账的摘要（§11.5 的 `journal` 段）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecutionJournal {
    pub admitted: usize,
    pub dispatched: usize,
    pub completed: usize,
    /// 结束时仍在途的执行数，必须为 0。
    pub outstanding_at_end: u64,
    pub session_view_calls: u64,
    pub execute_calls: u64,
    pub cancel_calls: u64,
    pub close_calls: u64,
    /// 端口发出的事件条数。
    pub events_emitted: u64,
    /// 网关 `apply_event` 登记的事件条数（等于 `events_emitted`，除非投影把事件整条丢掉）。
    pub events_projected: u64,
    /// 结束时网关仍保留的执行记录数。网关没有剪枝接口，这个数会单调增长；
    /// 记下来是为了让「基准跑完内存涨了多少」有据可查，而不是被当成泄漏。
    pub execution_records_retained: usize,
    pub projection: ProjectionReport,
    /// 被排除的那段（fake 命令 10 毫秒）的真实成本，非门禁列。
    pub driver_round_trip: DriverRoundTrip,
    pub driver_round_trip_median_nanos: Option<u64>,
    /// 因锁中毒而丢掉的诊断采样数，必须为 0。
    pub dropped_round_trips: u64,
    pub permit_reconciliation: PermitReconciliation,
}

/// 一次基准跑完的结论。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    /// 计划是否逐字等于 §11.1。非 false 时**不得**输出「达标」。
    pub conforms_to_spec: bool,
    pub gate_p95_nanos: u64,
    pub rounds_evaluated: usize,
    pub rounds_passing: usize,
    /// 最差一轮的 p95；没有任何一轮测到样本时是 `None`。
    pub worst_round_p95_nanos: Option<u64>,
    pub failures_total: usize,
    pub event_projection_clean: bool,
    pub sample_counts_match: bool,
    /// 门禁本身是否成立（§11.6 的判定式）。
    pub gate_passed: bool,
    /// 逐字结论行，供 stdout 与产物共用同一句措辞。
    pub conclusions: Vec<String>,
}

/// 一次完整基准的结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BenchRun {
    pub plan: BenchPlan,
    pub conforms_to_spec: bool,
    /// 实测 CPU 数与内存字节数，由入口注入（基准进程读不到 CPU 拓扑）。
    /// §11.6 要求 `environment` 段足以让照着产物的人不必回头猜环境。
    pub measured_vcpus: u32,
    pub measured_memory_bytes: u64,
    /// 预热轮的结论（样本不进分位数；只用来证明预热真的跑过）。
    pub warmup: RoundOutcome,
    pub rounds: Vec<RoundOutcome>,
    /// 全部轮的逐请求原始样本（预热**不含**，预热不是被测对象）。
    pub raw: Vec<RawSample>,
    pub journal: ExecutionJournal,
    /// 全程真实墙钟耗时（纳秒）。
    pub wall_time_nanos: u64,
    /// 口径披露：偏离判据之处原样写进产物，不藏在解释里。
    pub notes: Vec<String>,
}

impl BenchRun {
    pub fn rounds_evaluated(&self) -> usize {
        self.rounds.len()
    }

    pub fn rounds_passing(&self) -> usize {
        self.rounds.iter().filter(|r| r.passes_gate()).count()
    }

    /// 最差一轮的 p95（不是平均、不是最好的一轮——§11.3 逐轮判定的理由）。
    pub fn worst_round_p95_nanos(&self) -> Option<u64> {
        self.rounds
            .iter()
            .filter_map(RoundOutcome::p95_nanos)
            .max()
    }

    pub fn failures_total(&self) -> usize {
        self.rounds
            .iter()
            .map(RoundOutcome::failures)
            .sum::<usize>()
            .saturating_add(self.warmup.failures())
    }

    /// §11.6 的判定式：`all(round.p95 <= 10ms)` 且 `failures == 0`。
    ///
    /// 附加两条同样属于判据字面的门槛：事件重复/丢失为 0（CM-60 性能门槛），
    /// 以及完成数与样本数一致（否则分位数是拿残缺样本算的）。
    pub fn verdict(&self) -> Verdict {
        let gate_passed = !self.rounds.is_empty()
            && self.rounds_passing() == self.rounds.len()
            && self.failures_total() == 0
            && self.journal.projection.is_clean()
            && self.journal.permit_reconciliation.gateway_ledger_balanced
            && !self
                .rounds
                .iter()
                .any(RoundOutcome::sample_count_mismatch);
        let worst = self.worst_round_p95_nanos();
        let mut conclusions = Vec::new();
        for round in &self.rounds {
            conclusions.push(match round.p95_nanos() {
                Some(p95) => format!(
                    "round {}: p95 = {:.3} ms（门禁 10 ms），N = {}，失败 {}，排队 {}，{}",
                    round.round,
                    p95 as f64 / 1_000_000.0,
                    round.n(),
                    round.failures(),
                    round.outcome.queued,
                    if round.passes_gate() {
                        "通过"
                    } else {
                        "未通过"
                    },
                ),
                None => format!(
                    "round {}: 未采到任何样本（不是 0 ms），N = {}，失败 {}",
                    round.round,
                    round.n(),
                    round.failures(),
                ),
            });
        }
        if !self.conforms_to_spec {
            conclusions.push(format!(
                "本次运行不是 §11.1 的规格计划（并发 {} / 预热 {} / 每轮 {} / {} 轮 / fake {} ms），\
                 因此不作「按判据达标」的结论。",
                self.plan.concurrency,
                self.plan.warmup,
                self.plan.per_round,
                self.plan.rounds,
                self.plan.fake_command.as_millis(),
            ));
        }
        conclusions.push(if gate_passed {
            format!(
                "门禁判定式成立：{} 轮逐轮 p95 均 ≤ 10 ms 且失败数为 0，事件重复/丢失为 0。",
                self.rounds.len(),
            )
        } else {
            "门禁判定式不成立：存在未通过的轮次、失败请求、事件重复/丢失或样本数不一致。".to_string()
        });
        Verdict {
            conforms_to_spec: self.conforms_to_spec,
            gate_p95_nanos: GATE_P95_NANOS,
            rounds_evaluated: self.rounds.len(),
            rounds_passing: self.rounds_passing(),
            worst_round_p95_nanos: worst,
            failures_total: self.failures_total(),
            event_projection_clean: self.journal.projection.is_clean(),
            sample_counts_match: !self.rounds.iter().any(RoundOutcome::sample_count_mismatch),
            gate_passed,
            conclusions,
        }
    }

    /// 判定式成立**且**计划逐字等于 §11.1 时，才允许说「按判据达标」。
    pub fn comparable_to_criterion(&self) -> bool {
        self.verdict().gate_passed && self.conforms_to_spec
    }
}

/// 基准跑不起来时的错误。
#[derive(Debug)]
pub enum BenchError {
    /// 网关/端口返回了运行时错误。
    Runtime(String),
    /// 运行时构造或 spawn 失败。
    RuntimeSetup(String),
    /// 计划本身不可运行（零样本、零并发、零轮次）。
    InvalidPlan(String),
}

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchError::Runtime(detail) => write!(f, "cm60-bench runtime: {detail}"),
            BenchError::RuntimeSetup(detail) => write!(f, "cm60-bench setup: {detail}"),
            BenchError::InvalidPlan(detail) => write!(f, "cm60-bench plan: {detail}"),
        }
    }
}

impl std::error::Error for BenchError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(round: usize, ordinal: usize, total: u64) -> RawSample {
        RawSample {
            round,
            ordinal,
            gateway_nanos: total / 2,
            registration_nanos: total - total / 2,
        }
    }

    #[test]
    fn total_is_summed_per_request_not_percentiles_added() {
        let samples: Vec<RawSample> = (1..=100u64).map(|n| sample(1, n as usize, n)).collect();
        let totals: Vec<u64> = samples.iter().map(total_nanos).collect();
        let per_request = Percentiles::of(&totals);
        let gateway: Vec<u64> = samples.iter().map(|s| s.gateway_nanos).collect();
        let registration: Vec<u64> = samples.iter().map(|s| s.registration_nanos).collect();
        // 两个口径在单调样本上恰好一致，但它们是不同的问题：
        // 这里钉住的是「逐请求先求和」这条实现路径，而不是巧合。
        assert_eq!(per_request.p95_nanos, Percentiles::of(&gateway).p95_nanos
            .saturating_add(Percentiles::of(&registration).p95_nanos));
    }

    #[test]
    fn empty_samples_yield_none_not_zero() {
        let percentiles = Percentiles::of(&[]);
        assert!(percentiles.is_empty());
        assert_eq!(percentiles.p95_nanos, None);
        assert_eq!(percentiles.max_nanos, None);
    }

    #[test]
    fn an_empty_round_never_passes_the_gate() {
        let round = RoundOutcome {
            round: 1,
            concurrency: 8,
            samples: Vec::new(),
            outcome: SampleOutcome {
                requested: 10,
                ..SampleOutcome::default()
            },
            rejections: BTreeMap::new(),
            queued_wait: Percentiles::of(&[]),
            percentiles: Percentiles::of(&[]),
            wall_time_nanos: 1,
            event_projection: ProjectionReport::default(),
        };
        assert!(!round.passes_gate(), "没测到 ≠ 0 毫秒通过");
    }

    #[test]
    fn failures_are_counted_not_deleted() {
        // 三个请求：两个完成（其中一个很慢），一个派发失败。
        let round = RoundOutcome {
            round: 1,
            concurrency: 2,
            samples: vec![sample(1, 1, 100), sample(1, 2, 1_000_000)],
            outcome: SampleOutcome {
                requested: 3,
                completed: 2,
                dispatch_failed: 1,
                ..SampleOutcome::default()
            },
            rejections: BTreeMap::new(),
            queued_wait: Percentiles::of(&[]),
            percentiles: Percentiles::of(&[100, 1_000_000]),
            wall_time_nanos: 1,
            event_projection: ProjectionReport::default(),
        };
        assert_eq!(round.n(), 2, "样本集合必须原样保留，不因失败而缩水");
        assert_eq!(round.failures(), 1, "失败数单列");
        assert!(!round.passes_gate(), "失败数非 0 时门禁不成立");
    }

    #[test]
    fn queued_and_rejected_requests_are_counted_outside_the_sample_set() {
        let outcome = SampleOutcome {
            requested: 5,
            completed: 2,
            rejected: 1,
            queued: 2,
            ..SampleOutcome::default()
        };
        assert_eq!(outcome.failures(), 1);
        assert_eq!(outcome.queued, 2);
        let mut merged = SampleOutcome::default();
        merged.merge(&outcome);
        assert_eq!(merged.queued, 2);
        assert_eq!(merged.completed, 2);
    }

    #[test]
    fn replay_marks_the_round_unusable() {
        let round = RoundOutcome {
            round: 1,
            concurrency: 2,
            samples: vec![sample(1, 1, 100)],
            outcome: SampleOutcome {
                requested: 2,
                completed: 1,
                replays: 1,
                ..SampleOutcome::default()
            },
            rejections: BTreeMap::new(),
            queued_wait: Percentiles::of(&[]),
            percentiles: Percentiles::of(&[100]),
            wall_time_nanos: 1,
            event_projection: ProjectionReport::default(),
        };
        assert!(round.sample_count_mismatch());
    }
}
