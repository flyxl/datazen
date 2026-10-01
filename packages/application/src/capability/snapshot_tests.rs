//! 快照的记录 / 比较能力。
//!
//! ## 每条断言对应的真实缺陷
//!
//! 计划第 99 行「新增能力缺失不会 no-op 成功」需要两件东西才能被证明：
//! 一份**可比的快照**，和一个能证明「多了/少了什么」的差异函数。
//! 只有 `CapabilityUnsupported` 一句话时，「这次请求比上次多要了一条能力」
//! 和「这次驱动真的不支持」在日志上完全一样，事后无法区分。

use std::collections::BTreeMap;

use super::super::CapabilityRegistry;
use super::{
    CapabilityKey, CapabilitySnapshot, CapabilityState, DegradationCause, DegradationRecord,
};

fn snapshot_of(registry: &CapabilityRegistry) -> CapabilitySnapshot {
    registry.snapshot()
}

#[test]
fn an_empty_registry_snapshots_every_key_as_unavailable() {
    let registry = CapabilityRegistry::new("conn-empty");
    let snapshot = snapshot_of(&registry);
    assert_eq!(snapshot.provider_id, "conn-empty");
    assert!(snapshot.states.is_empty());
    assert!(snapshot.degradations.is_empty());
    for key in CapabilityKey::ALL {
        assert_eq!(
            snapshot.state_of(key),
            CapabilityState::Unavailable,
            "{key:?} 未登记时快照必须读成不可用"
        );
    }
}

#[test]
fn a_snapshot_records_which_capabilities_the_provider_declared() {
    let mut registry = CapabilityRegistry::new("conn-declared");
    registry
        .declare_all([CapabilityKey::RowRead, CapabilityKey::StreamingResults])
        .expect("无蕴含关系，应被接受");
    let snapshot = snapshot_of(&registry);
    assert_eq!(
        snapshot.state_of(CapabilityKey::RowRead),
        CapabilityState::Available
    );
    assert_eq!(
        snapshot.state_of(CapabilityKey::StreamingResults),
        CapabilityState::Available
    );
    assert_eq!(
        snapshot.state_of(CapabilityKey::BackupArtifact),
        CapabilityState::Unavailable
    );
    assert_eq!(
        snapshot.confirmed_keys(),
        vec![CapabilityKey::RowRead, CapabilityKey::StreamingResults]
    );
}

#[test]
fn a_degradation_is_recorded_in_the_timeline_with_its_revision() {
    let mut registry = CapabilityRegistry::new("conn-timeline");
    registry.declare(CapabilityKey::RowRead).expect("应被接受");
    registry
        .lower(
            CapabilityKey::RowRead,
            DegradationCause::RuntimeProbeLowered,
        )
        .expect("已声明的键可降级");
    registry
        .lower(CapabilityKey::RowRead, DegradationCause::WeakerGuarantee)
        .expect("可再次降级");

    let snapshot = snapshot_of(&registry);
    let timeline = snapshot.degradation_timeline();
    assert_eq!(timeline.len(), 2, "每一次降低都必须留下证据");
    assert!(timeline[0].at_revision <= timeline[1].at_revision);
    assert_eq!(timeline[0].cause, DegradationCause::RuntimeProbeLowered);
    assert_eq!(timeline[1].cause, DegradationCause::WeakerGuarantee);
    assert_eq!(
        snapshot.state_of(CapabilityKey::RowRead),
        CapabilityState::Degraded {
            cause: DegradationCause::WeakerGuarantee
        }
    );
}

/// 真实缺陷：快照被当成许可——回放一份旧快照把 `Degraded` 抬回 `Available`。
#[test]
fn replaying_an_older_snapshot_does_not_raise_a_degraded_capability() {
    let mut registry = CapabilityRegistry::new("conn-replay");
    registry
        .declare(CapabilityKey::PreciseCancel)
        .expect("应被接受");
    let healthy = snapshot_of(&registry);

    registry
        .lower(
            CapabilityKey::PreciseCancel,
            DegradationCause::RuntimeProbeLowered,
        )
        .expect("已声明的键可降级");
    let degraded = snapshot_of(&registry);

    let changes = healthy.difference_from(&degraded);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].key, "session.preciseCancel");
    assert_eq!(changes[0].from, CapabilityState::Available);
    assert_eq!(
        changes[0].to,
        CapabilityState::Degraded {
            cause: DegradationCause::RuntimeProbeLowered
        }
    );
    assert!(changes[0].is_regression(), "降低必须被识别为退化");
    assert!(degraded.state_of(CapabilityKey::PreciseCancel) != CapabilityState::Available);
}

/// 真实缺陷：`difference_from` 只扫一侧的键，于是「新增的能力」在比较里消失——
/// 这正是计划第 99 行要挡的 no-op。
#[test]
fn a_capability_present_only_in_the_newer_snapshot_still_shows_up_as_a_difference() {
    let mut before = CapabilityRegistry::new("conn-grow");
    before.declare(CapabilityKey::RowRead).expect("应被接受");
    let mut after = before.clone();
    after
        .declare_all([
            CapabilityKey::SnapshotPerTable,
            CapabilityKey::SnapshotPerDatabase,
            CapabilityKey::SnapshotCoordinated,
        ])
        .expect("蕴含链完整，应被接受");

    let added = snapshot_of(&before).difference_from(&snapshot_of(&after));
    let added_keys: Vec<&str> = added.iter().map(|change| change.key.as_str()).collect();
    assert_eq!(
        added_keys,
        vec![
            "snapshot.coordinated",
            "snapshot.perDatabase",
            "snapshot.perTable"
        ],
        "新增能力必须作为差异出现，不能因为基线里没有就被跳过；\
         两侧同状态的键（data.rowRead）也不许混进差异表"
    );
    assert!(added
        .iter()
        .all(|change| change.from == CapabilityState::Unavailable
            && change.to == CapabilityState::Available));
    assert!(!added.iter().any(|change| change.is_regression()));
}

/// 真实缺陷：能力被悄悄拿走（降级回不可用）后比较函数说「没变化」。
#[test]
fn a_capability_that_disappears_is_reported_as_a_regression() {
    let healthy = CapabilitySnapshot {
        provider_id: "conn".to_string(),
        revision: 3,
        states: BTreeMap::from([("data.rowRead".to_string(), CapabilityState::Available)]),
        degradations: Vec::new(),
    };
    let stripped = CapabilitySnapshot {
        provider_id: "conn".to_string(),
        revision: 4,
        states: BTreeMap::new(),
        degradations: vec![DegradationRecord {
            key: CapabilityKey::RowRead,
            cause: DegradationCause::WeakerGuarantee,
            at_revision: 4,
        }],
    };

    let changes = healthy.difference_from(&stripped);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].key, "data.rowRead");
    assert_eq!(changes[0].from, CapabilityState::Available);
    assert_eq!(changes[0].to, CapabilityState::Unavailable);
    assert!(changes[0].is_regression());
    assert!(!healthy.same_capabilities_as(&stripped));
}

/// 真实缺陷：`difference_from` 遇到**本 crate 不认识**的键字面值（来自更高版本的快照）
/// 就 panic 或跳过——前者是崩溃，后者是新的 no-op。
#[test]
fn an_unknown_key_literal_is_still_compared_rather_than_skipped() {
    let known = CapabilitySnapshot {
        provider_id: "conn".to_string(),
        revision: 1,
        states: BTreeMap::new(),
        degradations: Vec::new(),
    };
    let future = CapabilitySnapshot {
        provider_id: "conn".to_string(),
        revision: 2,
        states: BTreeMap::from([(
            "catalog.introspection".to_string(),
            CapabilityState::Available,
        )]),
        degradations: Vec::new(),
    };

    let changes = future.difference_from(&known);
    assert_eq!(changes.len(), 1, "未知键也必须进差异表");
    assert_eq!(changes[0].key, "catalog.introspection");
    assert_eq!(
        changes[0].typed_key(),
        None,
        "本 crate 不认识它，但不能因此不比较"
    );
    assert!(!known.same_capabilities_as(&future));
}

/// 快照的键字面值必须是稳定契约，否则跨版本比较无从谈起。
#[test]
fn every_key_has_a_unique_stable_wire_literal() {
    let mut literals: Vec<&str> = CapabilityKey::ALL
        .into_iter()
        .map(CapabilityKey::as_str)
        .collect();
    let total = literals.len();
    literals.sort_unstable();
    literals.dedup();
    assert_eq!(literals.len(), total, "能力键字面值必须互不相同");
    for key in CapabilityKey::ALL {
        assert_eq!(
            CapabilityKey::from_wire(key.as_str()),
            Some(key),
            "{key:?} 的字面值必须能原样解析回来"
        );
    }
    assert_eq!(
        CapabilityKey::from_wire("data.teleport"),
        None,
        "不认识的字面值必须返回 None 而不是默认到某个键"
    );
}

/// 快照要能落盘、跨进程传、并且传回来还是一份（否则比较能力形同虚设）。
#[test]
fn a_snapshot_survives_a_serde_json_round_trip() {
    let mut registry = CapabilityRegistry::new("conn-json");
    registry
        .declare_all([
            CapabilityKey::RowRead,
            CapabilityKey::RowWrite,
            CapabilityKey::PreciseCancel,
        ])
        .expect("应被接受");
    registry
        .lower(CapabilityKey::RowWrite, DegradationCause::WeakerGuarantee)
        .expect("已声明的键可降级");

    let snapshot = snapshot_of(&registry);
    let encoded = serde_json::to_string(&snapshot).expect("快照可序列化");
    let decoded: CapabilitySnapshot = serde_json::from_str(&encoded).expect("快照可反序列化");

    assert_eq!(decoded, snapshot);
    assert!(decoded.same_capabilities_as(&snapshot));
    for key in CapabilityKey::ALL {
        assert_eq!(decoded.state_of(key), snapshot.state_of(key), "{key:?}");
    }
    assert_eq!(decoded.degradation_timeline().len(), 1);
}

/// 真实缺陷：快照为空时 `difference_from_nothing` 声称「这个 provider 没有任何差异」，
/// 于是全不可用被呈现成与全可用无从区分。
#[test]
fn the_baseline_comparison_reports_every_declared_capability() {
    let mut registry = CapabilityRegistry::new("conn-baseline");
    registry
        .declare_all([CapabilityKey::RowRead, CapabilityKey::BackupArtifact])
        .expect("应被接受");

    let gains = snapshot_of(&registry).difference_from_nothing();
    assert_eq!(
        gains
            .iter()
            .map(|change| change.key.as_str())
            .collect::<Vec<_>>(),
        vec!["backup.artifact", "data.rowRead"]
    );
    assert!(gains
        .iter()
        .all(|change| change.typed_key().is_some() && change.is_regression()));

    assert!(CapabilityRegistry::new("conn-nothing")
        .snapshot()
        .difference_from_nothing()
        .is_empty());
}
