//! `runner` 的单测单独成文件：`runner.rs` 本身已接近 800 行上限，且这一组
//! 测的是**判定口径**而不是驱动流程，值得能单独读完。

use super::*;
use crate::outcome::total_nanos;
use crate::plan::DEFAULT_PLAN;
use datazen_runtime::latency::nearest_rank_percentile;

#[test]
fn submission_token_rejections_preserve_the_machine_readable_reason() {
    assert_eq!(
        rejection_label(&GatewayError::SubmissionTokenRejected { reason: "expired" }),
        "submission-token:expired"
    );
}

#[tokio::test(start_paused = true)]
async fn harness_checks_owner_and_preserves_unknown_outcome_fencing() {
    let (port, journal) = FakeDriverPort::new(ready_view(), tiny().fake_command, EXEC_PREFIX, 0);
    let gateway = ExecutionGateway::new(
        port,
        OwnerMatchAuthorizer::shared(),
        InMemoryIdempotencyStore::shared(),
        Arc::new(InstantClock::new()),
    );
    let peer = RequestPrincipal::new(
        PrincipalId::new("principal-peer"),
        OrganizationId::new("org-cm60-bench"),
        DbSessionId::new(SESSION_ID),
    );
    assert!(matches!(
        gateway.accept(&peer, request(0)).await,
        Err(GatewayError::Runtime(
            datazen_runtime::connection::RuntimeError::UnknownSession(_)
        ))
    ));
    assert_eq!(journal.execute_calls(), 0);

    let accepted = gateway.accept(&principal(), request(0)).await.unwrap();
    gateway
        .dispatch(&principal(), accepted.execution_id())
        .await
        .unwrap();
    assert_eq!(journal.execute_calls(), 1);

    let mut retry = request(0);
    retry.idempotency_key = "fresh-key-same-query".to_owned();
    assert!(matches!(
        gateway.accept(&principal(), retry).await,
        Err(GatewayError::IdempotencyVerificationRequired { .. })
    ));
    assert_eq!(journal.execute_calls(), 1);
    assert!(gateway.accept(&principal(), request(1)).await.is_ok());
}

fn tiny() -> BenchPlan {
    BenchPlan {
        warmup: 4,
        per_round: 8,
        rounds: 2,
        concurrency: 2,
        fake_command: std::time::Duration::from_millis(10),
    }
}

#[test]
fn a_plan_with_no_samples_is_refused() {
    let error = run_bench(BenchPlan {
        per_round: 0,
        ..tiny()
    });
    assert!(error.is_err(), "零样本轮次不能被判定为通过");
}

#[tokio::test(start_paused = true)]
async fn every_request_produces_exactly_one_sample() {
    let plan = tiny();
    let run = drive_all(plan, 8, 17_179_869_184, 0)
        .await
        .unwrap_or_else(|error| panic!("bench failed: {error}"));
    let round = &run.rounds[0];
    assert_eq!(round.outcome.requested, plan.per_round);
    assert_eq!(round.n(), plan.per_round, "完成的请求必须各有一次两段计时");
    assert_eq!(round.failures(), 0);
    assert_eq!(round.outcome.queued, 0, "网关没有排队受理态");
    // N = 测到的 + 没测到的。这条恒等式一旦被打破，说明有人把失败样本删掉了。
    assert_eq!(round.n(), round.measured() + round.unmeasured_failures());
    assert!(!round.sample_count_mismatch());
}

/// F-01 的**真实 harness 守门测试**：开着故障注入跑整个 harness，断言
/// （1）N 仍然是每一条获准请求，**不因失败而缩水**；（2）打不出时长的那些**只**
/// 落在 `unmeasured_failures` 上，绝不被补成一个数；（3）门禁因此变红。
///
/// 之前只有 `latency.rs` 里一个纯函数级的用例（拿手工过滤好的向量喂给分位数），
/// 证明不了 harness 的记账——缺陷正是记账把失败样本吞了而那个用例照样绿。
#[tokio::test(start_paused = true)]
async fn injected_failures_stay_inside_n_and_turn_the_gate_red() {
    let plan = tiny();
    let run = drive_all(plan, 8, 17_179_869_184, 1)
        .await
        .unwrap_or_else(|error| panic!("bench failed: {error}"));
    let round = &run.rounds[0];
    assert_eq!(
        round.n(),
        plan.per_round,
        "每次执行都失败 ⇒ 每个请求都获准 ⇒ N 必须还是 per_round，样本不许缩水"
    );
    assert_eq!(
        round.measured(),
        0,
        "一次都测不出来：第二段终点根本不存在，不是「测出来是 0」"
    );
    assert_eq!(round.unmeasured_failures(), plan.per_round);
    assert!(round.failures() > 0, "失败数必须与分位数一起报出来");
    assert_eq!(
        round.failure_ratio(),
        Some(1.0),
        "占比也要报：全部失败就是 100%"
    );
    assert!(!round.sample_count_mismatch(), "记账本身仍须自洽");
    assert!(
        !round.passes_gate(),
        "有获准请求掉出分位数输入集合就不能判过"
    );
    assert!(!run.verdict().gate_passed);
    assert!(
        run.raw.is_empty(),
        "没有任何真实时长就一个样本都不许有，raw 里不能出现编出来的条目"
    );
}

/// F-04 的**真实 harness 守门测试**：记录下来的 p95 必须**恰好**是这一轮
/// 真的打完了两段计时的那批样本的分位数。
///
/// 之前唯一像样的守卫在 `latency.rs`，而它只测纯函数：手工过滤好的向量喂进去，
/// p95 当然诚实。harness 往向量里放了什么、失败项是不是被悄悄换成了别的数，它
/// 一个字都证明不了——而 F-01 的缺陷形状恰恰就是「向量被做短了而那条测试照样绿」。
///
/// 这条把两端钉在一起：取样侧（`round.samples` 的长度、raw 段的长度）必须等于
/// `measured`，算法侧（唯一实现 `latency::nearest_rank_percentile`）复算出的
/// p95 必须等于 `round.p95_nanos()`。两端中间任何一处动了，被削短或被补数，都会
/// 让这两行之一失配。
#[tokio::test(start_paused = true)]
async fn the_recorded_p95_is_the_percentile_of_the_samples_this_run_measured() {
    let plan = tiny();
    // 每 2 次派发注入一次失败：两条样本是真的，两条失败是真的，两者都不是 0。
    let run = drive_all(plan, 8, 17_179_869_184, 2)
        .await
        .unwrap_or_else(|error| panic!("bench failed: {error}"));
    let verdict = run.verdict();

    for round in &run.rounds {
        let totals: Vec<u64> = round.samples.iter().map(total_nanos).collect();
        assert_eq!(
            round.unmeasured_failures(),
            round.n() - round.measured(),
            "缺口必须正好是获准数减去真测到的条数"
        );
        assert!(
            round.unmeasured_failures() > 0 && round.measured() > 0,
            "这个用例要的是「一半测到一半测不到」，两头都不是空集才谈得上对照"
        );
        assert_eq!(
            round.p95_nanos(),
            nearest_rank_percentile(&totals, 0.95),
            "记录值与本次真测到的样本不符：要么有样本没进来，要么有值不是测出来的"
        );
        // 光比值不够：这个 plan 每轮只测到 4 条，而 nearest-rank 取第 `ceil(0.95*M)` 项，
        // `ceil(0.95*4) = 4` **恰好**落在最大值上，所以「把最大的一条剔掉」在这里会让
        // 上一行直接变红——**那是巧合，不是守卫**。实测反例：同样的藏法搬到 §11.1 的真实
        // 样本量（每轮 10000）上，掉掉的正是最大值，它排在第 `ceil(0.95*10000) = 9500` 项
        // **之后**：`ceil(0.95*9999)` 同样是 9500，名次没动，落在名次上的那个值也就没动，
        // 所以**只有值比是绿的**。拦住它的是条数——`Percentiles::of` 在同一次求值、同一个
        // 切片上记下 `input_len`，切片短一条，`percentile_input()` 就从 10000 掉到 9999，
        // 而 `measured()` 数的是 `samples.len()`，门禁第 6 条（条数 ≠ 实测条数）因此必红。
        // 真实样本量上的结论见
        // `outcome::tests::the_input_count_gate_holds_at_the_spec_sample_size`：
        // 守住它的是「条数与分位数同源」，不是任何一条在这里绿的断言。
        assert_eq!(
            round.percentile_input(),
            totals.len(),
            "分位数输入的条数必须就是本次真测到的样本数"
        );
        assert!(!round.sample_count_mismatch());
        assert_eq!(
            run.raw
                .iter()
                .filter(|sample| sample.round == round.round)
                .count(),
            round.measured(),
            "raw 段只能有真的测到的样本：失败的请求一条都不许在里面占位"
        );
    }

    // 全局对账：raw 里没有第二个来源，产物无法夹带构造值。
    let measured_total: usize = run.rounds.iter().map(|round| round.measured()).sum();
    assert_eq!(run.raw.len(), measured_total);
    assert_eq!(verdict.measured_total, measured_total);
    assert!(verdict.unmeasured_failures_total > 0);
    assert!(
        !verdict.gate_passed,
        "有获准请求掉出分位数输入集合，门禁就不许是绿的"
    );
}

/// 负向对照：不注入时恒等式照样成立，且门禁不受影响。
#[tokio::test(start_paused = true)]
async fn without_injection_every_admitted_request_is_measured() {
    let plan = tiny();
    let run = drive_all(plan, 8, 17_179_869_184, 0).await.unwrap();
    for round in std::iter::once(&run.warmup).chain(run.rounds.iter()) {
        assert_eq!(round.unmeasured_failures(), 0);
        assert_eq!(round.n(), round.measured());
        assert!(round.passes_gate());
    }
}

#[tokio::test(start_paused = true)]
async fn event_stream_is_lossless_across_the_round() {
    let run = drive_all(tiny(), 8, 17_179_869_184, 0).await.unwrap();
    assert!(run.journal.projection.is_clean(), "事件重复/丢失必须为 0");
    assert_eq!(run.journal.outstanding_at_end, 0, "在途执行必须归零");
}

#[tokio::test(start_paused = true)]
async fn warmup_samples_stay_out_of_the_measured_rounds() {
    let plan = tiny();
    let run = drive_all(plan, 8, 17_179_869_184, 0).await.unwrap();
    assert_eq!(run.warmup.outcome.requested, plan.warmup);
    assert_eq!(run.raw.len(), plan.per_round * plan.rounds);
}

#[tokio::test(start_paused = true)]
async fn idempotency_keys_are_unique_per_request() {
    let run = drive_all(tiny(), 8, 17_179_869_184, 0).await.unwrap();
    let total_replays: usize = std::iter::once(&run.warmup)
        .chain(run.rounds.iter())
        .map(|round| round.outcome.replays)
        .sum();
    assert_eq!(
        total_replays, 0,
        "键重复会让两次请求落在同一个 executionId 上"
    );
}

#[test]
fn the_default_plan_is_the_criterion_plan() {
    assert!(DEFAULT_PLAN.is_spec_plan());
    // 这里只钉常量；跑完整判据计划（5×10000）是 `--release` 基准的职责，不属于单测。
}
