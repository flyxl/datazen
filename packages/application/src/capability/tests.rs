//! 「能力缺失 ⇒ 显式错误」的判定集。
//!
//! ## 每条断言对应的真实缺陷
//!
//! `driver-capability-migration.md` §7 记录了现存缺陷：「13 of 15 drivers report a
//! successful no-op cancel today」；§6.1 的禁止条款是「**不得因为『驱动没实现』就把
//! `Unsupported` 翻译成成功**」。下面每条测试都对应其中一种具体翻车方式，
//! 不是覆盖率凑数。

use std::collections::BTreeSet;
use std::sync::Arc;

use super::{
    completion_of, CapabilityCompletion, CapabilityDomain, CapabilityGrant, CapabilityKey,
    CapabilityRegistrationError, CapabilityRegistry, CapabilityState, Declarations,
    DegradationCause, DegradePolicy,
};
use crate::error::ApiErrorCode;

const NOT_SUPPORTED: ApiErrorCode = ApiErrorCode::CapabilityUnsupported;

/// 全部能力的合法声明顺序（只解开蕴含，不多不少）。
fn declaration_order() -> Vec<CapabilityKey> {
    vec![
        CapabilityKey::StatefulSession,
        CapabilityKey::ContextObservation,
        CapabilityKey::SessionScopedHandles,
        CapabilityKey::ResetForReuse,
        CapabilityKey::PreciseCancel,
        CapabilityKey::NamespaceInPlaceSwitch,
        CapabilityKey::NamespaceReplacementSwitch,
        CapabilityKey::TransactionObservation,
        CapabilityKey::IsolationLevel,
        CapabilityKey::Savepoints,
        CapabilityKey::SnapshotPerTable,
        CapabilityKey::SnapshotPerDatabase,
        CapabilityKey::SnapshotCoordinated,
        CapabilityKey::RowRead,
        CapabilityKey::RowWrite,
        CapabilityKey::StreamingResults,
        CapabilityKey::BackupArtifact,
        CapabilityKey::RestoreFromArtifact,
    ]
}

fn fully_capable() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new("conn-full");
    registry
        .declare_all(declaration_order())
        .expect("合法顺序应被接受");
    registry
}

// ── 一、空注册表：全部 18 个键都显式报错 ────────────────────────────────

/// **核心守卫**。真实缺陷：驱动没实现却返回 `Ok(())`（迁移文档 §6.1 禁止条款第一条）。
/// 少了这条，`CapabilityRegistry::resolve` 的 `Unavailable` 分支可以随便返回 Full
/// 而无人发现。
#[test]
fn an_empty_registry_rejects_every_single_key_instead_of_assuming_support() {
    let registry = CapabilityRegistry::new("conn-empty");
    for key in CapabilityKey::ALL {
        assert_eq!(
            registry.require(key).unwrap_err().code,
            NOT_SUPPORTED,
            "{key:?} 在空注册表上必须报错"
        );
    }
}

/// 六类一个都不能少。防止后来者把某一类挪走或漏掉，而调用点仍在请求它。
#[test]
fn all_six_domains_are_reachable_from_the_key_table() {
    let covered: BTreeSet<CapabilityDomain> = CapabilityKey::ALL
        .into_iter()
        .map(CapabilityKey::domain)
        .collect();
    let expected: BTreeSet<CapabilityDomain> = CapabilityDomain::ALL.into_iter().collect();
    assert_eq!(covered, expected);
    assert_eq!(CapabilityDomain::ALL.len(), 6);
}

/// 类别闸门：某一整类的键缺失时，逐条取用必须逐条报错。
#[test]
fn a_whole_missing_domain_is_rejected_key_by_key() {
    let mut registry = CapabilityRegistry::new("conn-data-only");
    registry
        .declare_all([
            CapabilityKey::RowRead,
            CapabilityKey::RowWrite,
            CapabilityKey::StreamingResults,
        ])
        .expect("无蕴含关系，应被接受");
    for key in [
        CapabilityKey::StatefulSession,
        CapabilityKey::NamespaceInPlaceSwitch,
        CapabilityKey::Savepoints,
        CapabilityKey::SnapshotCoordinated,
        CapabilityKey::BackupArtifact,
    ] {
        assert_eq!(
            registry.require(key).unwrap_err().code,
            NOT_SUPPORTED,
            "{key:?} 未声明时必须报错"
        );
    }
}

// ── 二、部分声明：缺的那一个必须报错 ────────────────────────────────────

/// 真实缺陷：`require_all` 被写成 `for k in keys { let _ = registry.require(k); }`，
/// 循环吞掉错误后照样返回 `Ok(vec![])`——批量操作（如备份 Job）会「成功」产出一个
/// 缺了 `BackupArtifact` 的空备份。
#[test]
fn a_batched_operation_fails_when_exactly_one_of_its_capabilities_is_missing() {
    let mut registry = CapabilityRegistry::new("conn-backup-missing");
    registry.declare(CapabilityKey::RowRead).expect("应被接受");
    // 故意不声明 BackupArtifact：数据读得到，但备不出来。

    let err = registry
        .require_all(&[CapabilityKey::RowRead, CapabilityKey::BackupArtifact])
        .expect_err("缺一条就必须整体失败");
    assert_eq!(err.code, NOT_SUPPORTED);
    assert!(
        err.message.contains("backup.artifact"),
        "错误必须指明缺的是哪一条，否则调用方无法定位：{}",
        err.message
    );
}

/// 真实缺陷：批量路径遇到第一条错误就 `return Ok(默认空结果)`。
#[test]
fn a_batched_operation_succeeds_only_when_every_capability_is_present() {
    let registry = fully_capable();
    let grants = registry
        .require_all(&[CapabilityKey::RowRead, CapabilityKey::BackupArtifact])
        .expect("全部声明过时才成功");
    assert_eq!(grants.len(), 2);
    assert!(grants.iter().all(|grant| grant.is_full()));
}

// ── 三、降级不是成功 ────────────────────────────────────────────────────

/// 真实缺陷：把 `Degraded` 当成 `Available` 往上放，UI 与 `effectOutcome` 标成完成
/// （迁移文档 §6.1 要求降级必须标 `partial` 或 `unknown`）。
#[test]
fn a_degraded_capability_never_enables_the_feature_and_never_reports_full_completion() {
    let mut registry = CapabilityRegistry::new("conn-degraded");
    registry
        .declare(CapabilityKey::BackupArtifact)
        .expect("应被接受");
    registry
        .lower(
            CapabilityKey::BackupArtifact,
            DegradationCause::DeclaredOnly,
        )
        .expect("已声明的键可降级");

    assert!(!registry
        .state(CapabilityKey::BackupArtifact)
        .enables_feature());

    let policy = DegradePolicy::strict().accepting(
        CapabilityKey::BackupArtifact,
        DegradationCause::DeclaredOnly,
    );
    let grant = registry
        .resolve(CapabilityKey::BackupArtifact, &policy)
        .expect("点名的降级应被接受");
    assert!(!grant.is_full());
    assert_eq!(
        grant.completion(),
        CapabilityCompletion::Partial(DegradationCause::DeclaredOnly)
    );
    assert!(!grant.completion().is_full());
}

/// 真实缺陷：降级路径被「顺手」当成成功回给调用方。
#[test]
fn the_strict_entry_point_refuses_even_a_named_degradation() {
    let mut registry = CapabilityRegistry::new("conn-degraded-strict");
    registry
        .declare(CapabilityKey::PreciseCancel)
        .expect("应被接受");
    registry
        .lower(
            CapabilityKey::PreciseCancel,
            DegradationCause::RuntimeProbeLowered,
        )
        .expect("已声明的键可降级");

    assert_eq!(
        registry
            .require(CapabilityKey::PreciseCancel)
            .unwrap_err()
            .code,
        NOT_SUPPORTED,
        "严格取用不接受任何降级"
    );
}

/// 真实缺陷：把「接受降级」写成宽泛开关，让探测否决过的能力混过去。
/// 白名单必须**同时**点名键和原因，实际原因不符时也要挡住。
#[test]
fn a_degrade_policy_does_not_accept_a_different_cause_than_it_named() {
    let mut registry = CapabilityRegistry::new("conn-cause-mismatch");
    registry
        .declare(CapabilityKey::StreamingResults)
        .expect("应被接受");
    registry
        .lower(
            CapabilityKey::StreamingResults,
            DegradationCause::RuntimeProbeLowered,
        )
        .expect("已声明的键可降级");

    let policy = DegradePolicy::strict().accepting(
        CapabilityKey::StreamingResults,
        DegradationCause::DeclaredOnly,
    );
    assert_eq!(
        registry
            .resolve(CapabilityKey::StreamingResults, &policy)
            .unwrap_err()
            .code,
        NOT_SUPPORTED,
        "点名的原因与实际原因不符时必须报错"
    );
}

/// 真实缺陷：把「探测否决」写成「还没探测」，从而重新抬回 Full
/// （`DegradationCause::strict_default` 就是为这条存在的）。
#[test]
fn a_degrade_with_unknown_provenance_is_never_reconfirmable() {
    assert!(!DegradationCause::strict_default().is_reconfirmable());
    assert!(DegradationCause::DeclaredOnly.is_reconfirmable());
    assert!(DegradationCause::WeakerGuarantee.is_reconfirmable());
}

/// 真实缺陷：批量路径只看最后一个授权的状态，前面几个降级被吞掉。
#[test]
fn one_degraded_grant_pulls_a_whole_batch_to_partial() {
    let full = CapabilityGrant::Full {
        key: CapabilityKey::RowRead,
    };
    let degraded = CapabilityGrant::Degraded {
        key: CapabilityKey::BackupArtifact,
        cause: DegradationCause::WeakerGuarantee,
    };
    assert_eq!(completion_of(&[full, full]), CapabilityCompletion::Full);
    assert_eq!(
        completion_of(&[full, degraded]),
        CapabilityCompletion::Partial(DegradationCause::WeakerGuarantee)
    );
    assert!(!completion_of(&[full, degraded]).is_full());
}

// ── 四、注册期不变量 ────────────────────────────────────────────────────

/// 真实缺陷：声称 `snapshots = coordinated` 却不保证 `perDatabase`，
/// 快照于是能对较弱档位撒谎（连接 §5.2 的档位是单调的）。
#[test]
fn declaring_a_stronger_tier_without_its_weaker_tier_is_refused() {
    let mut registry = CapabilityRegistry::new("conn-implication");
    let err = registry
        .declare(CapabilityKey::SnapshotCoordinated)
        .expect_err("缺失蕴含必须被拒绝");
    assert_eq!(
        err,
        CapabilityRegistrationError::IncompleteImplication {
            strong: "snapshot.coordinated",
            weak: "snapshot.perDatabase",
        }
    );
    assert_eq!(
        registry.state(CapabilityKey::SnapshotCoordinated),
        CapabilityState::Unavailable,
        "失败的声明不得留下任何状态"
    );
}

/// 真实缺陷：只声明了 `sessionScopedHandles` 就以为有会话——迁移文档 §6.1 的
/// 「绝不放行」条款针对的正是「凭一个似是而非的迹象就声明固定会话」。
#[test]
fn a_session_scoped_handle_without_a_stateful_session_is_refused() {
    let mut registry = CapabilityRegistry::new("conn-handle");
    let err = registry
        .declare(CapabilityKey::SessionScopedHandles)
        .expect_err("缺失蕴含必须被拒绝");
    assert_eq!(
        err,
        CapabilityRegistrationError::IncompleteImplication {
            strong: "session.scopedHandles",
            weak: "session.stateful",
        }
    );
}

/// 真实缺陷：批量注册里后面一条失败，前面几条已经写进去了。
#[test]
fn a_failed_batch_declaration_leaves_the_registry_untouched() {
    let mut registry = CapabilityRegistry::new("conn-batch");
    let before = registry.revision();
    assert!(registry
        .declare_all([
            CapabilityKey::SnapshotPerTable,
            CapabilityKey::SnapshotCoordinated,
        ])
        .is_err());
    assert_eq!(registry.revision(), before);
    assert_eq!(
        registry.state(CapabilityKey::SnapshotPerTable),
        CapabilityState::Unavailable
    );
}

/// 真实缺陷：把「没声明过的能力」降级成 `Degraded` 以绕过不可用检查。
/// `Unavailable → Degraded` 是一次**提升**（部分可用强于完全不可用）。
#[test]
fn an_undeclared_capability_cannot_be_lowered_into_a_degraded_one() {
    let mut registry = CapabilityRegistry::new("conn-undeclared");
    let err = registry
        .lower(CapabilityKey::RowWrite, DegradationCause::DeclaredOnly)
        .expect_err("未声明的键不可降级");
    assert_eq!(
        err,
        CapabilityRegistrationError::LowerUndeclared {
            key: "data.rowWrite"
        }
    );
    assert_eq!(
        registry.state(CapabilityKey::RowWrite),
        CapabilityState::Unavailable
    );
}

/// 真实缺陷：运行期探测已经把能力否决了，后续又「想起来了」直接抬回 Full
/// （`system-overview.md:296`：探测可降低，不能无依据提升）。
#[test]
fn a_capability_denied_by_runtime_probe_cannot_be_raised_back() {
    let mut registry = CapabilityRegistry::new("conn-probe");
    registry
        .declare(CapabilityKey::PreciseCancel)
        .expect("应被接受");
    registry
        .lower(
            CapabilityKey::PreciseCancel,
            DegradationCause::RuntimeProbeLowered,
        )
        .expect("已声明的键可降级");
    let err = registry
        .confirm(CapabilityKey::PreciseCancel)
        .expect_err("探测否决过的能力不可抬回");
    assert_eq!(
        err,
        CapabilityRegistrationError::Unreconfirmable {
            key: "session.preciseCancel"
        }
    );
    assert_eq!(
        registry.state(CapabilityKey::PreciseCancel),
        CapabilityState::Degraded {
            cause: DegradationCause::RuntimeProbeLowered
        }
    );
}

/// 没有运行期证据的那一档降级可以凭新证据确认回来——这是「降级」与「否决」的区别。
#[test]
fn a_declared_only_capability_can_be_confirmed_back_to_full() {
    let mut registry = CapabilityRegistry::new("conn-confirm");
    registry
        .declare(CapabilityKey::ResetForReuse)
        .expect("应被接受");
    registry
        .lower(CapabilityKey::ResetForReuse, DegradationCause::DeclaredOnly)
        .expect("已声明的键可降级");
    registry
        .confirm(CapabilityKey::ResetForReuse)
        .expect("可重新确认");
    assert_eq!(
        registry.require(CapabilityKey::ResetForReuse).unwrap(),
        CapabilityGrant::Full {
            key: CapabilityKey::ResetForReuse
        }
    );
}

/// 真实缺陷：直接 `confirm` 一个从没声明过的键，等于「凭空的 Full」。
#[test]
fn confirming_an_undeclared_capability_is_refused() {
    let mut registry = CapabilityRegistry::new("conn-confirm-undeclared");
    assert_eq!(
        registry.confirm(CapabilityKey::BackupArtifact).unwrap_err(),
        CapabilityRegistrationError::ConfirmUndeclared {
            key: "backup.artifact"
        }
    );
}

// ── 五、声明来源缺失时失败关闭 ───────────────────────────────────────────

/// 真实缺陷：声明源解析不出连接（`None`）时被当成「没有特殊能力，即全支持」——
/// 这正是 `factory.rs:51-56` 警告的 "never 'this driver supports everything'"。
#[test]
fn an_unresolvable_declaration_source_leaves_every_capability_unavailable() {
    let declarations: Declarations = Arc::new(|_connection_id| None);
    let registry = CapabilityRegistry::from_declarations("conn-unresolvable", "c1", &declarations)
        .expect("注册表本身能建出来");
    for key in CapabilityKey::ALL {
        assert_eq!(
            registry.require(key).unwrap_err().code,
            NOT_SUPPORTED,
            "{key:?} 在声明源缺失时必须报错"
        );
    }
}

/// 声明源只报一部分能力时，没报的那些必须是错误而不是继承上一轮的结论。
#[test]
fn a_declaration_source_that_omits_keys_reports_them_missing() {
    let declarations: Declarations = Arc::new(|_| Some(vec![CapabilityKey::RowRead]));
    let registry =
        CapabilityRegistry::from_declarations("conn-partial", "c1", &declarations).unwrap();
    assert!(registry.require(CapabilityKey::RowRead).is_ok());
    for key in CapabilityKey::ALL
        .into_iter()
        .filter(|key| *key != CapabilityKey::RowRead)
    {
        assert_eq!(
            registry.require(key).unwrap_err().code,
            NOT_SUPPORTED,
            "{key:?} 未被声明时必须报错"
        );
    }
}

/// 真实缺陷：声明源连蕴含关系都不满足时，`from_declarations` 若把错误吞掉并
/// 返回一个「部分生效」的注册表，调用方会拿到一个不认识的半成品。
#[test]
fn a_declaration_source_violating_an_implication_fails_loudly() {
    let declarations: Declarations = Arc::new(|_| Some(vec![CapabilityKey::SnapshotCoordinated]));
    assert_eq!(
        CapabilityRegistry::from_declarations("conn-bad-source", "c1", &declarations)
            .expect_err("违反蕴含的声明源必须报错")
            .to_string(),
        "capability `snapshot.coordinated` implies `snapshot.perDatabase`: declare `snapshot.perDatabase` first or not at all"
    );
}

// ── 六、错误可定位且已脱敏 ──────────────────────────────────────────────

/// 真实缺陷：所有能力缺失都返回同一句错误，调用方无法判断该降级 UI、换驱动还是报错。
#[test]
fn a_capability_error_names_the_provider_and_the_key() {
    let empty = CapabilityRegistry::new("conn-7");
    let message = empty
        .require(CapabilityKey::SnapshotCoordinated)
        .unwrap_err()
        .message;
    assert!(message.contains("conn-7"), "{message}");
    assert!(message.contains("snapshot.coordinated"), "{message}");
    assert_eq!(fully_capable().provider_id(), "conn-full");
}

/// 真实缺陷：文案里带 driver 内部错误、SQL 或连接串。
/// 错误文案只允许由 provider 标识与能力字面值拼成。
#[test]
fn a_capability_error_message_carries_no_driver_detail() {
    let empty = CapabilityRegistry::new("conn-sanitized");
    let message = empty.require(CapabilityKey::RowWrite).unwrap_err().message;
    for forbidden in ["://", "password", "SELECT", "/Users/"] {
        assert!(
            !message.contains(forbidden),
            "错误文案泄漏了 `{forbidden}`：{message}"
        );
    }
}

/// 真实缺陷：降级被拒绝时的错误与「根本没声明」的错误长得一样，
/// 调用方分不清该等探测还是该直接报错。
#[test]
fn a_refused_degradation_is_distinguishable_from_a_missing_capability() {
    let mut registry = CapabilityRegistry::new("conn-distinguish");
    registry.declare(CapabilityKey::RowRead).expect("应被接受");
    registry
        .lower(CapabilityKey::RowRead, DegradationCause::WeakerGuarantee)
        .expect("已声明的键可降级");

    let refused = registry
        .require(CapabilityKey::RowRead)
        .unwrap_err()
        .message;
    let missing = registry
        .require(CapabilityKey::RowWrite)
        .unwrap_err()
        .message;
    assert!(refused.contains("降级原因"), "{refused}");
    assert!(missing.contains("未声明"), "{missing}");
    assert_ne!(refused, missing);
}
