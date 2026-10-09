//! **资源压力与 drain** —— 行为半的可执行判据。
//!
//! 判据：
//!
//! - 前置：总额度 20、控制预留 2；100 用户逻辑 session、短请求 fake 固定耗时 10 ms、队列上限 32。
//! - 步骤：提交 1000 次操作与取消，两个 worker 分配额度；将一个 worker drain；全部完成后关池。
//! - 断言：每次资源 journal 变化点真实资源不超过总额度；普通资源不侵占预留；
//!   拒绝/等待有界；drain 不接新资源；结束后非保留资源、任务、取消句柄和许可均归零。
//!
//! 这个二进制只覆盖**行为半**。性能半（release build / 4 vCPU 8 GiB / 预热 / 5 轮 /
//! 非排队网关附加耗时 p95 ≤ 10 ms）在 `src/bin/cm60-bench/main.rs`，与功能测试
//! 分开入口——两者不共享二进制。
//!
//! 夹具没有一次真实等待：账本每个入口都显式收 `now_ms`，变化点由单一原子 `seq` 定序，
//! 所以 1000 步压力链在任意负载下步序完全一致，不会有定时器竞态或偶发红。

mod cm60_pressure;
mod support;

use datazen_platform_api::ports::budget::ResourceClass;

use cm60_pressure::Change;
use cm60_pressure::{OPERATIONS, REQUEST_MS, RESERVED, SESSIONS_PER_USER, TICKS, TOTAL, USERS};

/// 前置：总额度 20、控制预留 2、单用户 100 逻辑 session、队列上限 32、短请求 10 ms。
#[test]
fn cm60_precondition_quota_shape_and_hundred_logical_sessions() {
    let report = cm60_pressure::run();

    assert_eq!(report.sessions_opened, SESSIONS_PER_USER);
    assert_eq!(report.sessions_reclaimable, SESSIONS_PER_USER);
    assert_eq!(report.submitted, OPERATIONS);
    // 100 个用户 × 10 拍，每拍一个 fake 固定 10 ms 的短请求 = 100 ms 虚拟时间，
    // 一分不多一分不少。时间由「拍」推进，不由「提交次数」推进。
    assert_eq!(report.elapsed_ms, (TICKS as u64) * REQUEST_MS);
    assert_eq!(TICKS * USERS, OPERATIONS);

    // 额度形状：`[2,2,1,0]` ⇒ 保留 5、共享 15、合计正好 20。
    let service_quota = support::quota(TOTAL, RESERVED);
    assert_eq!(service_quota.reserved_of(ResourceClass::Control), 2);
    assert_eq!(service_quota.shared, TOTAL - 5);
    assert_eq!(service_quota.shared + 5, TOTAL);
    // 保留额按类专属，且不可借用共享。
    assert!(!ResourceClass::Control.may_join_shared());
    assert!(ResourceClass::Interactive.may_join_shared());
}

/// 断言一：每个资源 journal 变化点的真实资源都不超过总额度。
///
/// 逐点判定发生在 `ResourceJournal::observe` 内部——漏掉任何一步都会当场红，
/// 不是收尾时补一句总数。
#[test]
fn cm60_assertion_1_live_resources_never_exceed_total_at_any_change_point() {
    let report = cm60_pressure::run();
    let points = report.journal.points();

    assert!(
        !report.journal.is_empty(),
        "变化点台账不能为空：否则断言是空跑"
    );
    // 判据要的是「不超过总额度」，不是「等于总额度」。这条判据的流量全是普通类
    // （Interactive/Metadata/Job，Control 一次都不提交），它能触到的天花板是
    // 共享 15 + 自类专属保留 2(Interactive) + 1(Metadata) = 18；
    // Control 的保留 2 份必须原封不动——那正是断言二在证的事。
    assert_eq!(
        report.journal.peak_live(),
        TOTAL - 2,
        "普通流量应把池子压到 15 共享 + 3 自类专属保留；峰值 {}",
        report.journal.peak_live(),
    );
    assert!(
        report.journal.peak_live() < TOTAL,
        "Control 保留额被普通流量吃掉了：峰值 {} 达到总额度 {}",
        report.journal.peak_live(),
        TOTAL,
    );
    // 定序键必须严格递增且连续——顺序只来自 `seq`，不来自时间。
    for (index, point) in points.iter().enumerate() {
        assert_eq!(point.seq, index as u64, "变化点 {index} 的定序键不连续");
        assert!(
            point.at_ms <= (OPERATIONS as u64) * REQUEST_MS,
            "变化点 {} 的虚拟时刻 {} 越界",
            point.seq,
            point.at_ms,
        );
        assert!(
            point.after.live <= TOTAL,
            "变化点 {}：真实资源 {} 超过总额度 {}",
            point.seq,
            point.after.live,
            TOTAL,
        );
    }
}

/// 断言二：普通资源不侵占控制预留。
///
/// 这条只有在「普通流量真的压上来」时才有意义：1000 次提交里没有一次投 Control，
/// 所以 Control 保留全程不动；共享水位则必须始终 ≤ `20 − 5 = 15`。
#[test]
fn cm60_assertion_2_ordinary_resources_never_encroach_on_the_control_reserve() {
    let report = cm60_pressure::run();
    let points = report.journal.points();
    let control_baseline = report.journal.control_baseline();

    assert_eq!(control_baseline, 2);
    assert!(
        report.journal.peak_shared() > 0,
        "共享池必须真的被普通流量压上去，否则「不侵占预留」是空跑"
    );
    assert_eq!(
        report.journal.peak_shared(),
        TOTAL - 5,
        "共享水位应被压到共享池上限 15"
    );
    for point in &points {
        assert!(
            point.after.shared <= TOTAL - 5,
            "变化点 {}：共享水位 {} 侵占保留",
            point.seq,
            point.after.shared,
        );
        assert!(
            point.after.control_reserved_live == 0,
            "变化点 {}：控制预留被普通资源占用 {}",
            point.seq,
            point.after.control_reserved_live,
        );
        assert_eq!(
            point.after.control_free, control_baseline,
            "变化点 {}：控制预留剩余应为 {}",
            point.seq, control_baseline,
        );
    }
}

/// 断言三：拒绝与等待有界。
///
/// 两个上界都要真的被撞到，否则「有界」只是没测出来：
/// 每类每服务队列上限 32、单用户队列上限 32（本场景用 16 个 principal，
/// 所以先撞到的是每类上限）。
#[test]
fn cm60_assertion_3_denials_and_waits_are_bounded() {
    let report = cm60_pressure::run();
    let cap = support::config(TOTAL, RESERVED).queue_cap_per_class_per_service;

    assert_eq!(cap, 32);
    assert!(
        report.queue_full > 0,
        "队列上限必须真的被撞到过，否则「等待有界」没被检验"
    );
    assert!(report.journal.peak_queued() > 0, "必须有请求真的排过队");
    for point in report.journal.points() {
        for class in ResourceClass::ALL {
            let depth = point.after.queued_by_class[class.index()];
            assert!(
                depth <= cap,
                "变化点 {}：{class:?} 等待 {depth} 超过上限 {cap}",
                point.seq,
            );
        }
        assert!(point.after.queued_total() <= 4 * cap);
    }
    // 拒绝必须是**终态可解释**的：只允许三类，且每一类都实际发生过。
    for (reason, count) in &report.denials {
        assert!(
            reason.starts_with("QueueFull")
                || reason.starts_with("NodeLost")
                || reason.starts_with("Exhausted"),
            "出现未预期的拒绝 {reason}（{count} 次）",
        );
    }
    assert!(
        report
            .denials
            .keys()
            .any(|reason| reason.starts_with("NodeLost")),
        "drain 之后必须有请求因节点失联被拒"
    );
}

/// 断言四：drain 不接新资源。
///
/// 两面都要证：被排空的节点**一份新资源都不发**，而另一个节点**继续正常服务**
/// ——后者证明 drain 是节点级的，不是把整个服务停掉。
/// 不抢占由 `run()` 在 drain 当场断言。
#[test]
fn cm60_assertion_4_drain_accepts_no_new_resources_and_preempts_nothing() {
    let report = cm60_pressure::run();
    let points = report.journal.points();

    assert!(
        report.drained.submits > 0,
        "drain 之后必须还有请求投给被排空的节点，否则这条断言是空跑"
    );
    assert_eq!(
        report.drained.granted, 0,
        "断言四：被排空的节点直批发出了 {} 份新资源",
        report.drained.granted,
    );
    assert_eq!(
        report.drained.queued, 0,
        "断言四：被排空的节点又把 {} 份请求排进了队列——排空等于不接新资源",
        report.drained.queued,
    );
    assert_eq!(
        report.drained.rejected, report.drained.node_lost,
        "被排空节点上的 {} 次拒绝里，只有 {} 次是节点失联",
        report.drained.rejected, report.drained.node_lost,
    );
    assert!(
        report.drained.node_lost > 0,
        "被排空的节点必须用「节点失联」拒掉新请求，而不是悄悄受理"
    );
    assert!(
        report.peer.submits > 0,
        "drain 之后必须有请求投给另一个节点"
    );
    assert!(
        report.peer.accepted() > 0,
        "另一个节点必须继续服务（直批 {} + 入队 {}），drain 是节点级而非服务级",
        report.peer.granted,
        report.peer.queued,
    );
    assert!(report.peer.node_lost == 0, "未排空的节点不该出现节点失联");
    // 变化点台账里必须真的记下了那一次排空，否则「drain 不接新资源」只存在于计数里。
    let drained_points = points
        .iter()
        .filter(|point| matches!(point.change, Change::NodeDrained))
        .count();
    assert_eq!(drained_points, 1, "变化点台账里必须恰好记下一次节点排空");
    assert!(
        report.live_on_drained_at_drain > 0,
        "drain 时该节点必须手上有 permit，否则「不抢占」没被检验"
    );
    assert_eq!(
        report.live_on_drained_at_drain,
        report.drain_outstanding + report.drain_pinned,
        "drain 报告应覆盖该节点全部在手 permit"
    );
    // 不抢占要覆盖的两类 permit：钉住的（活跃事务/游标）和没钉住的。
    // 两类都在场，「drain 不抢占」才不是只覆盖了容易的那一半。
    assert!(
        report.drain_pinned > 0,
        "被排空的节点上没有钉住 permit，「不抢占」没覆盖钉住的一类"
    );
    assert!(
        report.drain_outstanding > 0,
        "被排空的节点上没有普通 permit，「不抢占」没覆盖普通的一类"
    );
}

/// 断言五（收尾）：结束后非保留资源、任务、取消句柄和许可均归零。
///
/// 「非保留」是关键词：控制预留那 2 份是配置额度，不随关池消失。
#[test]
fn cm60_everything_is_zero_after_the_pool_closes() {
    let report = cm60_pressure::run();
    let final_state = report.final_watermark();
    let points = report.journal.points();

    // 收尾本身也得是一条变化点，而不是一笔悄悄的汇总。
    assert_eq!(report.journal.len(), points.len());
    assert!(
        points
            .iter()
            .any(|point| matches!(point.change, Change::PoolClosed)),
        "变化点台账里必须记下关池这一步"
    );
    assert!(
        report.admitted > 0,
        "本轮一份许可都没放行过，归零断言就是空跑"
    );
    // 许可归零。
    assert_eq!(report.leftover_permits, 0, "permit 台账仍有残留");
    assert_eq!(final_state.live, 0, "在手续数应归零");
    assert_eq!(final_state.shared, 0, "共享占用应归零");
    assert_eq!(final_state.reserved_live, 0, "保留占用应归零");
    // 每一份放行过的许可都回来了：放行 == 消费 + 归还，一张不多一张不少。
    assert_eq!(
        report.granted_total,
        report.consumed_total + report.returned_total,
        "许可流水不平：放行 {} ≠ 消费 {} + 归还 {}",
        report.granted_total,
        report.consumed_total,
        report.returned_total,
    );
    assert_eq!(report.released as u64, report.consumed_total);
    // 取消句柄归零：等待队列里一条不剩。
    assert_eq!(final_state.queued_total(), 0, "仍有请求挂在等待队列里");
    for class in ResourceClass::ALL {
        assert_eq!(
            final_state.queued_by_class[class.index()],
            0,
            "{class:?} 队列残留"
        );
    }
    // 任务归零：逻辑 session 额度已全部归还，能重新开出同样多。
    assert_eq!(report.sessions_reclaimable, report.sessions_opened);
    // 控制预留额度本身仍在（那是配置，不是残留）。
    assert_eq!(final_state.control_free, report.journal.control_baseline());
    // 取消确实发生过——否则「取消句柄」这条没有被检验。
    assert!(report.cancelled > 0, "本轮必须真的取消过等待者");
}
