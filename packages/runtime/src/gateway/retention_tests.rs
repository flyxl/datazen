//! 保留期清扫的单元测试。
//!
//! 主线是一条不变量：`retained_until >= expires_at`，以及它在清扫处的**代码级**体现——
//! 删除之前必须先确认 `expires_at` 已过。这两条各有一条会杀掉违反者的测试：
//! [`a_grant_whose_retention_ends_before_expiry_is_never_deleted`] 造出违反不变量的
//! 数据并证明清扫拒绝；[`a_retired_grant_leaves_a_tombstone_that_outlives_the_grant`]
//! 证明即便删除真的发生了，墓碑仍在挡着重放。
//!
//! 时间全是常量纳秒，没有任何真实等待——虚拟时间只经调用方传入的 `now_nanos` 推进。

use super::*;
use crate::connection::{Counter, DbSessionId, ExecutionId};

const HOUR: u64 = 60 * 60 * 1_000_000_000;
const DAY: u64 = 24 * HOUR;

fn scope(key: &str) -> IdempotencyScope {
    IdempotencyScope::new(DbSessionId::new("dbs_cm70_0001"), Counter::new(7), key)
}

fn execution(name: &str) -> ExecutionId {
    ExecutionId::new(name)
}

// ── 不变量本身 ────────────────────────────────────────────────────────────

#[test]
fn the_production_constructor_always_keeps_the_grant_past_expiry() {
    for expires_at in [0_u64, 1, HOUR, DAY, u64::MAX - 1, u64::MAX] {
        let grant = Grant::for_execution(execution("e1"), scope("k"), 0, expires_at);
        assert!(
            grant.retained_until_nanos() >= grant.expires_at_nanos(),
            "expires_at={expires_at} 造出了 retained_until < expires_at"
        );
        assert_eq!(grant.expires_at_nanos(), expires_at);
    }
}

#[test]
fn the_production_constructor_keeps_a_whole_retention_window_after_expiry() {
    let grant = Grant::for_execution(execution("e1"), scope("k"), 0, DAY);
    assert_eq!(
        grant.retained_until_nanos(),
        DAY + RETENTION_AFTER_EXPIRY_NANOS,
        "保留窗口就是 expires_at 之后整整一段，不是别的数"
    );
    assert_eq!(
        RETENTION_AFTER_EXPIRY_NANOS,
        24 * HOUR,
        "保留窗口与 24 小时一致"
    );
}

/// **杀掉违反者的测试。**
///
/// 造一条 `retained_until < expires_at` 的授予（生产构造做不到，只能走测试专用口子），
/// 然后在「保留期已到、令牌还没过期」的时刻清扫：必须**一个都不删**，
/// 还得在报告里点名。
#[test]
fn a_grant_whose_retention_ends_before_expiry_is_never_deleted() {
    let registry = GrantRegistry::new();
    let expires_at = 10 * DAY;
    let retained_until = 5 * DAY; // 违反 retained_until >= expires_at
    registry.record(
        "violator",
        Grant::with_retained_until_for_test(
            execution("e_violate"),
            scope("violator"),
            expires_at,
            retained_until,
        ),
    );

    // 在保留期已过、令牌仍未过期的那一刻清扫。
    let report = registry.sweep(retained_until);
    assert!(
        report.deleted.is_empty(),
        "令牌未过期就删了记录，正是禁止的那件事：{:?}",
        report.deleted
    );
    assert_eq!(
        report.refused,
        vec!["violator".to_owned()],
        "拒绝必须被点名，否则运维看不到"
    );
    assert!(registry.grant("violator").is_some(), "授予必须还在");
    assert!(!registry.is_retired("violator"), "没有删除就不该有墓碑");

    // 再往前推一秒、往后推一大截，未过期期间**任何时刻**都不删。
    for now in [retained_until, retained_until + 1, expires_at - 1] {
        let report = registry.sweep(now);
        assert!(report.deleted.is_empty(), "now={now} 提前删了未过期的授予");
        assert!(registry.grant("violator").is_some(), "now={now} 授予丢了");
    }

    // 只有令牌自己也过期了，才终于可以删。
    let report = registry.sweep(expires_at);
    assert_eq!(report.deleted.len(), 1, "两个条件都满足后应当删除");
    assert!(report.refused.is_empty(), "不再有违反者");
}

// ── 清扫的时序 ────────────────────────────────────────────────────────────

#[test]
fn a_grant_survives_until_both_the_expiry_and_the_retention_window_have_passed() {
    let registry = GrantRegistry::new();
    let expires_at = DAY;
    let retained_until = expires_at + RETENTION_AFTER_EXPIRY_NANOS;
    registry.record(
        "k1",
        Grant::for_execution(execution("e1"), scope("k1"), 0, expires_at),
    );

    for now in [0, expires_at - 1, expires_at, retained_until - 1] {
        let report = registry.sweep(now);
        assert!(
            report.deleted.is_empty(),
            "now={now} 删早了：过期只是必要条件，保留期也是"
        );
        assert!(registry.grant("k1").is_some(), "now={now} 授予丢了");
    }

    let report = registry.sweep(retained_until);
    assert_eq!(report.deleted.len(), 1);
    assert_eq!(report.deleted[0].digest, "k1");
    assert_eq!(report.deleted[0].execution_id, execution("e1"));
    assert_eq!(
        report.deleted[0].scope,
        scope("k1"),
        "退役项必须带着账本作用域，否则账本里那条记录永远删不掉"
    );
    assert!(registry.is_empty());
}

#[test]
fn a_retired_grant_leaves_a_tombstone_that_outlives_the_grant() {
    let registry = GrantRegistry::new();
    let expires_at = DAY;
    let retained_until = expires_at + RETENTION_AFTER_EXPIRY_NANOS;
    registry.record(
        "k1",
        Grant::for_execution(execution("e1"), scope("k1"), 0, expires_at),
    );

    let report = registry.sweep(retained_until);
    assert_eq!(report.deleted.len(), 1);
    assert_eq!(
        report.tombstones_pruned, 0,
        "剪枝必须排在删除之前：这趟刚立的墓碑不能被自己剪掉"
    );
    assert!(registry.grant("k1").is_none(), "授予已删");
    assert!(
        registry.is_retired("k1"),
        "墓碑必须留下：删除提前发生时，它是唯一的拦截"
    );

    // 墓碑活到令牌自己到期为止——之后过期判定接手，不必再占位置。
    let report = registry.sweep(expires_at - 1);
    assert_eq!(report.tombstones_pruned, 0);
    assert!(registry.is_retired("k1"));

    let report = registry.sweep(expires_at);
    assert_eq!(report.tombstones_pruned, 1);
    assert!(!registry.is_retired("k1"));
    assert!(report.deleted.is_empty() && report.refused.is_empty());
    assert!(
        registry.sweep(expires_at).is_empty(),
        "墓碑剪掉之后报告才真的空"
    );
}

#[test]
fn a_sweep_of_an_empty_registry_reports_nothing() {
    let registry = GrantRegistry::new();
    assert!(registry.is_empty());
    let report = registry.sweep(u64::MAX);
    assert!(report.is_empty());
    assert!(report.deleted.is_empty());
    assert!(report.refused.is_empty());
    assert_eq!(report.tombstones_pruned, 0);
}

// ── 早退路径（测试专用口子 + 毒化锁） ──────────────────────────────────────

#[test]
fn force_retire_removes_the_grant_immediately_and_leaves_a_tombstone() {
    let registry = GrantRegistry::new();
    registry.record(
        "k1",
        Grant::for_execution(execution("e1"), scope("k1"), 0, DAY),
    );
    assert!(registry.force_retire("k1", 1));
    assert!(registry.grant("k1").is_none());
    assert!(registry.is_retired("k1"));
    assert!(
        !registry.force_retire("k1", 2),
        "重复退役必须如实报告没删掉"
    );
}

#[test]
fn a_poisoned_registry_fails_closed_on_the_tombstone_question() {
    let registry = GrantRegistry::new();
    registry.record(
        "k1",
        Grant::for_execution(execution("e1"), scope("k1"), 0, DAY),
    );
    // 在持锁时 panic，把锁弄成中毒态。
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry.poison_for_test();
    }));
    assert!(poisoned.is_err(), "poison_for_test 必须真的 panic");

    assert!(
        registry.is_retired("k1"),
        "锁中毒时读不出墓碑，就必须按『已退役』处理——拒绝比放行安全"
    );
    assert!(
        registry.is_retired("从未登记过的摘要"),
        "读不出来一律当已退役"
    );
    assert_eq!(registry.len(), 0, "读不出来时计数为 0，而不是猜一个数");
    assert!(registry.grant("k1").is_none());
    assert!(
        registry.sweep(u64::MAX).is_empty(),
        "锁中毒时清扫什么都不做——绝不能在读不出到期时刻时还去删"
    );
}
