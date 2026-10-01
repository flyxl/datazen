//! 端口层统一错误。
//!
//! 变体集合来自 `shared-boundaries-and-ports.md` §4.1 的目标设计，逐条落地，**不增不减**。
//!
//! 约定（概要 §6.4 与 §4.1）：
//!
//! - 端口层**只报告基础设施与并发事实**，不表达业务拒绝。权限不足、目标不合法、额度已满、
//!   会话已失效等判定由用例层完成并转成 `ApiError`。
//! - 端口不做重试、不做缓存、不做授权。缓存与版本计算是端口**提供方**的责任
//!   （例如 `SecretProvider` 负责 `credentialRevision`）。
//! - `PortError::ArtifactExpired` 是端口取值，**HTTP 状态码由 server host 决定**，
//!   不构成端口契约（team-server 出于防 ID 枚举把它与 `NotFound` 一起映射成 404）。
//! - 执行终态错误走 `ExecutionErrorCode`，与 `ApiError.code` 是两个独立命名空间。
//!
//! **`PortError` 刻意不实现 serde**（§4.1 的目标设计只 `derive(Debug, thiserror::Error)`）。
//! 原因有二：`entity: &'static str` 无法在不泄漏内存的前提下反序列化；更重要的是跨边界的
//! 错误类型是 `ApiError`（概要 §6.3、连接 §13），由 host 负责把 `PortError` 映射过去。
//! 在端口层再定一套 JSON 形态，只会给前端多一个没有消费者的错误形状。

/// 端口错误。`entity` 用 `&'static str` 承载聚合名（compile 期常量，不引入生命周期）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortError {
    /// 依赖的后端不可用（数据库、元数据库、文件系统……）。可重试由用例层判定。
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),

    /// compare-and-set 冲突：调用方拿到的版本已经不是最新，需重新读取后重试。
    #[error("compare-and-set conflict on {entity} {id}")]
    CasConflict { entity: &'static str, id: String },

    /// 资源不存在。注意 host 可能出于防 ID 枚举把它与「无权限」合并成同一响应。
    #[error("not found: {0}")]
    NotFound(String),

    /// 令牌签名/有效期/作用域/owner epoch 校验失败，或签名密钥版本未知。**未知版本一律拒绝**。
    #[error("token verification failed")]
    TokenInvalid,

    /// 端口提供方超时。预算申请的 10 秒 acquire 超时属于用例层语义，此处只报事实。
    #[error("provider timeout: {0}")]
    ProviderTimeout(String),

    /// 产物已过 TTL，读取必须失败，不得以缓存副本继续服务。
    #[error("artifact expired")]
    ArtifactExpired,

    /// 超出配额。
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),
}

impl PortError {
    /// 便于用例层统一登记的聚合/对象名，出现在错误消息里；不含机密。
    pub fn cas_conflict(entity: &'static str, id: impl Into<String>) -> Self {
        Self::CasConflict {
            entity,
            id: id.into(),
        }
    }

    /// 该错误是否属于「同一请求重发仍可能成功」的类别。
    ///
    /// 只覆盖端口事实层面的显然可重试项；**这不是重试政策**——重试政策由
    /// `ApiError.retryDisposition` 表达，且用例层必须结合「是否已建立执行记录」判断。
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::BackendUnavailable(_) | Self::ProviderTimeout(_) | Self::CasConflict { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cas_conflict_carries_entity_and_id() {
        let err = PortError::cas_conflict("profile", "conn-1");
        assert_eq!(
            err.to_string(),
            "compare-and-set conflict on profile conn-1"
        );
        assert!(err.is_transient());
    }

    #[test]
    fn variant_set_is_exactly_the_seven_from_the_spec() {
        // §4.1 的变体表是本契约的一部分：多一个「业务拒绝」变体就等于把判定放错层，
        // 少一个则端口无法表达事实。这里把集合钉死。
        let all = [
            PortError::BackendUnavailable("db".into()),
            PortError::CasConflict {
                entity: "profile",
                id: "1".into(),
            },
            PortError::NotFound("conn-1".into()),
            PortError::TokenInvalid,
            PortError::ProviderTimeout("db".into()),
            PortError::ArtifactExpired,
            PortError::QuotaExceeded("rows".into()),
        ];
        let names: Vec<&str> = all.iter().map(|e| variant_name(e)).collect();
        assert_eq!(
            names,
            [
                "BackendUnavailable",
                "CasConflict",
                "NotFound",
                "TokenInvalid",
                "ProviderTimeout",
                "ArtifactExpired",
                "QuotaExceeded",
            ]
        );
    }

    fn variant_name(error: &PortError) -> &'static str {
        match error {
            PortError::BackendUnavailable(_) => "BackendUnavailable",
            PortError::CasConflict { .. } => "CasConflict",
            PortError::NotFound(_) => "NotFound",
            PortError::TokenInvalid => "TokenInvalid",
            PortError::ProviderTimeout(_) => "ProviderTimeout",
            PortError::ArtifactExpired => "ArtifactExpired",
            PortError::QuotaExceeded(_) => "QuotaExceeded",
        }
    }

    #[test]
    fn display_messages_match_the_spec_text() {
        // §4.1 把 `#[error(...)]` 文案直接写进了契约，host 的日志与重试判断都读它。
        let cases: Vec<(PortError, &str)> = vec![
            (
                PortError::BackendUnavailable("db".into()),
                "backend unavailable: db",
            ),
            (
                PortError::CasConflict {
                    entity: "profile",
                    id: "conn-1".into(),
                },
                "compare-and-set conflict on profile conn-1",
            ),
            (PortError::NotFound("conn-1".into()), "not found: conn-1"),
            (PortError::TokenInvalid, "token verification failed"),
            (
                PortError::ProviderTimeout("db".into()),
                "provider timeout: db",
            ),
            (PortError::ArtifactExpired, "artifact expired"),
            (
                PortError::QuotaExceeded("rows".into()),
                "quota exceeded: rows",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn the_port_error_has_no_wire_shape_of_its_own() {
        // 跨边界的错误类型是 `ApiError`（application 层）。如果哪天有人给 `PortError`
        // 加了 serde，这里会失败并提醒：要么删掉，要么连同 host 的映射一起改。
        let source = include_str!("error.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        for marker in ["Serialize", "Deserialize", "serde_json"] {
            assert!(
                !source.contains(marker),
                "PortError 不应有 serde 形态，发现了 `{marker}`：{source}"
            );
        }
    }

    #[test]
    fn token_invalid_and_artifact_expired_are_not_transient() {
        assert!(!PortError::TokenInvalid.is_transient());
        assert!(!PortError::ArtifactExpired.is_transient());
        assert!(!PortError::QuotaExceeded("rows".into()).is_transient());
    }
}
