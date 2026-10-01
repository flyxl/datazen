//! `ProfileRepository`：连接配置的持久化端口。
//!
//! 词汇表（§4.2）：`ProfileScope`、`ProfileRecord`、`ConfigRevision`。
//! 形状以[连接 §4.1](connection-management.md) 的内部 `ConnectionProfile` 为准：
//! 同列 `connectionId` 与 `secretRef`，因此 `connectionId` → `SecretRef` 的映射由
//! `ProfileRecord` 持有，`SecretProvider` 只认 `SecretRef`、**永不认 `connectionId`**。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::dto::profile::{ProfileDraft, ProfilePatch};
use crate::error::PortError;
use crate::id::{
    ConfigRevision, ConnectionId, CredentialRevision, IdempotencyKey, NetworkRouteRef,
    NetworkRouteRevision, OrganizationId, SecretRef, Timestamp,
};
use crate::target::NamespaceTarget;

/// 仓储作用域：组织边界。
///
/// 以组织限定查询；越组织访问返回 `None` / 空而不是错误（CM-05），
/// 端口不替调用方做越权判定，也不泄漏「存在但无权」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileScope {
    pub organization_id: OrganizationId,
}

impl ProfileScope {
    pub fn new(organization_id: OrganizationId) -> Self {
        Self { organization_id }
    }

    /// 从 `RequestContext` 取作用域。身份只能来自 adapter 构造的上下文（INV-01）。
    pub fn from_context(ctx: &RequestContext) -> Self {
        Self::new(ctx.organization_id.clone())
    }
}

/// 内部连接配置的持久化记录。
///
/// **含** `secret_ref` 与网络路由引用，因此**不得**直接作为 `ProfileView` 返回：
/// §4.1 要求列表不返回 password、token、TLS 私钥或任何可直接解密材料。
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRecord {
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    pub name: String,
    pub driver_id: String,
    pub config_revision: ConfigRevision,
    pub initial_namespace: NamespaceTarget,
    /// 可见且非敏感的配置项。
    pub driver_options: std::collections::BTreeMap<String, serde_json::Value>,
    /// 敏感字段**只允许引用 `SecretRef`**，禁止内联明文。
    pub secret_ref: Option<SecretRef>,
    pub credential_revision: CredentialRevision,
    pub network_route_ref: Option<NetworkRouteRef>,
    pub network_route_revision: NetworkRouteRevision,
    pub read_only: bool,
    pub policy_ref: Option<String>,
    pub enabled: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ProfileRecord {
    pub fn new(
        organization_id: OrganizationId,
        connection_id: ConnectionId,
        driver_id: impl Into<String>,
    ) -> Self {
        Self {
            organization_id,
            connection_id,
            name: String::new(),
            driver_id: driver_id.into(),
            config_revision: ConfigRevision::new(0),
            initial_namespace: NamespaceTarget::default(),
            driver_options: std::collections::BTreeMap::new(),
            secret_ref: None,
            credential_revision: CredentialRevision::new(0),
            network_route_ref: None,
            network_route_revision: NetworkRouteRevision::new(0),
            read_only: false,
            policy_ref: None,
            enabled: true,
            created_at: Timestamp::new(""),
            updated_at: Timestamp::new(""),
        }
    }

    /// 投影为可回显视图：剔除 `secretRef`、网络路由与内部开关，只留下非敏感配置。
    pub fn to_view(&self) -> crate::dto::profile::ProfileView {
        let mut view = crate::dto::profile::ProfileView::new(
            self.connection_id.clone(),
            self.name.clone(),
            self.driver_id.clone(),
        )
        .with_config_revision(self.config_revision.counter())
        .with_credential_revision(self.credential_revision.counter())
        .with_initial_namespace(self.initial_namespace.clone())
        .with_enabled(self.enabled);

        for (key, value) in &self.driver_options {
            if !is_sensitive_option(key) {
                view = view.with_public_option(key.clone(), value.clone());
            }
        }
        if self.secret_ref.is_some() {
            view = view.with_credential_configured();
        }
        view
    }

    /// 是否已配置凭据。只表达布尔，不泄漏是否存在、长度或内容。
    pub fn credential_configured(&self) -> bool {
        self.secret_ref.is_some()
    }
}

/// 保守的敏感字段名黑名单。
///
/// 宿主不认识某 driver 的 schema 时宁可**多剔除**：视图里少显示一个字段只是体验问题，
/// 错泄一个字段是安全事故。
fn is_sensitive_option(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    const SENSITIVE_MARKERS: [&str; 12] = [
        "password",
        "passwd",
        "secret",
        "token",
        "credential",
        "privatekey",
        "private_key",
        "certkey",
        "apikey",
        "api_key",
        "authorization",
        "signature",
    ];
    SENSITIVE_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

/// 连接配置仓储。
#[async_trait]
pub trait ProfileRepository: Send + Sync + 'static {
    /// 以组织限定查询；越组织访问返回空而不是报错（CM-05）。
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError>;

    /// 取单条配置。缺失或越组织均返回 `None`——**不是** `NotFound`。
    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError>;

    async fn create(
        &self,
        ctx: &RequestContext,
        draft: ProfileDraft,
        idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError>;

    /// 按 `expected` 保存：版本不匹配返回 `PortError::CasConflict`，端口只承诺这一个取值。
    ///
    /// 它落到哪个 `ApiError.code` 由 server host 决定：§13 code 表里的 `TargetConflict` 指
    /// 命名空间**目标**冲突，与 CAS 失败无关；`configRevision` CAS 失败由 server host 映射为
    /// 409 `ConfigRevisionMismatch`。
    async fn compare_and_set(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
        expected: ConfigRevision,
        patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError>;

    /// 禁用后不再出现在默认列表。
    ///
    /// 物理会话按[连接 §7.5](connection-management.md) 关闭，**不由本端口触发**。
    async fn disable(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<ProfileRecord, PortError>;

    async fn advance_credential_revision(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<CredentialRevision, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Counter;
    use std::collections::BTreeMap;

    fn record() -> ProfileRecord {
        let mut driver_options = BTreeMap::new();
        driver_options.insert("host".to_owned(), serde_json::json!("db.internal"));
        driver_options.insert("port".to_owned(), serde_json::json!(5432));
        driver_options.insert("sslMode".to_owned(), serde_json::json!("require"));
        driver_options.insert("password".to_owned(), serde_json::json!("s3cret"));
        driver_options.insert("privateKeyPem".to_owned(), serde_json::json!("-----BEGIN"));
        driver_options.insert("apiToken".to_owned(), serde_json::json!("tok"));
        driver_options.insert(
            "authorizationHeader".to_owned(),
            serde_json::json!("Bearer x"),
        );

        ProfileRecord {
            name: "prod".into(),
            initial_namespace: NamespaceTarget::database("app"),
            driver_options,
            secret_ref: Some(SecretRef::new("secret://vault/db-prod")),
            read_only: true,
            created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            updated_at: Timestamp::new("2026-01-01T00:00:00Z"),
            ..ProfileRecord::new(
                OrganizationId::new("org-1"),
                ConnectionId::new("conn-1"),
                "postgres",
            )
        }
    }

    fn ctx() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            crate::id::PrincipalId::new("user-1"),
            None,
            crate::id::ClientInstanceId::new("client-1"),
            crate::id::RequestId::new("req-1"),
            None,
        )
    }

    #[test]
    fn view_projection_drops_sensitive_options_and_secret_refs() {
        let view = record().to_view();
        let json = serde_json::to_string(&view).expect("serialize");
        for forbidden in [
            "s3cret",
            "BEGIN",
            "apiToken",
            "privateKeyPem",
            "authorizationHeader",
            "secretRef",
        ] {
            assert!(!json.contains(forbidden), "视图泄漏了 {forbidden}: {json}");
        }
        assert_eq!(
            view.public_options.get("host"),
            Some(&serde_json::json!("db.internal"))
        );
        assert_eq!(
            view.public_options.get("port"),
            Some(&serde_json::json!(5432))
        );
        assert_eq!(
            view.public_options.get("sslMode"),
            Some(&serde_json::json!("require"))
        );
        assert!(view.credential_configured);
    }

    #[test]
    fn view_projection_keeps_shape_fields_in_sync() {
        let record = record();
        let view = record.to_view();
        assert_eq!(view.config_revision, record.config_revision.counter());
        assert_eq!(
            view.credential_revision,
            record.credential_revision.counter()
        );
        assert_eq!(view.config_revision, Counter::new(0));
        assert_eq!(view.initial_namespace, record.initial_namespace);
        assert!(view.enabled);
    }

    #[test]
    fn sensitive_marker_matching_is_case_insensitive_and_substring_based() {
        assert!(is_sensitive_option("Password"));
        assert!(is_sensitive_option("MY_TOKEN"));
        assert!(is_sensitive_option("sslPrivateKey"));
        assert!(!is_sensitive_option("host"));
        assert!(!is_sensitive_option("port"));
        assert!(!is_sensitive_option("sslMode"));
        assert!(!is_sensitive_option("database"));
    }

    #[test]
    fn missing_secret_ref_means_no_configured_credentials() {
        let mut record = record();
        record.secret_ref = None;
        assert!(!record.credential_configured());
        assert!(!record.to_view().credential_configured);
    }

    #[test]
    fn scope_comes_from_the_adapter_built_context_only() {
        let scope = ProfileScope::from_context(&ctx());
        assert_eq!(scope.organization_id, OrganizationId::new("org-1"));
    }

    #[test]
    fn profile_record_keys_on_the_persisted_connection_id_not_a_db_session_id() {
        // 编译期保证：ProfileRecord 的身份字段是 ConnectionId，没有 DbSessionId。
        let record = record();
        assert_eq!(record.connection_id, ConnectionId::new("conn-1"));
    }
}
