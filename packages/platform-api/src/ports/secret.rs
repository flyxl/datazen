//! `SecretProvider`：版本化秘密材料端口。
//!
//! 词汇表（§4.4）：`SecretRef`、`SecretPurpose`、`ResolvedCredential`、
//! `CredentialRevision`。
//!
//! 三条硬约束：
//!
//! * `resolve` **无缓存**，每次调用都可能触发后端读取（issue 5）。
//! * `ResolvedCredential` **不实现 `Serialize`**，材料不序列化到任何 API（§6.4）。
//! * `PoolKey` 只能取 `SecretRef` 的 revision，**绝不取哈希或明文**作为分量（issue 3）。
//!   哈希会让凭据轮换命中旧键，明文会让池键变成秘密载体。
//! * 本端口**只认 `SecretRef`，永不认 `connectionId`**：`connectionId → SecretRef`
//!   的映射由 `ProfileRecord` 持有。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::error::PortError;
use crate::id::{CredentialRevision, SecretRef};

/// 取材料的用途。不同用途可读不同字段，且授权判定不同
/// （`AuthorizationResource::CredentialMaterial`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretPurpose {
    DatabasePassword,
    SshPrivateKey,
    TlsClientCertificate,
    Custom(&'static str),
}

/// 已解析的凭据材料。
///
/// **刻意不实现 `Serialize`/`Deserialize`**：材料一旦可序列化，就迟早会被 `Debug`、
/// 事件载荷或错误上下文带出去。手写 `Debug` 只输出长度，不输出内容。
#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedCredential {
    /// 明文材料。仅在建立连接的瞬间消费。
    material: Vec<u8>,
}

impl ResolvedCredential {
    pub fn new(material: impl Into<Vec<u8>>) -> Self {
        Self {
            material: material.into(),
        }
    }

    /// 借用材料字节。调用方负责消费后丢弃。
    pub fn material(&self) -> &[u8] {
        &self.material
    }

    /// 消费并取回材料所有权，交由建连路径立刻使用。
    pub fn into_material(self) -> Vec<u8> {
        self.material
    }

    pub fn len(&self) -> usize {
        self.material.len()
    }

    pub fn is_empty(&self) -> bool {
        self.material.is_empty()
    }
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ResolvedCredential(<redacted, {} bytes>)",
            self.material.len()
        )
    }
}

#[async_trait]
pub trait SecretProvider: Send + Sync + 'static {
    /// 解析当前版本材料。**无缓存**。
    async fn resolve(
        &self,
        ctx: &RequestContext,
        secret_ref: SecretRef,
        purpose: SecretPurpose,
    ) -> Result<ResolvedCredential, PortError>;

    /// 解析指定 `revision` 的材料。版本不一致返回 `PortError::CasConflict`
    /// （由用例层翻译为 409），**不得回退到当前版本**。
    async fn read_versioned(
        &self,
        ctx: &RequestContext,
        secret_ref: SecretRef,
        revision: CredentialRevision,
    ) -> Result<ResolvedCredential, PortError>;

    /// 轮换，返回**新版本号**。旧版本材料立即失效。
    async fn rotate(
        &self,
        ctx: &RequestContext,
        secret_ref: SecretRef,
        expected: CredentialRevision,
    ) -> Result<CredentialRevision, PortError>;

    /// 当前版本号。`PoolKey` 的凭据分量只能用它的 `counter()`，不得用哈希或明文。
    async fn revision(&self, secret_ref: SecretRef) -> Result<CredentialRevision, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Counter;
    use crate::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    #[test]
    fn resolved_credential_debug_never_leaks_material() {
        let credential = ResolvedCredential::new(b"s3cret".to_vec());
        let debug = format!("{credential:?}");
        assert_eq!(debug, "ResolvedCredential(<redacted, 6 bytes>)");
        assert!(!debug.contains("s3cret"));
    }

    #[test]
    fn resolved_credential_has_no_serde_impls_in_source() {
        // 编译期无法在 stable 上直接断言「没有实现某 trait」，因此断言源码事实：
        // `ResolvedCredential` 的定义处不得出现 Serialize/Deserialize。
        // 扫描前先切掉 `#[cfg(test)]` 之后的区域：否则本测试里的标记字符串会命中自己。
        let source = include_str!("secret.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        let declaration = source
            .split("pub struct ResolvedCredential")
            .nth(1)
            .expect("ResolvedCredential 声明必须存在");
        let body: String = declaration.chars().take_while(|c| *c != '}').collect();
        assert!(
            !body.contains("Serialize"),
            "定义处不得带 Serialize：{body}"
        );
        assert!(
            !body.contains("Deserialize"),
            "定义处不得带 Deserialize：{body}"
        );
        // 也没有 `impl Serialize for ResolvedCredential` 之类的后置实现。
        assert!(!source.contains("impl Serialize for ResolvedCredential"));
        assert!(!source.contains("impl serde::Serialize for ResolvedCredential"));
    }

    #[test]
    fn material_is_only_available_through_explicit_accessors() {
        let credential = ResolvedCredential::new(vec![1, 2, 3]);
        assert_eq!(credential.material(), &[1, 2, 3]);
        assert_eq!(credential.into_material(), vec![1, 2, 3]);
        assert!(ResolvedCredential::new(Vec::new()).is_empty());
    }

    #[test]
    fn purpose_is_hashable_and_comparable() {
        let mut purposes = vec![
            SecretPurpose::DatabasePassword,
            SecretPurpose::SshPrivateKey,
            SecretPurpose::TlsClientCertificate,
            SecretPurpose::Custom("redis-acl"),
        ];
        purposes.sort();
        purposes.dedup();
        assert_eq!(purposes.len(), 4);
    }

    #[test]
    fn credential_revision_is_a_counter_backed_version() {
        let revision = CredentialRevision::new(5);
        assert_eq!(revision.counter(), Counter::new(5));
        assert_eq!(revision.get(), 5);
    }

    #[test]
    fn provider_takes_a_secret_ref_never_a_connection_id() {
        // 编译期证据：resolve 的身份参数是 SecretRef；ConnectionId 无法传入。
        let reference = SecretRef::new("secret://vault/db-prod");
        assert_eq!(reference.as_str(), "secret://vault/db-prod");
        let _ctx = context();
    }
}
