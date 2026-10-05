//! verify_recovery：按 §7 决策表把 checkpoint + 当前证据裁决为
//! ResumeAfterVerify / Reject / RequireManualReview。

use datazen_platform_api::dto::job::Checkpoint;
use datazen_runtime::job::RecoveryVerdict;

/// 恢复所需证据在 checkpoint.verification_evidence 中的标记协议。
/// 由准备期/应用期写入；恢复核验只读这些标记与边界载荷摘要。
pub const EVIDENCE_SNAPSHOT_PROVEN: &str = "snapshot-proven";
pub const EVIDENCE_SOURCE_CHANGED: &str = "source-changed";
pub const EVIDENCE_UNKNOWN_COMMIT: &str = "unknown-commit";
pub const EVIDENCE_TARGET_VERIFIED: &str = "target-verified";
pub const EVIDENCE_CHECKPOINT_ACK_MISSING: &str = "checkpoint-ack-missing";
pub const EVIDENCE_CLEANUP_UNCONFIRMED: &str = "cleanup-unconfirmed";
pub const EVIDENCE_RESUME_FORBIDDEN: &str = "resume-forbidden";

pub fn verify_checkpoint(checkpoint: &Checkpoint) -> RecoveryVerdict {
    let evidence = &checkpoint.verification_evidence;
    let has = |marker: &str| evidence.iter().any(|e| e.contains(marker));

    // 版本/能力变化：不自动升级。
    if has(EVIDENCE_SOURCE_CHANGED) {
        return RecoveryVerdict::Reject {
            reason: "SourceChanged: source fingerprint no longer matches the frozen plan".into(),
        };
    }
    // 恢复政策禁止：缺完整 key/快照证明时不自动续写。
    if has(EVIDENCE_RESUME_FORBIDDEN) || checkpoint.recovery_policy == "forbidAutoResume" {
        return RecoveryVerdict::Reject {
            reason: "recovery policy forbids automatic resume".into(),
        };
    }
    if has(EVIDENCE_CLEANUP_UNCONFIRMED) {
        return RecoveryVerdict::RequireManualReview {
            reason: "cleanup was not confirmed; isolated writer scope must be hand-verified".into(),
        };
    }
    // commit 应答丢失、目标已核验：补 checkpoint 后继续。
    if has(EVIDENCE_UNKNOWN_COMMIT) {
        if has(EVIDENCE_TARGET_VERIFIED) {
            return RecoveryVerdict::ResumeAfterVerify {
                resume_through: checkpoint.committed.len(),
            };
        }
        return RecoveryVerdict::RequireManualReview {
            reason: "commit acknowledgement lost and no target proof recorded".into(),
        };
    }
    // checkpoint 缺失但已提交边界存在：不重发已提交批。
    if has(EVIDENCE_CHECKPOINT_ACK_MISSING) {
        if checkpoint.committed.iter().any(|b| !b.evidence.is_empty()) {
            return RecoveryVerdict::ResumeAfterVerify {
                resume_through: checkpoint.committed.len(),
            };
        }
        return RecoveryVerdict::RequireManualReview {
            reason: "checkpoint missing and no committed boundaries to补".into(),
        };
    }
    // 正常：快照证明 + 每批都有确认来源 ⇒ 可补边界继续。
    if has(EVIDENCE_SNAPSHOT_PROVEN)
        && checkpoint
            .committed
            .iter()
            .all(|b| !b.evidence.iter().all(String::is_empty))
    {
        return RecoveryVerdict::ResumeAfterVerify {
            resume_through: checkpoint.committed.len(),
        };
    }
    RecoveryVerdict::RequireManualReview {
        reason: "insufficient recovery evidence; no automatic resume".into(),
    }
}
