//! verify_recovery：按 §7 决策表把 checkpoint + 当前证据裁决为
//! ResumeAfterVerify / Reject / RequireManualReview。

use datazen_platform_api::dto::job::{Checkpoint, CommitBoundary};
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
            reason: "checkpoint missing and no committed boundaries to fall back on".into(),
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

/// 从「冻结体政策 + 快照证明 + 未知提交批数 + 已确认边界」推导恢复证据标记（§7）。
///
/// 证据必须由真正掌握事实的一方写出来：runtime 只看到 `StageOutcome`，看不到准备期
/// 的政策与快照证明，所以宿主导出这些标记，再交给 [`verify_checkpoint`] 裁决。
/// 不写 `checkpoint-ack-missing`：那表示 checkpoint 在管道内部丢失，只能由管道自己报。
pub fn derive_evidence(
    recovery_policy: &str,
    snapshot_proven: bool,
    unknown_commits: u64,
    committed: &[CommitBoundary],
) -> Vec<String> {
    let all_evidenced = committed.iter().all(|b| !b.evidence.is_empty());
    let mut markers = Vec::new();
    if recovery_policy != "resumeAfterVerify" {
        markers.push(EVIDENCE_RESUME_FORBIDDEN.to_string());
    }
    if unknown_commits > 0 {
        markers.push(EVIDENCE_UNKNOWN_COMMIT.to_string());
        // 应答丢失时，只有目标侧逐批核验过才敢续写，否则一律人工复核。
        if all_evidenced && !committed.is_empty() {
            markers.push(EVIDENCE_TARGET_VERIFIED.to_string());
        }
    }
    if snapshot_proven && !committed.is_empty() && all_evidenced {
        markers.push(EVIDENCE_SNAPSHOT_PROVEN.to_string());
    }
    markers
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::StageId;

    fn boundary(evidence: &[&str]) -> CommitBoundary {
        CommitBoundary {
            stage_id: StageId::new("data"),
            stable_target_fingerprint: "fp".into(),
            committed_at: "2026-07-01T00:00:00Z".into(),
            operation_id: None,
            batch_id: None,
            payload_digest: None,
            evidence: evidence.iter().map(|e| (*e).to_string()).collect(),
            verified_at: None,
        }
    }

    #[test]
    fn derive_evidence_refuses_resume_when_the_freeze_forbids_it() {
        let markers = derive_evidence("forbidAutoResume", true, 0, &[boundary(&["source-row-1"])]);
        assert!(markers.iter().any(|m| m == EVIDENCE_RESUME_FORBIDDEN));
    }

    #[test]
    fn derive_evidence_marks_unknown_commit_only_with_target_proof() {
        let with_proof =
            derive_evidence("resumeAfterVerify", true, 1, &[boundary(&["source-row-1"])]);
        assert!(with_proof.iter().any(|m| m == EVIDENCE_UNKNOWN_COMMIT));
        assert!(with_proof.iter().any(|m| m == EVIDENCE_TARGET_VERIFIED));

        let without_proof = derive_evidence("resumeAfterVerify", true, 1, &[boundary(&[])]);
        assert!(without_proof.iter().any(|m| m == EVIDENCE_UNKNOWN_COMMIT));
        assert!(!without_proof.iter().any(|m| m == EVIDENCE_TARGET_VERIFIED));
    }

    #[test]
    fn derive_evidence_requires_evidence_on_every_boundary() {
        let markers = derive_evidence(
            "resumeAfterVerify",
            true,
            0,
            &[boundary(&["source-row-1"]), boundary(&[])],
        );
        assert!(!markers.iter().any(|m| m == EVIDENCE_SNAPSHOT_PROVEN));
    }
}
