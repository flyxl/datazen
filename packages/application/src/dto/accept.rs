//! 幂等接受记录（连接 §13.1）。
//!
//! ## 落盘什么、不落盘什么
//!
//! 原文：「执行与 Job 接受记录持久化时只保存请求摘要、`executionId`/`jobId`、稳定目标/owner、
//! 状态和 receipt 的可落盘投影，**禁止保存原请求里的 `SessionHandle`**」。
//!
//! 因此本模块的记录类型**不接受** `OpenSessionRequest` / `ExecuteInSessionRequest`，
//! 只接受摘要 + 已确定身份。构造路径是「先算出摘要，再逐字段填」，没有把请求整体塞进去的入口。
//!
//! ## 保留窗口
//!
//! 「记录至少保留到 `expiresAt + 24h`」由存储层执行，因此这里把窗口长度显式建模为
//! [`AcceptRetention`]，而不是让调用方各自记一个 24。

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::idempotency::IdempotentOperation;
use datazen_platform_api::id::{
    ExecutionId, IdempotencyKey, JobId, OrganizationId, PrincipalId, Timestamp,
};
use datazen_platform_api::target::CanonicalTarget;
use serde::{Deserialize, Serialize};

/// 接受记录的主体：一次接受要么立了执行记录，要么立了 Job 记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AcceptedSubject {
    /// 已登记执行记录（`executeInSession` / `executeAtTarget`）。
    Execution { execution_id: ExecutionId },
    /// 已登记 Job（`startJob`）。
    Job { job_id: JobId },
}

/// 落盘的幂等接受记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdempotencyAcceptRecord {
    /// 幂等键本身（opaque，`Debug` 脱敏）。去重按**令牌摘要**比较，不按原请求体。
    pub idempotency_key: IdempotencyKey,
    /// 幂等操作。
    pub operation: IdempotentOperation,
    /// 归属组织。
    pub organization_id: OrganizationId,
    /// 归属主体。
    pub principal_id: PrincipalId,
    /// 请求摘要（规范化请求 + 身份的稳定指纹）。同键不同摘要 = `idempotencyConflict`。
    pub request_digest: String,
    /// 已建立的主体。
    pub subject: AcceptedSubject,
    /// 稳定目标指纹（规范化后的目标，不是展示名）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_target: Option<CanonicalTarget>,
    /// 归属（枚举；不含句柄）。
    pub owner: OwnerRef,
    /// 记录时刻。
    pub recorded_at: Timestamp,
    /// 令牌过期时刻。
    pub expires_at: Timestamp,
}

impl IdempotencyAcceptRecord {
    /// 构造接受记录。`digest` 必须是规范化请求的稳定指纹，不允许塞入原始请求体。
    // 10 个参数是刻意设计：`new` 是幂等接受记录的公共构造器，每个参数对应记录上的一个
    // 具名字段，且 `request_digest` 取 `impl Into<String>` 让调用点直接传 `&str` 或 `String`。
    // 收成参数结构体属于契约变更，会同时失效所有既有的位置调用点。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        idempotency_key: IdempotencyKey,
        operation: IdempotentOperation,
        organization_id: OrganizationId,
        principal_id: PrincipalId,
        request_digest: impl Into<String>,
        subject: AcceptedSubject,
        stable_target: Option<CanonicalTarget>,
        owner: OwnerRef,
        recorded_at: Timestamp,
        expires_at: Timestamp,
    ) -> Self {
        Self {
            idempotency_key,
            operation,
            organization_id,
            principal_id,
            request_digest: request_digest.into(),
            subject,
            stable_target,
            owner,
            recorded_at,
            expires_at,
        }
    }

    /// 归属的执行 id；非执行类操作返回 `None`。
    pub fn execution_id(&self) -> Option<&ExecutionId> {
        match &self.subject {
            AcceptedSubject::Execution { execution_id } => Some(execution_id),
            AcceptedSubject::Job { .. } => None,
        }
    }

    /// 同一令牌是否对应同一请求（去重判定，连接 §13.1）。
    ///
    /// 「同键不同请求」必须报 `idempotencyConflict` 而不是静默复用旧结果。
    pub fn matches_digest(&self, digest: &str) -> bool {
        self.request_digest == digest
    }
}

/// 幂等接受记录的保留窗口（连接 §13.1：至少保留到 `expiresAt` 之后 24 小时）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptRetention {
    /// 过期后的额外保留小时数。
    pub after_expiry_hours: u64,
}

impl AcceptRetention {
    /// 连接 §13.1 规定的下限。
    pub const MINIMUM_AFTER_EXPIRY_HOURS: u64 = 24;

    /// 默认保留窗口。
    pub const fn default_window() -> Self {
        Self {
            after_expiry_hours: Self::MINIMUM_AFTER_EXPIRY_HOURS,
        }
    }

    /// 是否满足「至少 24 小时」的下限。存储实现应当在写盘前拒绝更短的窗口。
    pub fn meets_minimum(&self) -> bool {
        self.after_expiry_hours >= Self::MINIMUM_AFTER_EXPIRY_HOURS
    }
}

impl Default for AcceptRetention {
    fn default() -> Self {
        Self::default_window()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::{ClientInstanceId, EditorSessionId};

    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn owner() -> OwnerRef {
        OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("editor-1"),
        }
    }

    fn record() -> IdempotencyAcceptRecord {
        IdempotencyAcceptRecord::new(
            IdempotencyKey::new("idem-1"),
            IdempotentOperation::ExecuteInSession,
            OrganizationId::new("org-1"),
            PrincipalId::new("principal-1"),
            "digest-abcd",
            AcceptedSubject::Execution {
                execution_id: ExecutionId::new("exec-1"),
            },
            None,
            owner(),
            Timestamp::new("2026-01-01T00:00:00Z"),
            Timestamp::new("2026-01-02T00:00:00Z"),
        )
    }

    #[test]
    fn the_accept_record_has_no_field_that_can_hold_a_session_handle() {
        let source = code_only(include_str!("accept.rs"));
        let body = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string();
        let declaration = body
            .split("pub struct IdempotencyAcceptRecord {")
            .nth(1)
            .and_then(|rest| rest.split("\n}").next())
            .expect("accept record declaration")
            .to_string();
        for forbidden in [
            "db_session_id",
            "runtime_epoch",
            "session_handle",
            "SessionHandle",
            "DbSessionId",
            "RuntimeEpoch",
            "pub request:",
            "#[serde(flatten)]",
        ] {
            assert!(
                !declaration.contains(forbidden),
                "接受记录声明不得包含 {forbidden}"
            );
        }
        // 构造参数里同样不得出现请求 DTO。
        assert!(!body.contains("ExecuteInSessionRequest"));
        assert!(!body.contains("OpenSessionRequest"));
    }

    #[test]
    fn accept_record_round_trips_and_redacts_its_key_in_debug() {
        let original = record();
        let encoded = serde_json::to_string(&original).expect("serialize");
        let decoded: IdempotencyAcceptRecord = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, original);

        let json: serde_json::Value = serde_json::from_str(&encoded).expect("json");
        assert_eq!(json["idempotencyKey"], "idem-1");
        assert_eq!(json["operation"], "executeInSession");
        assert_eq!(json["subject"]["kind"], "execution");
        assert_eq!(json["subject"]["executionId"], "exec-1");
        assert!(json.get("stableTarget").is_none());

        let debug = format!("{original:?}");
        assert!(!debug.contains("idem-1"), "幂等键不得进入 Debug 输出");
        assert!(debug.contains("IdempotencyKey(<redacted>)"));
    }

    #[test]
    fn a_job_acceptance_has_no_execution_id() {
        let job = IdempotencyAcceptRecord::new(
            IdempotencyKey::new("idem-2"),
            IdempotentOperation::StartJob,
            OrganizationId::new("org-1"),
            PrincipalId::new("principal-1"),
            "digest-efgh",
            AcceptedSubject::Job {
                job_id: JobId::new("job-1"),
            },
            None,
            owner(),
            Timestamp::new("t0"),
            Timestamp::new("t1"),
        );
        assert!(job.execution_id().is_none());
        let encoded = serde_json::to_string(&job).expect("serialize");
        assert!(encoded.contains("\"jobId\":\"job-1\""));
        let decoded: IdempotencyAcceptRecord = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, job);
    }

    #[test]
    fn same_key_with_a_different_digest_is_detectable() {
        let original = record();
        assert!(original.matches_digest("digest-abcd"));
        assert!(!original.matches_digest("digest-other"));
    }

    #[test]
    fn retention_window_is_at_least_twenty_four_hours() {
        assert_eq!(AcceptRetention::MINIMUM_AFTER_EXPIRY_HOURS, 24);
        assert_eq!(AcceptRetention::default_window().after_expiry_hours, 24);
        assert!(AcceptRetention::default_window().meets_minimum());
        assert!(!AcceptRetention {
            after_expiry_hours: 23
        }
        .meets_minimum());
    }

    #[test]
    fn token_expiry_is_owned_by_the_issuer_not_by_this_record() {
        let original = record();
        assert_eq!(original.expires_at, Timestamp::new("2026-01-02T00:00:00Z"));
        // 记录本身不判断「现在是否过期」：那要靠令牌核验端口（SubmissionTokenIssuer）。
        assert!(!body_mentions_expiry_decision(include_str!("accept.rs")));
    }

    fn body_mentions_expiry_decision(source: &str) -> bool {
        let body = code_only(source);
        let body = body.split("#[cfg(test)]").next().unwrap_or_default();
        body.contains("is_expired_at") || body.contains("SystemTime::now")
    }
}
