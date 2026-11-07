//! `IdentityResolver` 的服务端适配器（WR-D4 执行接线）。
//!
//! 端口语义（`datazen::execution::application::identity`）：身份键是**服务端
//! 生成、不可伪造**的稳定键；客户端传入的用户名/角色/principal 一律不进
//! `ExecutionIdentity`（CM §4.1）。本适配器的裁决：
//!
//! - `execution_identity_key` = HMAC-SHA256(server_secret 派生钥，
//!   `"execution-identity:{org}|{connection}|{isolation}"`)——同一
//!   `(组织, 连接, 隔离)` 稳定（INV-02 会话连续性），跨组织/连接不可互推；
//!   委托身份走独立命名空间（`"delegation-identity:{org}|{delegation}"`），
//!   与本人身份**不是**同一条路径（CM-06）。
//! - `policy_isolation_key` 委托 [`PolicyService::policy_isolation_key`]（§6.2
//!   的唯一实现处）；本适配器不自行做隔离键 HMAC。
//! - `credential_revision` = 该连接秘密材料在 [`ServerSecretVault`] 里的当前
//!   版本（`stored_secret.current_revision`；无材料行 = 0）。凭据轮换推进
//!   版本号，旧物理连接按 PoolKey 语义退出池。
//! - **登记（主线接线项）**：`resolve` 的隔离键输入取
//!   `policy_ref_revision = 0`、`read_only = false` 的快照位——执行上下文的
//!   只读标记与策略引用版本随 WR-D6 的执行入口接线一起进来，届时改从
//!   请求上下文取值。
//!
//! 委托路径（`resolve_delegation`）落在 `execution_service_identity` 表
//! （v7）：行必须存在、未吊销、未过期、scope 与请求声明**逐字相等**、目标
//! 连接以行内 `target_connection_id` 为准——委托主体不得凭委托继承调用者
//! 的全量权限（CM-06）。

use std::sync::Arc;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use rusqlite::OptionalExtension;
use sha2::Sha256;

use datazen::execution::application::identity::{ExecutionIdentity, IdentityResolver};
use datazen::execution::domain::ExecutionTarget;
use datazen::shared::clock::Clock;
use datazen::shared::context::{DelegationRef, RequestContext};
use datazen::shared::error::PortError;
use datazen::shared::id::{
    ConnectionId, CredentialRevision, ExecutionIdentityKey, PolicyIsolationKey, Timestamp,
};

use crate::execution::secret_provider::ServerSecretVault;
use crate::policy::PolicyService;
use crate::storage::AdminStore;

type HmacSha256 = Hmac<Sha256>;

/// 密钥派生标签：身份键命名空间（与 vault 的封装钥/完整性钥互不相关）。
const IDENTITY_KEY_LABEL: &[u8] = b"datazen:server:execution-identity:v1";

/// 委托身份的服务端适配器。一个进程一份（单进程 v1）。
pub struct ServerIdentityResolver {
    store: Arc<AdminStore>,
    policy: Arc<PolicyService>,
    vault: Arc<ServerSecretVault>,
    clock: Arc<dyn Clock>,
    mac_key: Vec<u8>,
}

impl std::fmt::Debug for ServerIdentityResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerIdentityResolver")
            .field("store", &"AdminStore")
            .field("policy", &"PolicyService")
            .field("vault", &"ServerSecretVault")
            .field("clock", &"Clock")
            .field("mac_key", &"[redacted]")
            .finish()
    }
}

impl ServerIdentityResolver {
    /// 构造。`key_material` 与 vault 同源（宿主注入的 server 原料）；空原料
    /// fail-closed（空钥 HMAC 是公开常量）。
    pub fn new(
        store: Arc<AdminStore>,
        policy: Arc<PolicyService>,
        vault: Arc<ServerSecretVault>,
        clock: Arc<dyn Clock>,
        key_material: &[u8],
    ) -> Result<Self, PortError> {
        if key_material.is_empty() {
            return Err(PortError::BackendUnavailable(
                "identity resolver key material is empty".into(),
            ));
        }
        let mut mac = HmacSha256::new_from_slice(key_material)
            .map_err(|_| PortError::BackendUnavailable("identity key rejected".into()))?;
        mac.update(IDENTITY_KEY_LABEL);
        Ok(Self {
            store,
            policy,
            vault,
            clock,
            mac_key: mac.finalize().into_bytes().to_vec(),
        })
    }

    /// 身份键铸造：HMAC(派生钥, 命名空间消息)。命名空间把本人身份与委托
    /// 身份分开（CM-06：不是同一条路径）；消息不含任何客户端可控词汇。
    fn mint_key(&self, message: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.mac_key)
            .expect("mac key length is fixed at construction");
        mac.update(message.as_bytes());
        let digest = mac.finalize().into_bytes();
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
    }

    /// 连接凭据的当前版本（无材料行 = 0；轮换推进）。
    fn credential_revision(&self, organization_id: &str, connection_id: &str) -> u64 {
        let secret_ref = ServerSecretVault::mint_ref("connection", organization_id, connection_id);
        self.vault
            .sync_current_revision(organization_id, &secret_ref)
            .ok()
            .flatten()
            .unwrap_or(0)
    }

    /// 隔离键快照输入（登记项：policy_ref_revision / read_only 目前取保守
    /// 默认，随 WR-D6 执行入口接线改从请求上下文取值）。
    fn resolve_isolation_key(
        &self,
        organization_id: &str,
        principal_id: &str,
    ) -> Result<PolicyIsolationKey, PortError> {
        let permission_version = self
            .policy
            .permission_version(organization_id)
            .map_err(|_| PortError::BackendUnavailable("permission version unavailable".into()))?;
        let key = self
            .policy
            .policy_isolation_key(organization_id, principal_id, 0, permission_version, false)
            .map_err(|_| {
                PortError::BackendUnavailable("policy isolation key unavailable".into())
            })?;
        Ok(PolicyIsolationKey::new(key))
    }
}

#[derive(Debug, Clone)]
struct ServiceIdentityRow {
    scope: String,
    target_connection_id: String,
    expires_at: Timestamp,
    revoked: bool,
}

fn sync_service_identity(
    store: &AdminStore,
    organization_id: &str,
    delegation_id: &str,
) -> Result<Option<ServiceIdentityRow>, PortError> {
    let conn = store
        .lock_for_adapter()
        .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
    conn.query_row(
        "SELECT scope, target_connection_id, expires_at, revoked
         FROM execution_service_identity
         WHERE organization_id = ?1 AND delegation_id = ?2",
        rusqlite::params![organization_id, delegation_id],
        |row| {
            Ok(ServiceIdentityRow {
                scope: row.get(0)?,
                target_connection_id: row.get(1)?,
                expires_at: Timestamp::new(row.get::<_, String>(2)?),
                revoked: row.get::<_, i64>(3)? != 0,
            })
        },
    )
    .optional()
    .map_err(|error| PortError::BackendUnavailable(error.to_string()))
}

#[async_trait]
impl IdentityResolver for ServerIdentityResolver {
    async fn resolve(
        &self,
        ctx: &RequestContext,
        target: &ExecutionTarget,
    ) -> Result<ExecutionIdentity, PortError> {
        let organization = ctx.organization_id.as_str().to_string();
        let connection = target.connection_id.as_str().to_string();
        let principal = ctx.principal_id.as_str().to_string();
        let isolation = self.resolve_isolation_key(&organization, &principal)?;
        let revision = self.credential_revision(&organization, &connection);
        // 身份键消息只含服务端事实：org / connection / 隔离键（隔离键本身
        // 也是服务端 HMAC 产物）——CM §4.1 的「用户名不进身份」由构造保证。
        let message = format!(
            "execution-identity:{organization}|{connection}|{}",
            isolation.as_str()
        );
        let execution_identity_key = ExecutionIdentityKey::new(self.mint_key(&message));
        Ok(ExecutionIdentity {
            organization_id: ctx.organization_id.clone(),
            connection_id: target.connection_id.clone(),
            execution_identity_key,
            policy_isolation_key: isolation,
            credential_revision: CredentialRevision::new(revision),
        })
    }

    async fn resolve_delegation(
        &self,
        _ctx: &RequestContext,
        delegation: DelegationRef,
    ) -> Result<ExecutionIdentity, PortError> {
        let organization = delegation.organization_id.as_str().to_string();
        let delegation_id = delegation.delegation_id.as_str().to_string();
        let now = self.clock.now();
        let store = self.store.clone();
        let row = tokio::task::spawn_blocking(move || {
            sync_service_identity(&store, &organization, &delegation_id)
        })
        .await
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))??;
        let row = row.ok_or_else(|| PortError::NotFound("service identity".into()))?;
        if row.revoked {
            return Err(PortError::TokenInvalid);
        }
        // 两个 Timestamp 都是定宽 RFC3339 UTC 毫秒（shared::clock 口径），
        // 字符串序 = 时刻序。
        if row.expires_at <= now {
            return Err(PortError::TokenInvalid);
        }
        // scope 逐字相等：委托主体只能落在被授予的作用域里（CM-06）。
        if row.scope != delegation.scope {
            return Err(PortError::TokenInvalid);
        }
        let organization = delegation.organization_id.as_str().to_string();
        let delegation_id = delegation.delegation_id.as_str().to_string();
        let revision = self.credential_revision(&organization, row.target_connection_id.as_str());
        let execution_identity_key = ExecutionIdentityKey::new(self.mint_key(&format!(
            "delegation-identity:{organization}|{delegation_id}"
        )));
        let isolation = PolicyIsolationKey::new(self.mint_key(&format!(
            "delegation-isolation:{organization}|{delegation_id}"
        )));
        Ok(ExecutionIdentity {
            organization_id: delegation.organization_id.clone(),
            connection_id: ConnectionId::new(row.target_connection_id),
            execution_identity_key,
            policy_isolation_key: isolation,
            credential_revision: CredentialRevision::new(revision),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::shared::context::RequestContext;
    use datazen::shared::id::{
        ClientInstanceId, DelegationId, OrganizationId, PrincipalId, RequestId,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    struct FixedClock(Timestamp);
    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            self.0.clone()
        }
    }

    struct AdvanceClock(Arc<AtomicU64>);
    impl Clock for AdvanceClock {
        fn now(&self) -> Timestamp {
            // 定宽 RFC3339 毫秒（epoch 秒直接换算，毫秒位补零）。
            let secs = self.0.load(Ordering::SeqCst);
            Timestamp::new(
                chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
                    .expect("in range")
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            )
        }
    }

    const KEY: &[u8] = b"identity-resolver-test-key-32bytes!!";

    fn store() -> Arc<AdminStore> {
        let store = AdminStore::open_in_memory().expect("store");
        store.migrate().expect("migrate");
        Arc::new(store)
    }

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

    fn target() -> ExecutionTarget {
        ExecutionTarget::new(
            ConnectionId::new("conn-1"),
            datazen::connection::domain::NamespaceTarget::database("app"),
        )
    }

    fn make_resolver(store: Arc<AdminStore>, clock: Arc<dyn Clock>) -> ServerIdentityResolver {
        let policy = Arc::new(
            PolicyService::new(store.clone(), b"policy-secret-32-bytes-minimum!!!".to_vec())
                .expect("policy"),
        );
        let vault = Arc::new(ServerSecretVault::new(store.clone(), KEY.to_vec()).expect("vault"));
        ServerIdentityResolver::new(store, policy, vault, clock, KEY).expect("resolver")
    }

    #[tokio::test]
    async fn identity_key_is_stable_and_server_generated() {
        let resolver = make_resolver(
            store(),
            Arc::new(FixedClock(Timestamp::new("2026-01-01T00:00:00.000Z"))),
        );
        let ctx = context();
        let first = resolver.resolve(&ctx, &target()).await.expect("resolve");
        let second = resolver.resolve(&ctx, &target()).await.expect("resolve");
        assert_eq!(
            first.execution_identity_key, second.execution_identity_key,
            "same inputs, same key"
        );
        // 键不含任何客户端词汇明文（HMAC 产物）。
        let rendered = format!("{:?}", first.execution_identity_key.as_str());
        assert!(!rendered.contains("user-1"));
        // 隔离维度变化 → 键变化（隔离键进消息）。
        let mut other_ctx = context();
        other_ctx.principal_id = PrincipalId::new("user-2");
        let third = resolver
            .resolve(&other_ctx, &target())
            .await
            .expect("resolve");
        assert_ne!(first.execution_identity_key, third.execution_identity_key);
    }

    #[tokio::test]
    async fn credential_revision_tracks_the_vault_version() {
        let store = store();
        let vault = Arc::new(ServerSecretVault::new(store.clone(), KEY.to_vec()).expect("vault"));
        let policy = Arc::new(
            PolicyService::new(store.clone(), b"policy-secret-32-bytes-minimum!!!".to_vec())
                .expect("policy"),
        );
        let resolver = ServerIdentityResolver::new(
            store.clone(),
            policy,
            vault.clone(),
            Arc::new(AdvanceClock(Arc::new(AtomicU64::new(1_700_000_000)))),
            KEY,
        )
        .expect("resolver");
        let ctx = context();
        let before = resolver.resolve(&ctx, &target()).await.expect("resolve");
        assert_eq!(
            before.credential_revision.counter(),
            datazen::shared::id::Counter::new(0),
            "无材料行 = 0"
        );
        let secret_ref = ServerSecretVault::mint_ref("connection", "org-1", "conn-1");
        let revision = vault
            .sync_store_material("org-1", &secret_ref, b"credentials")
            .expect("store material");
        let after = resolver.resolve(&ctx, &target()).await.expect("resolve");
        assert_eq!(
            after.credential_revision.counter(),
            revision.counter(),
            "轮换推进可见"
        );
    }

    #[tokio::test]
    async fn delegation_requires_a_live_scope_matching_row() {
        let store = store();
        let resolver = make_resolver(
            store.clone(),
            Arc::new(AdvanceClock(Arc::new(AtomicU64::new(1_700_000_000)))),
        );
        let ctx = context();
        let delegation = DelegationRef::new(
            DelegationId::new("dlg-1"),
            OrganizationId::new("org-1"),
            PrincipalId::new("svc-bot"),
            ClientInstanceId::new("client-1"),
            "workflow:run".into(),
            Timestamp::new("2026-01-01T00:00:00.000Z"),
        );
        // 无行 → NotFound（防枚举不区分「无行」与「吊销」的事实面由端口层定）。
        let missing = resolver.resolve_delegation(&ctx, delegation.clone()).await;
        assert!(matches!(missing, Err(PortError::NotFound(_))));
        // 落一行：scope 匹配、未吊销、未过期。
        let conn = store.lock_for_adapter().expect("conn");
        conn.execute(
            "INSERT INTO execution_service_identity
             (organization_id, delegation_id, service_principal, scope, target_connection_id, expires_at, revoked, created_at, updated_at)
             VALUES ('org-1','dlg-1','svc:datazen/workflow','workflow:run','conn-1','2099-01-01T00:00:00.000Z',0,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",
            [],
        )
        .expect("insert");
        drop(conn);
        let granted = resolver
            .resolve_delegation(&ctx, delegation.clone())
            .await
            .expect("granted");
        assert_eq!(
            granted.connection_id.as_str(),
            "conn-1",
            "目标连接以行内事实为准"
        );
        // scope 不匹配 → 拒绝（CM-06：只能落在被授予的作用域里）。
        let mut other_scope = delegation.clone();
        other_scope.scope = "workflow:admin".into();
        let refused = resolver.resolve_delegation(&ctx, other_scope).await;
        assert!(matches!(refused, Err(PortError::TokenInvalid)));
        // 过期 → 拒绝（时钟推过 expires_at）。
        let late = make_resolver(
            store,
            Arc::new(AdvanceClock(Arc::new(AtomicU64::new(4_102_444_800)))),
        ); // 2100-01-01
        let expired = late.resolve_delegation(&ctx, delegation).await;
        assert!(matches!(expired, Err(PortError::TokenInvalid)));
    }

    #[tokio::test]
    async fn empty_key_material_is_rejected_fail_closed() {
        let store = store();
        let policy = Arc::new(
            PolicyService::new(store.clone(), b"policy-secret-32-bytes-minimum!!!".to_vec())
                .expect("policy"),
        );
        let vault = Arc::new(ServerSecretVault::new(store.clone(), KEY.to_vec()).expect("vault"));
        let error = ServerIdentityResolver::new(
            store,
            policy,
            vault,
            Arc::new(AdvanceClock(Arc::new(AtomicU64::new(0)))),
            b"",
        );
        assert!(error.is_err(), "空钥 fail-closed");
    }
}
