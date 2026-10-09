//! 版本化 checkpoint 与 CommitBoundary 构造（§7）。
//!
//! CommitBoundary 的 `evidence` 承载「确认来源」与「行数」，payload_digest
//! 承载载荷摘要；`Checkpoint` 聚合已确认连续边界 + 源/映射证据。
//! 每个 CommitBoundary 已带显式的 batchId/payloadDigest/committedAt，不需要
//! 再从 fingerprint 字符串里解析这些键。

use datazen_platform_api::dto::job::{Checkpoint, CommitBoundary};
use datazen_platform_api::id::{JobId, StageId, Timestamp};

pub fn commit_boundary(
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
        committed_at: Timestamp::new(now_iso8601()),
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
    state_version: datazen_platform_api::id::JobStateVersion,
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

fn now_iso8601() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
