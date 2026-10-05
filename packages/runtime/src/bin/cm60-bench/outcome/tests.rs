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
fn a_shortened_percentile_input_is_caught() {
    // 第三种藏法，也是最难发现的一种：**一个样本都不删，账一个数都不改**，
    // 只是喂给分位数的切片短了。产物里 `measured` / `n` / `unmeasured_failures`
    // 全都自洽，p95 还会因此**变小**——没有任何一列会矛盾。
    //
    // 这里刻意**不再另立一列**去追这条：上一轮写的是 `percentile_input: totals.len()`
    // 加一条 `percentile_input == measured` 断言，那两样东西读的是同一个变量，
    // 切片一短就一起短，断言必然放行。条数现在长在 `Percentiles` 里，
    // 所以下面这个缺陷只有**一种**写法，而且它没法把条数留在 2 上蒙过去。
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
        percentiles: Percentiles::of(&[100, 1_000_000]),
        wall_time_nanos: 1,
        event_projection: ProjectionReport::default(),
    };
    assert!(!round.sample_count_mismatch(), "输入齐全时账是平的");
    assert!(round.passes_gate(), "p95 101 纳秒 ≪ 10 毫秒门禁");

    // 把最慢的那条从分位数输入里剔掉——账上仍然是两条样本、零缺口。
    // 切片一做短，条数**跟着**降到 1：没有第二个来源可以让它留在 2 上。
    round.percentiles = Percentiles::of(&[100]);
    assert_eq!(round.measured(), 2, "样本仓库一条没少");
    assert_eq!(
        round.percentile_input(),
        1,
        "输入条数必须跟着分位数走，而不是跟样本数走"
    );
    assert!(
        round.sample_count_mismatch(),
        "切片被做短了必须被当成账不平"
    );
    assert!(
        !round.passes_gate(),
        "被削短的输入算出来的 p95 不许通过门禁"
    );
}

/// F-04-1 的**真实样本量**守门测试：§11.1 每轮 10000 条。
///
/// 上一轮只在 4 条的小样本上验证过那条「剔掉最大值」的藏法被抓住，于是判成已修。
/// 在 10000 条上它**抓不住**，而且抓不住它的只有值比那一道：nearest-rank 取第
/// `ceil(0.95*M)` 项，而掉掉的正是最大值，它排在第 `ceil(0.95*10000) = 9500` 项**之后**——
/// `ceil(0.95*9999)` 同样是 9500，名次没动，落在名次上的那个值也就没动。
///
/// 「值比抓不住」不等于这一刀没人拦：条数由 [`Percentiles::of`] 在**同一次求值、同一个
/// 切片**上记下，切片一短它就跟着短，而它要对照的 [`RoundOutcome::measured`] 数的是样本
/// 仓库（`samples.len()`），两者不是同一个东西，于是门禁第 6 条必然发火。所以本测试把
/// 两件事**分别**钉住：
///
/// 1. **值比在真实样本量上确实是瞎的**（否则会有人再拿它当守卫）；
/// 2. **条数比不是**——输入被做短时 [`RoundOutcome::percentile_input`] 一定跟着降，
///    [`RoundOutcome::sample_count_mismatch`] 一定为真、门禁一定为红。
///
/// 这就是修复的判据：不再依赖「多记一列 + 一条断言」，而是让两者**同源**，
/// 于是失守只有一个入口（`Percentiles::of` 的入参），而那个入口正对着门禁第 6 条。
#[test]
fn the_input_count_gate_holds_at_the_spec_sample_size() {
    const PER_ROUND: usize = 10_000; // §11.1 的真实每轮样本量，不是小样本近似。
    let totals: Vec<u64> = (0..PER_ROUND as u64).map(|n| 1_000 + n * 37).collect();

    let full = Percentiles::of(&totals);
    let cut = Percentiles::of(&totals[..PER_ROUND - 1]);
    // 1. 值比在 10000 条上分不出差别——这正是必须另有一条判据的原因。
    assert_eq!(
        full.p95_nanos, cut.p95_nanos,
        "10000 条上剔掉最大值不改 p95 值：值比天然抓不到这种藏法"
    );
    // 2. 条数比一定分得出差别——与样本量无关。
    assert_eq!(full.input_len(), PER_ROUND);
    assert_eq!(cut.input_len(), PER_ROUND - 1);

    let samples: Vec<RawSample> = (1..=PER_ROUND)
        .map(|n| sample(1, n, totals[n - 1]))
        .collect();
    let accounting = SampleOutcome {
        requested: PER_ROUND,
        admitted: PER_ROUND,
        completed: PER_ROUND,
        ..SampleOutcome::default()
    };
    let mut round = RoundOutcome {
        round: 1,
        concurrency: 8,
        samples: samples.clone(),
        outcome: accounting,
        rejections: BTreeMap::new(),
        queued_wait: Percentiles::of(&[]),
        percentiles: full,
        wall_time_nanos: 1,
        event_projection: ProjectionReport::default(),
    };
    assert_eq!(round.measured(), PER_ROUND);
    assert_eq!(round.percentile_input(), PER_ROUND);
    assert!(!round.sample_count_mismatch(), "10000 条齐全时账是平的");

    // 只把喂进去的切片做短：账上仍然是 10000 条样本、零缺口、p95 值一模一样。
    round.percentiles = cut;
    assert_eq!(round.measured(), PER_ROUND, "样本仓库一条没少");
    assert_eq!(round.p95_nanos(), full.p95_nanos, "值仍然一模一样");
    assert!(
        round.sample_count_mismatch(),
        "在 §11.1 的真实样本量上，值比瞎着而条数比必须看得见"
    );
    assert!(
        !round.passes_gate(),
        "被削短的输入算出来的 p95 不许通过门禁，哪怕值一模一样"
    );
}

/// 4 条小样本上「剔掉最大值」**值比**也会红——这是 `ceil(0.95*4) = 4` 恰好落在最大值上，
/// 属于**巧合**，不是守卫。钉住它是为了让下一个人知道：这条断言在小样本上通过，
/// 并不能推出它在 §11.1 的每轮 10000 条上也成立（那里**只有值比瞎**：名次与值都不动，
/// 靠的是条数与分位数同源那一条把它拦下）。
#[test]
fn catching_the_shortened_input_on_four_samples_is_coincidence() {
    let totals = [100u64, 200, 300, 1_000_000];
    let cut = &totals[..3];
    assert_eq!(
        Percentiles::of(cut).p95_nanos,
        nearest_rank_percentile(cut, 0.95),
        "值比在 4 条上分得出差别"
    );
    assert_eq!(
        Percentiles::of(cut).p95_nanos,
        Some(300),
        "ceil(0.95*3) = 3 落在被剔掉那条之后的最后一位上，所以这里才看得见"
    );
    assert_ne!(
        Percentiles::of(&totals).p95_nanos,
        Percentiles::of(cut).p95_nanos,
        "巧合而已：同一套藏法在 10000 条上值比是平的"
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
        percentiles: Percentiles::of(&[100]),
        wall_time_nanos: 1,
        event_projection: ProjectionReport::default(),
    };
    assert!(round.sample_count_mismatch());
}
