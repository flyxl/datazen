//! CM-47/48：提交窗口、检查点窗口与重启禁止续写旧快照。

use super::*;

#[test]
fn cm47_48_source_changed_is_rejected() {
    let ckpt = checkpoint_with(&["source-changed"], "resumeAfterVerify", vec![boundary(10)]);
    let verdict = verify_checkpoint(&ckpt);
    assert!(
        matches!(
            verdict,
            datazen_runtime::job::RecoveryVerdict::Reject { .. }
        ),
        "source changed must reject, got {verdict:?}"
    );
}

#[test]
fn cm47_48_resume_forbidden_policy_is_rejected() {
    let ckpt = checkpoint_with(&["resume-forbidden"], "forbidAutoResume", vec![]);
    let verdict = verify_checkpoint(&ckpt);
    assert!(matches!(
        verdict,
        datazen_runtime::job::RecoveryVerdict::Reject { .. }
    ));
}

#[test]
fn cm47_48_unknown_commit_requires_target_proof() {
    // commit 应答丢失 + 无目标证明 → 人工核验。
    let ckpt = checkpoint_with(&["unknown-commit"], "resumeAfterVerify", vec![boundary(5)]);
    let verdict = verify_checkpoint(&ckpt);
    assert!(matches!(
        verdict,
        datazen_runtime::job::RecoveryVerdict::RequireManualReview { .. }
    ));
    // commit 应答丢失 + 目标已核验 → 补边界后继续。
    let ckpt = checkpoint_with(
        &["unknown-commit", "target-verified"],
        "resumeAfterVerify",
        vec![boundary(5)],
    );
    let verdict = verify_checkpoint(&ckpt);
    assert!(matches!(
        verdict,
        datazen_runtime::job::RecoveryVerdict::ResumeAfterVerify { .. }
    ));
}

#[test]
fn cm47_48_checkpoint_missing_but_committed_boundaries_preserved() {
    let ckpt = checkpoint_with(
        &["checkpoint-ack-missing"],
        "resumeAfterVerify",
        vec![boundary(7)],
    );
    let verdict = verify_checkpoint(&ckpt);
    assert!(matches!(
        verdict,
        datazen_runtime::job::RecoveryVerdict::ResumeAfterVerify { .. }
    ));
}

#[test]
fn cm47_48_cleanup_unconfirmed_requires_manual_review() {
    let ckpt = checkpoint_with(
        &["cleanup-unconfirmed"],
        "resumeAfterVerify",
        vec![boundary(3)],
    );
    let verdict = verify_checkpoint(&ckpt);
    assert!(matches!(
        verdict,
        datazen_runtime::job::RecoveryVerdict::RequireManualReview { .. }
    ));
}

#[test]
fn cm47_48_restart_never_resumes_from_bare_fingerprint() {
    // 现有仅目标 fingerprint 的 checkpoint 类型不能替代逐批证据（§7）：
    // 无 snapshot-proven / target-verified 标记时必须人工核验。
    let ckpt = checkpoint_with(&[], "resumeAfterVerify", vec![boundary(1)]);
    let verdict = verify_checkpoint(&ckpt);
    assert!(
        matches!(
            verdict,
            datazen_runtime::job::RecoveryVerdict::RequireManualReview { .. }
        ),
        "bare checkpoint must not auto-resume, got {verdict:?}"
    );
    // snapshot-proven + 每条边界均有确认来源 → 可续跑。
    let ckpt = checkpoint_with(&["snapshot-proven"], "resumeAfterVerify", vec![boundary(1)]);
    let verdict = verify_checkpoint(&ckpt);
    assert!(matches!(
        verdict,
        datazen_runtime::job::RecoveryVerdict::ResumeAfterVerify { .. }
    ));
}
