//! `ProfileRepository` / `ProfileReader`：连接档案（服务端词汇）的持久化端口。
//!
//! 原属 platform-api（`ports/profile.rs` + `ports/profile_reader.rs`），D2
//! （2026-10-26）起唯一定义在 `datazen-modules::connection::domain`；platform-api
//! 只再导出同一类型。
//!
//! 词汇表（§4.2）：`ProfileScope`、`ProfileRecord`、`ConfigRevision`。
//! 形状以[连接 §4.1](../../../../../docs/architecture/platform/connection-management.md)
//! 的内部 `ConnectionProfile` 为准：同列 `connectionId` 与 `secretRef`，因此
//! `connectionId` → `SecretRef` 的映射由 `ProfileRecord` 持有，`SecretProvider`
//! 只认 `SecretRef`、**永不认 `connectionId`**。
//!
//! 表单词汇（`ConnectionConfig`，桌面连接表单的 IPC 载荷）是同一聚合的另一个
//! 投影，见 [`catalog`](super::catalog)。两套词汇不互相改写：表单必须原样回显，
//! Profile 视图不得携带可解密材料。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::shared::context::RequestContext;
use crate::shared::error::PortError;
use crate::shared::id::{
    ConfigRevision, ConnectionId, CredentialRevision, IdempotencyKey, NetworkRouteRef,
    NetworkRouteRevision, OrganizationId, SecretRef, Timestamp,
};

use super::target::NamespaceTarget;

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
/// 回显视图投影（`ProfileView::from_record`）在 application 层。
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

    /// 是否已配置凭据。只表达布尔，不泄漏是否存在、长度或内容。
    pub fn credential_configured(&self) -> bool {
        self.secret_ref.is_some()
    }
}

/// 新建连接的配置草稿。`credentials` 是**只写**字段：
/// 落进 `SecretProvider` 后永不回显，也不进 `ProfileView`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraft {
    pub name: String,
    pub driver_id: String,
    pub initial_namespace: NamespaceTarget,
    pub public_options: BTreeMap<String, serde_json::Value>,
    /// 只写。序列化出去的草稿即视为已提交，不得再回写视图。
    pub credentials: Option<BTreeMap<String, String>>,
}

impl ProfileDraft {
    pub fn new(name: impl Into<String>, driver_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            driver_id: driver_id.into(),
            initial_namespace: NamespaceTarget::default(),
            public_options: BTreeMap::new(),
            credentials: None,
        }
    }

    pub fn with_initial_namespace(mut self, namespace: NamespaceTarget) -> Self {
        self.initial_namespace = namespace;
        self
    }

    pub fn with_public_option(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.public_options.insert(key.into(), value);
        self
    }

    /// 草稿的凭据是否已填写。
    pub fn has_credentials(&self) -> bool {
        self.credentials.as_ref().is_some_and(|c| !c.is_empty())
    }

    /// 清除凭据，用于把草稿转成可回显的部分。
    ///
    /// 用例在返回任何响应前必须先调用它，避免凭据经 `Debug`/序列化泄漏到日志或事件。
    pub fn without_credentials(mut self) -> Self {
        self.credentials = None;
        self
    }
}

/// 配置补丁。`driverId` 不可改：driver 决定 namespaceShape 与能力，
/// 换 driver 等价于换一条配置，不做原地修改。
pub type ProfilePatch = ProfileDraftPatch;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraftPatch {
    pub name: Option<String>,
    pub initial_namespace: Option<NamespaceTarget>,
    pub public_options: Option<BTreeMap<String, serde_json::Value>>,
    pub credentials: Option<BTreeMap<String, String>>,
}

impl ProfileDraftPatch {
    /// 补丁里是否带了敏感字段。`SecretProvider` 需要凭据时才轮换。
    pub fn touches_credentials(&self) -> bool {
        self.credentials.as_ref().is_some_and(|c| !c.is_empty())
    }
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
    /// 物理会话按[连接 §7.5](../../../../../docs/architecture/platform/connection-management.md)
    /// 关闭，**不由本端口触发**。
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

/// 连接档案的只读窄端口（原 platform-api `ProfileReader`）。
///
/// 与 [`ProfileRepository`] 读同一份事实，但**只暴露读**：注入这个端口的组件
/// 在类型层面就写不了档案。写路径只留在 `ProfileRepository`；实现方
/// （桌面的 `DesktopProfiles`）同时实现两者，事实仍然只有一份。
/// 按**聚合**拆、**按读写再切一刀也不合并**。
///
/// 边界：失败一律 `Result<_, PortError>`；端口不重试、不鉴权、不缓存；
/// 越组织访问按「不存在」处理（`None` / 空列表），不泄漏「存在但无权」（CM-05）。
#[async_trait]
pub trait ProfileReader: Send + Sync + 'static {
    /// 按 `connectionId` 读档案。缺失或越组织均返回 `None`——**不是** `NotFound`。
    ///
    /// 身份参数是**持久化**的 `ConnectionId`；运行时会话 id（`DbSessionId`）在这里
    /// 不可表达（`docs/architecture/naming.md` 的 ID 术语规范）。
    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError>;

    /// 列出作用域内的全部档案。越组织返回**空列表**而不是错误（CM-05）。
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::{ClientInstanceId, Counter, PrincipalId, RequestId};
    use serde_json::json;
    use std::sync::Arc;

    fn ctx() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    /// 端口必须**对象安全**：能以 `Arc<dyn Trait>` 注入（§4.1 的注入形态）。
    ///
    /// `?Sized` 是承重的：`dyn Trait` 本身不是 `Sized`，少了它这个断言退化成
    /// 「某个 `Sized` 类型满足这几个约束」，与对象安全无关。
    fn assert_object_safe<T: Send + Sync + 'static + ?Sized>() {}

    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    struct StubProfiles;

    #[async_trait]
    impl ProfileReader for StubProfiles {
        async fn get(
            &self,
            _ctx: &RequestContext,
            _connection_id: ConnectionId,
        ) -> Result<Option<ProfileRecord>, PortError> {
            Ok(None)
        }

        async fn list(
            &self,
            _ctx: &RequestContext,
            _scope: ProfileScope,
        ) -> Result<Vec<ProfileRecord>, PortError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn scope_comes_from_the_adapter_built_context_only() {
        let scope = ProfileScope::from_context(&ctx());
        assert_eq!(scope.organization_id, OrganizationId::new("org-1"));
    }

    #[test]
    fn profile_record_keys_on_the_persisted_connection_id_not_a_db_session_id() {
        // 编译期保证：ProfileRecord 的身份字段是 ConnectionId，没有 DbSessionId。
        let record = ProfileRecord::new(
            OrganizationId::new("org-1"),
            ConnectionId::new("conn-1"),
            "postgres",
        );
        assert_eq!(record.connection_id, ConnectionId::new("conn-1"));
    }

    #[test]
    fn missing_secret_ref_means_no_configured_credentials() {
        let mut record = ProfileRecord::new(
            OrganizationId::new("org-1"),
            ConnectionId::new("conn-1"),
            "postgres",
        );
        record.secret_ref = None;
        assert!(!record.credential_configured());
    }

    #[test]
    fn profile_draft_credentials_are_write_only_and_clearable() {
        let draft = ProfileDraft {
            credentials: Some(BTreeMap::from([(
                "password".to_owned(),
                "s3cret".to_owned(),
            )])),
            ..ProfileDraft::new("prod", "postgres")
        };
        assert!(draft.has_credentials());

        let echoed = draft.clone().without_credentials();
        assert!(!echoed.has_credentials());
        let value = serde_json::to_value(&echoed).expect("serialize");
        assert_eq!(value["credentials"], json!(null));
        assert!(serde_json::to_string(&echoed)
            .expect("serialize")
            .find("s3cret")
            .is_none());
    }

    #[test]
    fn profile_draft_round_trips_with_initial_namespace() {
        let draft = ProfileDraft::new("prod", "postgres").with_initial_namespace(
            NamespaceTarget::new(Some("app".into()), None, Some("public".into()), vec![]),
        );
        let value = serde_json::to_value(&draft).expect("serialize");
        assert_eq!(value["initialNamespace"]["schema"], json!("public"));
        assert_eq!(
            serde_json::from_value::<ProfileDraft>(value).expect("deserialize"),
            draft
        );
    }

    #[test]
    fn profile_patch_cannot_change_the_driver() {
        let patch = ProfilePatch {
            name: Some("renamed".into()),
            ..ProfilePatch::default()
        };
        let value = serde_json::to_value(&patch).expect("serialize");
        assert!(value.get("driverId").is_none(), "driverId 不可改");
        assert!(!patch.touches_credentials());
        assert_eq!(
            serde_json::from_value::<ProfilePatch>(value).expect("deserialize"),
            patch
        );
    }

    #[test]
    fn counter_versions_stay_distinct_from_the_shared_vocabulary() {
        let record = ProfileRecord::new(
            OrganizationId::new("org-1"),
            ConnectionId::new("conn-1"),
            "postgres",
        );
        assert_eq!(record.config_revision, ConfigRevision::new(0));
        assert_eq!(record.config_revision.counter(), Counter::ZERO);
        assert_ne!(
            std::any::TypeId::of::<ConfigRevision>(),
            std::any::TypeId::of::<CredentialRevision>()
        );
    }

    #[tokio::test]
    async fn the_reader_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn ProfileReader>();
        let reader: Arc<dyn ProfileReader> = Arc::new(StubProfiles);
        let record = reader
            .get(&ctx(), ConnectionId::new("conn-1"))
            .await
            .expect("读取失败必须是端口事实");
        assert!(record.is_none());
        let records = reader
            .list(&ctx(), ProfileScope::new(OrganizationId::new("org-1")))
            .await
            .expect("列举失败必须是端口事实");
        assert!(records.is_empty());
    }

    #[test]
    fn the_reader_declares_no_write_path() {
        let source = code_only(include_str!("profile.rs"));
        let production = source.split("#[cfg(test)]").next().unwrap_or_default();
        let declaration = production
            .split("pub trait ProfileReader")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        assert_eq!(
            declaration.matches("async fn").count(),
            2,
            "只读端口只允许两个方法：{declaration}"
        );
        for forbidden in [
            "create",
            "compare_and_set",
            "disable",
            "advance_credential_revision",
        ] {
            assert!(
                !declaration.contains(forbidden),
                "只读端口不得出现写方法 `{forbidden}`：{declaration}"
            );
        }
    }

    #[test]
    fn the_reader_reuses_the_repository_vocabulary_instead_of_reshaping_it() {
        // 只读端口复用既有词汇表：档案记录与作用域都来自本模块的仓储定义，
        // 没有另起一份同义形状。
        let source = code_only(include_str!("profile.rs"));
        // 只扫生产代码：`#[cfg(test)]` 之后是测试自身，避免自引用误计。
        let production = source.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(production.contains("use super::target::NamespaceTarget;"));
        assert_eq!(
            production.matches("pub struct ProfileScope").count(),
            1,
            "作用域只有一份定义"
        );
    }
}
