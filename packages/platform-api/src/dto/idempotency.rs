//! 幂等契约：可幂等操作与提交令牌。
//!
//! §13.1：幂等键**不是**客户端随意 UUID。`BackendClient` 先经
//! `SubmissionTokenIssuer` 取签名提交令牌（本地也走同一个 port），令牌内容含随机 nonce、
//! operation、组织/principal/client、issuedAt/expiresAt、keyVersion，运行时会话操作还绑定
//! owner `runtimeEpoch`；默认有效期 24 小时，**不含秘密**。
//!
//! `createProfile` 是配置写入，不绑定会话与 runtimeEpoch，签名只限定身份、组织与操作。
//! 验证与去重的其余规则（去重 key = 令牌摘要、不相信客户端时间、过期分类、保留期）
//! 属于 `SubmissionTokenIssuer` 端口实现，不在 DTO 层。

use serde::{Deserialize, Serialize};

use crate::id::{IdempotencyKey, Timestamp};

/// 可幂等的操作。取值集合固定，新增即破坏兼容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdempotentOperation {
    /// 配置写入：不绑定会话与 runtimeEpoch。
    CreateProfile,
    OpenSession,
    ExecuteInSession,
    ExecuteAtTarget,
    SetSessionContext,
    StartJob,
}

impl IdempotentOperation {
    /// 是否绑定运行时会话纪元。
    pub fn binds_runtime_epoch(self) -> bool {
        !matches!(self, Self::CreateProfile | Self::StartJob)
    }
}

/// 签名提交令牌。`idempotency_key` 是**不透明签名令牌**，客户端不得解析或改写。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionToken {
    pub idempotency_key: IdempotencyKey,
    pub expires_at: Timestamp,
}

impl SubmissionToken {
    /// 令牌是否已过 `now` 给出的时刻。**不相信客户端时间**：
    /// 该判断只用于给用户提示，实际判定由服务端按签名与保留期做。
    pub fn is_expired_at(&self, now: &Timestamp) -> bool {
        // 时间戳是 ISO-8601 字符串，字典序即时间序（同一规范化格式下）。
        self.expires_at.as_str() <= now.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn idempotent_operation_uses_the_spec_literals() {
        let expected = [
            (IdempotentOperation::CreateProfile, "createProfile"),
            (IdempotentOperation::OpenSession, "openSession"),
            (IdempotentOperation::ExecuteInSession, "executeInSession"),
            (IdempotentOperation::ExecuteAtTarget, "executeAtTarget"),
            (IdempotentOperation::SetSessionContext, "setSessionContext"),
            (IdempotentOperation::StartJob, "startJob"),
        ];
        for (operation, literal) in expected {
            let value = serde_json::to_value(operation).expect("serialize");
            assert_eq!(value, json!(literal));
            assert_eq!(
                serde_json::from_value::<IdempotentOperation>(value).expect("deserialize"),
                operation
            );
        }
    }

    #[test]
    fn only_configuration_writes_skip_the_runtime_epoch() {
        assert!(!IdempotentOperation::CreateProfile.binds_runtime_epoch());
        assert!(!IdempotentOperation::StartJob.binds_runtime_epoch());
        assert!(IdempotentOperation::OpenSession.binds_runtime_epoch());
        assert!(IdempotentOperation::ExecuteInSession.binds_runtime_epoch());
        assert!(IdempotentOperation::ExecuteAtTarget.binds_runtime_epoch());
        assert!(IdempotentOperation::SetSessionContext.binds_runtime_epoch());
    }

    #[test]
    fn submission_token_round_trips_and_stays_opaque() {
        let token = SubmissionToken {
            idempotency_key: IdempotencyKey::new("signed.token.value"),
            expires_at: Timestamp::new("2026-01-02T00:00:00Z"),
        };
        let value = serde_json::to_value(&token).expect("serialize");
        assert_eq!(value["idempotencyKey"], json!("signed.token.value"));
        assert_eq!(value["expiresAt"], json!("2026-01-02T00:00:00Z"));
        assert_eq!(
            serde_json::from_value::<SubmissionToken>(value).expect("deserialize"),
            token
        );
    }

    #[test]
    fn expiry_comparison_uses_the_declared_iso_ordering() {
        let token = SubmissionToken {
            idempotency_key: IdempotencyKey::new("k"),
            expires_at: Timestamp::new("2026-01-02T00:00:00Z"),
        };
        assert!(!token.is_expired_at(&Timestamp::new("2026-01-01T23:59:59Z")));
        assert!(token.is_expired_at(&Timestamp::new("2026-01-02T00:00:00Z")));
        assert!(token.is_expired_at(&Timestamp::new("2026-01-03T00:00:00Z")));
    }

    #[test]
    fn idempotency_key_debug_is_redacted() {
        assert_eq!(
            format!("{:?}", IdempotencyKey::new("k")),
            "IdempotencyKey(<redacted>)"
        );
    }
}
