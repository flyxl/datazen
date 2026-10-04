//! `outcome` 的单测单独成文件：`outcome.rs` 的**类型**已经贴近 800 行上限，
//! 而这一组测的是**判定口径**（N 怎么记、缺口怎么露、分位数输入怎么对账），
//! 值得能单独读完——它读起来就是一份口径说明书。
use super::*;
use crate::driver::ProjectionReport;
use std::collections::BTreeMap;

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
        percentile_input: 0,
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
        percentile_input: 2,
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
        percentile_input: 2,
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
        percentile_input: 2,
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
fn a_shortened_percentile_input_is_caught() {
    // 第三种藏法，也是最难发现的一种：**一个样本都不删，账一个数都不改**，
    // 只是喂给分位数的切片短了。产物里 `measured` / `n` / `unmeasured_failures`
    // 全都自洽，p95 还会因此**变小**——没有任何一列会矛盾。
    //
    // 所以输入条数必须单独记一列并进第 6 条：否则「p95 吃的是哪几个数」这件事
    // 只存在于 `Percentiles::of` 那一行的临时切片里，谁都看不见。
    let mut round = RoundOutcome {
        round: 1,
        concurrency: 2,
        samples: vec![sample(1, 1, 100), sample(1, 2, 1_000_000)],
        outcome: SampleOutcome {
            requested: 2,
            admitted: 2,
            completed: 2,
            ..SampleOutcome::default()
        },
        rejections: BTreeMap::new(),
        queued_wait: Percentiles::of(&[]),
        percentile_input: 2,
        percentiles: Percentiles::of(&[100, 1_000_000]),
        wall_time_nanos: 1,
        event_projection: ProjectionReport::default(),
    };
    assert!(!round.sample_count_mismatch(), "输入齐全时账是平的");
    assert!(round.passes_gate(), "p95 101 纳秒 ≪ 10 毫秒门禁");

    // 把最慢的那条从分位数输入里剔掉——账上仍然是两条样本、零缺口。
    round.percentile_input = 1;
    round.percentiles = Percentiles::of(&[100]);
    assert_eq!(round.measured(), 2, "样本仓库一条没少");
    assert!(
        round.sample_count_mismatch(),
        "切片被做短了必须被当成账不平"
    );
    assert!(
        !round.passes_gate(),
        "被削短的输入算出来的 p95 不许通过门禁"
    );
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
        percentile_input: 1,
        percentiles: Percentiles::of(&[100]),
        wall_time_nanos: 1,
        event_projection: ProjectionReport::default(),
    };
    assert!(round.sample_count_mismatch());
}
