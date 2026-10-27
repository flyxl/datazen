//! 版本化 checkpoint 与 CommitBoundary 构造（§7）。
//!
//! CommitBoundary 的 `evidence` 承载「确认来源」与「行数」，payload_digest
//! 承载载荷摘要；`Checkpoint` 聚合已确认连续边界 + 源/映射证据。
//! 每个 CommitBoundary 已带显式的 batchId/payloadDigest/committedAt，不需要
//! 再从 fingerprint 字符串里解析这些键。
//!
//! 时刻不由本模块读取：`committed_at` 来自调用方注入的 [`Clock`]，
//! 生产装配给 `SystemClock`，测试给固定时钟。

use crate::jobs::job::{Checkpoint, CommitBoundary};
use crate::shared::clock::Clock;
use crate::shared::id::{JobId, StageId, Timestamp};

pub fn commit_boundary(
    clock: &dyn Clock,
    stage_id: &str,
    batch_id: &str,
    payload_digest: &str,
    rows: u64,
    evidence_source: &str,
    stable_target_fingerprint: &str,
    verified_at: Option<&str>,
) -> CommitBoundary {
    CommitBoundary {
        stage_id: StageId::new(stage_id),
        stable_target_fingerprint: stable_target_fingerprint.to_string(),
        committed_at: clock.now(),
        operation_id: None,
        batch_id: Some(batch_id.to_string()),
        payload_digest: Some(payload_digest.to_string()),
        evidence: vec![
            format!("rows={rows}"),
            format!("confirmed={evidence_source}"),
        ],
        verified_at: verified_at.map(|s| Timestamp::new(s.to_string())),
    }
}

pub fn versioned_checkpoint(
    job_id: &str,
    state_version: crate::shared::id::JobStateVersion,
    stable_target_fingerprint: &str,
    committed: Vec<CommitBoundary>,
    verification_evidence: Vec<String>,
    recovery_policy: &str,
) -> Checkpoint {
    Checkpoint {
        job_id: JobId::new(job_id),
        state_version,
        stable_target_fingerprint: stable_target_fingerprint.to_string(),
        committed,
        verification_evidence,
        recovery_policy: recovery_policy.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::clock::FixedClock;

    #[test]
    fn the_commit_timestamp_comes_from_the_injected_clock() {
        let boundary = commit_boundary(
            &FixedClock::at("2026-03-04T05:06:07.000Z"),
            "data",
            "public.users#b1",
            "sha256:payload",
            3,
            "target-commit-ack",
            "sha256:target",
            None,
        );

        assert_eq!(boundary.committed_at.as_str(), "2026-03-04T05:06:07.000Z");
        assert_eq!(
            boundary.evidence,
            vec![
                "rows=3".to_string(),
                "confirmed=target-commit-ack".to_string()
            ]
        );
    }
}
