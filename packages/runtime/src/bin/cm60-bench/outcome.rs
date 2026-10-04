//! 基准的产物数据结构：逐请求原始样本、逐轮结论、事件/许可对账、以及最终判定。
//!
//! ## 为什么这些字段是「少的几个」不行
//!
//! `fake-runtime-fixtures.md` §11.3 把每轮必须输出的东西列全了：N、p50、p90、p95、p99、
//! 最大值、失败数、排队数。少输出任何一项，读者就没法判断这一轮是不是在同一个实验里。
//! 同一节还钉死了两条口径，结构上体现为字段的分离：
//!
//! ## N 是「获准且未排队」的请求数，不是样本条数
//!
//! §11.3（`fake-runtime-fixtures.md:569`）的口径是「**不删除失败样本**：失败、超时、
//! 被取消的样本数与占比必须与分位数一起输出，**禁止只统计成功样本**」，唯一的豁免在
//! `:568`：「被 `QueueFull` 拒绝或排队等待的请求」。
//!
//! 所以本文件把三个数**分开**记，谁也不顶替谁：
//!
//! - **N = [`RoundOutcome::n`] = [`SampleOutcome::admitted`]**：获准（受理成功）且未排队的
//!   请求数，**含随后失败的那些**。
//! - **measured = [`RoundOutcome::measured`] = `samples.len()`**：真正打完两段计时的条数，
//!   分位数只对这一堆算。
//! - **unmeasured_failures = [`SampleOutcome::unmeasured_failures`]**：获准却打不出第二段
//!   终点的请求数（派发失败、幂等重发）。
//!
//! 恒等式 `n() == measured() + unmeasured_failures()` 由
//! [`RoundOutcome::sample_count_mismatch`] 守住，其中「N ≠ 样本数 + 缺口数」这一条正是
//! **「N 悄悄只统计了成功样本」的探测器**；`unmeasured_failures > 0` 时门禁必为 false
//! （[`RoundOutcome::passes_gate`]）。
//!
//! **不允许用任何构造值把缺失的样本补齐**（0、超时值、上一段的值都不行）：那等于把编造的
//! 数字喂给 p95，比少报一条样本更坏——它把失败藏进分位数里，而且看不出来。缺口只能是缺口。
//!
//! 另有一处更早的坑：被拒的请求**不是被豁免**，而是**从未获准**——`rejected` 里的每一项在
//! 计数上根本不进入 N。产物里这是分开的两列，不要互相改述。

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
///
/// 每一列都**互不顶替**：N（`admitted`）、分位数的输入条数（不在本结构里，见
/// [`RoundOutcome::measured`]）、失败分类、以及「获准却没测出时长」的缺口。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SampleOutcome {
    /// 本轮发出的请求数。失败占比的分母。
    pub requested: usize,
    /// **获准（受理成功）且未排队**的请求数 —— §11.3 的 N，**含随后失败的**。
    /// 恒等式：`admitted == completed + dispatch_failed + replays`。
    pub admitted: usize,
    /// 走完「受理 → 派发 → 登记」全流程、**两段计时都打上了终点**的请求数。
    /// 分位数的输入条数应当等于它（见 [`RoundOutcome::measured`]）。
    pub completed: usize,
    /// 受理被拒（含队列已满）。**从未获准**，因此不进入 N —— 不是被豁免，是不在口径里。
    pub rejected: usize,
    /// 获准但派发失败。已计入 N（`admitted`），但驱动一失败第二段终点就打不出来，
    /// 因此**没有样本**，只进 [`Self::unmeasured_failures`]；绝不编一个时长补进去。
    pub dispatch_failed: usize,
    /// 进入排队等待的请求数。等待时长另报分位数（§11.3）。
    pub queued: usize,
    /// 幂等重发数。规格基准里恒为 0：非 0 说明幂等键构造有问题，
    /// 两次请求会落在同一个 executionId 上，事件流随即被判重复，整轮数据不可用。
    pub replays: usize,
    /// 获准、却打不出第二段终点的请求数（= `dispatch_failed + replays`）。
    /// 这是「失败样本不得静默消失」的显式出口：缺口被计数、被写进产物、被门禁判红，
    /// 而不是被分位数悄悄吞掉。
    pub unmeasured_failures: usize,
}

impl SampleOutcome {
    /// 失败数（§11.3「失败数单列」）。判定式要求它为 0。
    pub fn failures(&self) -> usize {
        self.rejected + self.dispatch_failed
    }

    /// 失败占比（§11.3 要求「样本数**与占比**」与分位数一起输出）。
    /// 分母是本轮发出的请求数；`requested == 0` 时无占比（`None`），不是 0%。
    pub fn failure_ratio(&self) -> Option<f64> {
        if self.requested == 0 {
            return None;
        }
        Some(self.failures() as f64 / self.requested as f64)
    }

    pub fn merge(&mut self, other: &SampleOutcome) {
        self.requested += other.requested;
        self.admitted += other.admitted;
        self.completed += other.completed;
        self.rejected += other.rejected;
        self.dispatch_failed += other.dispatch_failed;
        self.queued += other.queued;
        self.replays += other.replays;
        self.unmeasured_failures += other.unmeasured_failures;
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
    /// 本轮样本的分位数（输入 = `samples.len()`，即 [`Self::measured`]；
    /// N 是 [`Self::n`]，两者**不是**同一个数）。
    pub percentiles: Percentiles,
    /// 本轮真实墙钟耗时（纳秒）。**不参与门禁**：它含 fake 命令的虚拟 10 毫秒与
    /// 事件投影，只用来解释量级，不是被测的「网关附加耗时」。
    pub wall_time_nanos: u64,
    /// 本轮事件投影（重复/丢失必须为 0，§11.1 性能门槛）。
    pub event_projection: ProjectionReport,
}

impl RoundOutcome {
    /// §11.3 要求输出的 N：**获准且未排队**的请求数，**含随后失败的**。
    ///
    /// 它**不是** `samples.len()`。失败样本不删除（`:569`），它们仍留在 N 里，只是分位数
    /// 算不到它们——那一批的去处是 [`Self::unmeasured_failures`]。拿 `samples.len()` 当 N
    /// 就是「只统计成功样本」，正是 §11.3 明文禁止的那件事。
    pub fn n(&self) -> usize {
        self.outcome.admitted
    }

    /// 分位数的真实输入条数 = `n() - unmeasured_failures()`，也就是 `samples.len()`。
    pub fn measured(&self) -> usize {
        self.samples.len()
    }

    /// 获准却打不出第二段终点的请求数。非 0 即「有请求掉出了 p95 的输入集合」。
    pub fn unmeasured_failures(&self) -> usize {
        self.outcome.unmeasured_failures
    }

    pub fn p95_nanos(&self) -> Option<u64> {
        self.percentiles.p95_nanos
    }

    pub fn failures(&self) -> usize {
        self.outcome.failures()
    }

    pub fn failure_ratio(&self) -> Option<f64> {
        self.outcome.failure_ratio()
    }

    /// 逐轮门禁：p95 ≤ 10 毫秒（§11.3），**且**失败数为 0（§11.6），
    /// **且**没有任何获准请求掉出分位数输入集合。
    ///
    /// 「没测到」返回 false：空样本集的 p95 是 `None`，不是 0，不会被当成通过。
    pub fn passes_gate(&self) -> bool {
        match self.p95_nanos() {
            Some(p95) => {
                p95 <= GATE_P95_NANOS
                    && self.failures() == 0
                    && self.unmeasured_failures() == 0
                    && !self.sample_count_mismatch()
            }
            None => false,
        }
    }

    /// 样本账不自洽 → 本轮数据不可用。**任一条**成立即不可用：
    ///
    /// 1. 走完全流程的条数 ≠ 实际样本条数（有人删过样本）；
    /// 2. **N ≠ 完成 + 派发失败 + 重发**（N 被悄悄缩成了成功样本数）；
    /// 3. **N ≠ 测到 + 未测出**（获准却没打点的请求在账上凭空消失）；
    /// 4. **未测出 ≠ 派发失败 + 重发**（缺口只报一部分，等于把失败藏起来）；
    /// 5. 出现幂等重发（同一 executionId 会收两遍事件，整轮数据不可用）。
    ///
    /// 第 2、4 条是**承重**的那两条。R1 缺陷的形状正是「`completed == samples.len()`
    /// 依然成立（第 1 条放行），但 N 只剩下成功样本数」——第 2 条把它拦下；
    /// 而「知道有失败打不出终点，却干脆不记这个缺口」这种更省事的写法，
    /// 第 4 条把它拦下。两条都过不了的账，才允许拿去算 p95。
    pub fn sample_count_mismatch(&self) -> bool {
        let measured = self.measured();
        let unmeasurable = self.outcome.dispatch_failed + self.outcome.replays;
        self.outcome.completed != measured
            || self.n() != self.outcome.completed + unmeasurable
            || self.n() != measured + self.unmeasured_failures()
            || self.unmeasured_failures() != unmeasurable
            || self.outcome.replays != 0
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

/// 把占比渲染成百分数字符串；`None`（分母为 0）渲染成 `-`，**不是 `0.000%`**。
fn ratio_text(ratio: Option<f64>) -> String {
    match ratio {
        Some(value) => format!("{:.3}%", value * 100.0),
        None => "-".to_string(),
    }
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
    /// 逐轮 N 之和（获准且未排队，含随后失败的）。口径范围：只含被测轮，**不含预热轮**
    /// （预热没有分位数，它的数字不参与任何 p95）。
    pub admitted_total: usize,
    /// 分位数真实输入条数之和。
    pub measured_total: usize,
    /// 全程获准却打不出第二段终点的请求数。**非 0 即门禁不成立**。
    pub unmeasured_failures_total: usize,
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
    /// 计划**形状**是否逐字等于 §11.1 的规格计划。它只看 [`BenchPlan`]，不看是否开了
    /// 故障注入——注入不改变计划的形状，它让**判定变红**（`gate_passed=false`）。
    /// 两者是不同的问题：前者是「这次跑的规模是不是判据那个规模」，后者是「结果是否成立」。
    pub conforms_to_spec: bool,
    /// 诊断用故障注入：每第 N 次驱动执行失败一次，0 = 关闭。**开着它跑出来的产物不是判据证据**，
    /// 但它必须跟着产物走，否则没人分得清一份「全绿」的数据是不是注入关掉之后才有的。
    pub inject_failure_every: u64,
    /// **声明值**（命令行注入），不是探测值。字段名因此叫 `declared_*`：
    /// 产物里写着一个数，不等于这个数被测量过。
    /// §11.6 要求 `environment` 段足以让照着产物的人不必回头猜环境——同一段里另有
    /// 真正探测出来的 `detected_parallelism`（`report::Environment`）。
    pub declared_vcpus: u32,
    pub declared_memory_bytes: u64,
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
        self.rounds.iter().filter_map(RoundOutcome::p95_nanos).max()
    }

    /// 失败数之和，**含预热轮**。这是有意的不对称：预热轮不参与任何 p95，但如果预热里
    /// 就出现了失败请求，那是 harness 自身不健康的信号，不该被「预热不算数」抹掉。
    pub fn failures_total(&self) -> usize {
        self.rounds
            .iter()
            .map(RoundOutcome::failures)
            .sum::<usize>()
            .saturating_add(self.warmup.failures())
    }

    /// 被测轮的 N 之和（预热轮不在 p95 口径内，故不计入这三个数）。
    pub fn admitted_total(&self) -> usize {
        self.rounds.iter().map(RoundOutcome::n).sum()
    }

    /// 被测轮的分位数输入条数之和。
    pub fn measured_total(&self) -> usize {
        self.rounds.iter().map(RoundOutcome::measured).sum()
    }

    /// 被测轮里获准却没测出时长的请求数之和。
    pub fn unmeasured_failures_total(&self) -> usize {
        self.rounds
            .iter()
            .map(RoundOutcome::unmeasured_failures)
            .sum()
    }

    /// §11.6 的判定式：`all(round.p95 <= 10ms)` 且 `failures == 0`。
    ///
    /// 附加两条同样属于判据字面的门槛：事件重复/丢失为 0（CM-60 性能门槛），
    /// 以及样本账自洽（否则分位数是拿一个来路不明的 N 算的）。
    /// 「有获准请求掉出分位数」由 [`RoundOutcome::passes_gate`] 里的
    /// `unmeasured_failures() == 0` 承担，因此已被 `rounds_passing()` 蕴含，
    /// 这里不再重复列一遍——重复列只会让人误以为还能分开调。
    pub fn verdict(&self) -> Verdict {
        let gate_passed = !self.rounds.is_empty()
            && self.rounds_passing() == self.rounds.len()
            && self.failures_total() == 0
            && self.journal.projection.is_clean()
            && self.journal.permit_reconciliation.gateway_ledger_balanced
            && !self.rounds.iter().any(RoundOutcome::sample_count_mismatch);
        let worst = self.worst_round_p95_nanos();
        let mut conclusions = Vec::new();
        for round in &self.rounds {
            conclusions.push(match round.p95_nanos() {
                Some(p95) => format!(
                    "round {}: p95 = {:.3} ms（门禁 10 ms），N = {}（测到 {} / 未测出 {}），\
                     失败 {}（{}），排队 {}，{}",
                    round.round,
                    p95 as f64 / 1_000_000.0,
                    round.n(),
                    round.measured(),
                    round.unmeasured_failures(),
                    round.failures(),
                    ratio_text(round.failure_ratio()),
                    round.outcome.queued,
                    if round.passes_gate() {
                        "通过"
                    } else {
                        "未通过"
                    },
                ),
                None => format!(
                    "round {}: 未采到任何样本（不是 0 ms），N = {}（测到 {} / 未测出 {}），失败 {}（{}）",
                    round.round,
                    round.n(),
                    round.measured(),
                    round.unmeasured_failures(),
                    round.failures(),
                    ratio_text(round.failure_ratio()),
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
        if self.unmeasured_failures_total() > 0 {
            conclusions.push(format!(
                "有 {} 个**获准**请求没能出现在 p95 的样本集合里（派发失败或幂等重发）：\
                 它们仍然计入 N = {}，分位数只对剩下的 {} 条计算；\
                 这 {} 条**不参与**分位数，也不会用 0、超时值或任何构造值补进去填满。",
                self.unmeasured_failures_total(),
                self.admitted_total(),
                self.measured_total(),
                self.unmeasured_failures_total(),
            ));
        }
        conclusions.push(if gate_passed {
            format!(
                "门禁判定式成立：{} 轮逐轮 p95 均 ≤ 10 ms 且失败数为 0，事件重复/丢失为 0，\
                 且没有获准请求掉出分位数输入集合（N {} = 测到 {} + 未测出 {}）。",
                self.rounds.len(),
                self.admitted_total(),
                self.measured_total(),
                self.unmeasured_failures_total(),
            )
        } else {
            "门禁判定式不成立：存在未通过的轮次、失败请求、事件重复/丢失、样本数不一致，\
             或有获准请求掉出了分位数输入集合。"
                .to_string()
        });
        Verdict {
            conforms_to_spec: self.conforms_to_spec,
            gate_p95_nanos: GATE_P95_NANOS,
            rounds_evaluated: self.rounds.len(),
            rounds_passing: self.rounds_passing(),
            worst_round_p95_nanos: worst,
            failures_total: self.failures_total(),
            admitted_total: self.admitted_total(),
            measured_total: self.measured_total(),
            unmeasured_failures_total: self.unmeasured_failures_total(),
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
        // 两段反向互补：逐请求求和恒为 101，而两个 p95 各自 95、合计 190。
        // 这组数据让「先逐请求求和再取分位数」与「先取两个分位数再相加」
        // 产生可观测差异，因此本测试钉住的是实现路径本身，不是巧合相等。
        let samples: Vec<RawSample> = (1..=100u64)
            .map(|n| RawSample {
                round: 1,
                ordinal: n as usize,
                gateway_nanos: n,
                registration_nanos: 101 - n,
            })
            .collect();
        let totals: Vec<u64> = samples.iter().map(total_nanos).collect();
        assert!(totals.iter().all(|total| *total == 101));
        let gateway: Vec<u64> = samples.iter().map(|s| s.gateway_nanos).collect();
        let registration: Vec<u64> = samples.iter().map(|s| s.registration_nanos).collect();
        let sum_of_percentiles = Percentiles::of(&gateway)
            .p95_nanos
            .zip(Percentiles::of(&registration).p95_nanos)
            .map(|(g, r)| g.saturating_add(r));
        assert_eq!(Percentiles::of(&totals).p95_nanos, Some(101));
        assert_eq!(sum_of_percentiles, Some(190));
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
    fn failures_are_counted_not_deleted_and_n_does_not_shrink() {
        // 三个请求：两个完成（其中一个很慢），一个派发失败。
        let round = RoundOutcome {
            round: 1,
            concurrency: 2,
            samples: vec![sample(1, 1, 100), sample(1, 2, 1_000_000)],
            outcome: SampleOutcome {
                requested: 3,
                admitted: 3,
                completed: 2,
                dispatch_failed: 1,
                unmeasured_failures: 1,
                ..SampleOutcome::default()
            },
            rejections: BTreeMap::new(),
            queued_wait: Percentiles::of(&[]),
            percentiles: Percentiles::of(&[100, 1_000_000]),
            wall_time_nanos: 1,
            event_projection: ProjectionReport::default(),
        };
        assert_eq!(round.n(), 3, "N 是获准数，失败的请求仍留在 N 里");
        assert_eq!(round.measured(), 2, "分位数的输入条数只到测到的那两条");
        assert_eq!(round.unmeasured_failures(), 1, "缺口必须被显式计数");
        assert_eq!(round.n(), round.measured() + round.unmeasured_failures());
        assert_eq!(round.failures(), 1, "失败数单列");
        assert_eq!(
            round.failure_ratio(),
            Some(1.0 / 3.0),
            "占比与分位数一起输出"
        );
        assert!(!round.passes_gate(), "失败数非 0 时门禁不成立");
        assert!(!round.sample_count_mismatch(), "缺口被记账后账是自洽的");
    }

    #[test]
    fn a_gap_that_is_not_even_counted_is_caught() {
        // 比「N 缩水」更省事、也更危险的一种写法：N 不缩水了，缺口却干脆不记——
        // 于是分位数看起来完美，p95 漂亮，失败被彻底藏进黑洞。
        // 这正是「不许用 0 / 超时值 / 上一段值把样本补齐」这种做法的账目形状：
        // 编造出来的时长会让 ①②③ 三条全部自洽，只有第 4 条能看见它。
        let round = RoundOutcome {
            round: 1,
            concurrency: 2,
            samples: vec![sample(1, 1, 100), sample(1, 2, 200)],
            outcome: SampleOutcome {
                requested: 3,
                admitted: 3,
                completed: 2,
                dispatch_failed: 1,
                unmeasured_failures: 0,
                ..SampleOutcome::default()
            },
            rejections: BTreeMap::new(),
            queued_wait: Percentiles::of(&[]),
            percentiles: Percentiles::of(&[100, 200]),
            wall_time_nanos: 1,
            event_projection: ProjectionReport::default(),
        };
        assert!(round.p95_nanos().is_some(), "这条账的伪装正是 p95 很好看");
        assert!(
            round.sample_count_mismatch(),
            "派发失败了却没记缺口 ⇒ 账不平，不管分位数多漂亮"
        );
        assert!(!round.passes_gate(), "未记账的缺口不能通过门禁");
        // 对照组：同样的三次请求，缺口被如实记成 1 之后，账是平的——
        // 但门禁照样是红的。这正是「让门禁对未测出这个事实可见」的那一步：
        // 把账记对了不等于放过它。
        let mut honest = round.clone();
        honest.outcome.dispatch_failed = 1;
        honest.outcome.unmeasured_failures = 1;
        assert!(!honest.sample_count_mismatch(), "如实记账后账是平的");
        assert_eq!(honest.n(), 3);
        assert_eq!(honest.measured() + honest.unmeasured_failures(), honest.n());
        assert!(
            !honest.passes_gate(),
            "账平了，但仍有获准请求掉出分位数输入集合 ⇒ 门禁不成立"
        );
        assert_eq!(honest.failures(), 1, "失败数也必须仍然单列");
    }

    #[test]
    fn a_sample_accounting_that_shrinks_n_is_caught() {
        // 这一轮模拟 R1 的缺陷形状：「N 被悄悄改成只统计成功样本」。
        // 样本仍是两条，completed 也是 2，所以第 1 条（completed == measured）放行；
        // 但 admitted 被填成 2、那次派发失败没人记账，
        // 第 2 条（N == 完成 + 派发失败 + 重发）必须把它拦下来。
        let round = RoundOutcome {
            round: 1,
            concurrency: 2,
            samples: vec![sample(1, 1, 100), sample(1, 2, 1_000_000)],
            outcome: SampleOutcome {
                requested: 3,
                admitted: 2,
                completed: 2,
                dispatch_failed: 1,
                unmeasured_failures: 0,
                ..SampleOutcome::default()
            },
            rejections: BTreeMap::new(),
            queued_wait: Percentiles::of(&[]),
            percentiles: Percentiles::of(&[100, 1_000_000]),
            wall_time_nanos: 1,
            event_projection: ProjectionReport::default(),
        };
        assert!(
            round.sample_count_mismatch(),
            "N 缩水必须被当成账不平，而不是当成「样本集合原样保留」"
        );
        assert!(
            !round.passes_gate(),
            "样本账不平的一轮不允许通过门禁，哪怕 p95 看起来很漂亮"
        );
    }

    #[test]
    fn rejected_requests_never_enter_n_and_queued_is_reported_separately() {
        // 被拒不是「被豁免」，是「从未获准」：N 不含它，也不靠豁免规则把它排除。
        let outcome = SampleOutcome {
            requested: 5,
            admitted: 2,
            completed: 2,
            rejected: 1,
            queued: 2,
            ..SampleOutcome::default()
        };
        assert_eq!(outcome.failures(), 1);
        assert_eq!(outcome.queued, 2);
        assert_eq!(outcome.failure_ratio(), Some(0.2));
        let none_denominator = SampleOutcome::default();
        assert_eq!(
            none_denominator.failure_ratio(),
            None,
            "分母为 0 时没有占比，不是 0%"
        );
        let mut merged = SampleOutcome::default();
        merged.merge(&outcome);
        assert_eq!(merged.queued, 2);
        assert_eq!(merged.completed, 2);
        assert_eq!(merged.admitted, 2);
    }

    #[test]
    fn replay_marks_the_round_unusable() {
        let round = RoundOutcome {
            round: 1,
            concurrency: 2,
            samples: vec![sample(1, 1, 100)],
            outcome: SampleOutcome {
                requested: 2,
                admitted: 2,
                completed: 1,
                replays: 1,
                unmeasured_failures: 1,
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
