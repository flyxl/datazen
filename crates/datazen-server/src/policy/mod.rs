//! `PolicyService`：授权引擎（web-server-host §6）——角色基线 + 资源 ACL +
//! 权限版本 + `policyIsolationKey` 派生。
//!
//! 组成（§2.1 目标目录的 `policy/` 三件，delegation 随 §6.4 切片后补）：
//!
//! - [`rbac`]：六内置角色的基线矩阵与 action/resource 词表；
//! - [`acl`]：求值顺序的纯函数（deny > grant > 基线 > 默认拒绝）；
//! - [`version`]：`permission_versions` 的事务 helper（单调只增）。
//!
//! ## 查询侧与宿主侧职责的分界
//!
//! `authorize` 返回三值 [`Decision`]（Allowed / Denied / NotVisible），
//! **HTTP 映射在 host**（§7.1：`NotVisible` → 404、id 寻址的 `Denied` →
//! 404、组织级动作的 `Denied` → 403）——端口形状与状态码解耦。
//!
//! ## 管理用例与版本递增同事务（§6.3 的可执行形式）
//!
//! [`PolicyService::set_membership_enabled`] / [`grant_acl`] / [`deny_acl`]
//! / [`revoke_acl`] 的每一步都是**一个管理库事务**：实际授权变更（含禁用
//! 成员时的全部会话撤销）与 `permission_version` 递增要么都落要么都不落。
//! 「仅增加版本不构成撤权」（§6.3）反过来也由本层保证——不存在只改授权
//! 不递增版本的入口。
//!
//! ## 同步形态的裁决
//!
//! 与 [`crate::storage::known_hosts`]（同步端口适配器）同款：SQLite 查询
//! 在单连接互斥锁后是亚毫秒操作；L6 中间件在 async 上下文里直接调用，
//! `spawn_blocking` 包装属 handler 层的接线选择，不进端口语义。
//!
//! [`grant_acl`]: PolicyService::grant_acl
//! [`deny_acl`]: PolicyService::deny_acl
//! [`revoke_acl`]: PolicyService::revoke_acl

pub mod acl;
pub mod rbac;
pub mod version;

use std::sync::Arc;

use hmac::{Hmac, Mac};
use rusqlite::{params, OptionalExtension};
use sha2::Sha256;

use crate::storage::login_sessions::revoke_all_for_principal_tx;
use crate::storage::{AdminStore, StorageError};

pub use acl::Decision;
pub use rbac::{Action, ResourceType, Role};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

/// ACL 行的落库词汇（§6.1 `ck_acl_subject` CHECK 取值）。
///
/// 本刀只**求值** `User` 主体；`ServiceAccount` / `Delegated` 行可落库
/// （词表与 CHECK 已就位），求值随 §6.4 委托身份切片进入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectType {
    User,
    ServiceAccount,
    Delegated,
}

impl SubjectType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::ServiceAccount => "serviceAccount",
            Self::Delegated => "delegated",
        }
    }
}

/// 授权求值请求。`resource.organization_id` 与 `organization_id` 的比较
/// 就是 §6.1 第 1 步的组织边界。
#[derive(Debug, Clone)]
pub struct AuthorizeRequest {
    /// 请求主体的组织（来自会话行，**不是**请求体——INV-S1）。
    pub organization_id: String,
    pub principal_id: String,
    /// 目标资源的归属组织；跨组织 → [`Decision::NotVisible`]。
    pub resource: ResourceRef,
    pub action: Action,
}

/// 目标资源引用。`resource_id` 为空串表示**组织级**动作/条目（§6.1）。
#[derive(Debug, Clone)]
pub struct ResourceRef {
    pub organization_id: String,
    pub resource_type: ResourceType,
    pub resource_id: String,
}

impl ResourceRef {
    /// 组织级资源引用（资源 id 为空串）。
    pub fn organization_level(
        organization_id: impl Into<String>,
        resource_type: ResourceType,
    ) -> Self {
        Self {
            organization_id: organization_id.into(),
            resource_type,
            resource_id: String::new(),
        }
    }

    /// 是否组织级寻址（决定 `Denied` 的 403/404 映射，§7.1）。
    pub fn is_organization_level(&self) -> bool {
        self.resource_id.is_empty()
    }
}

/// 授权引擎。`server_secret` 由 config 注入（§6.2 的 HMAC 密钥；不落库、
/// 不进日志）。
pub struct PolicyService {
    store: Arc<AdminStore>,
    server_secret: Vec<u8>,
}

// Debug 手写而非 derive：store/server_secret 都不可见序列化（AdminStore 无
// Debug，secret 永不进任何输出——§6.2 注入纪律），只报形状。
impl std::fmt::Debug for PolicyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyService")
            .field("store", &"AdminStore")
            .field("server_secret", &"[redacted]")
            .finish()
    }
}

/// 授权管理的返回：变更后的版本与（禁用时）撤销的会话数——撤销传播的
/// 可观测面（§6.3 表格的宿主侧回执）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyChange {
    pub permission_version: i64,
    pub revoked_sessions: u64,
}

impl PolicyService {
    /// `server_secret` 为空即拒绝构造（fail-closed：空密钥的 HMAC 是
    /// 公开的常量，不是秘密）。
    pub fn new(store: Arc<AdminStore>, server_secret: Vec<u8>) -> Result<Self, StorageError> {
        if server_secret.is_empty() {
            return Err(StorageError::Query(
                "policy: server_secret must not be empty".into(),
            ));
        }
        Ok(Self {
            store,
            server_secret,
        })
    }

    fn backend(source: impl std::fmt::Display) -> StorageError {
        StorageError::Query(format!("policy: {source}"))
    }

    // ── 查询侧 ──

    /// §6.1 求值顺序的宿主入口。组织边界在 DB 之前判定——越组织资源
    /// 连一次查询都不发生（CM-05：不可见，且不泄漏存在性）。
    pub fn authorize(&self, request: &AuthorizeRequest) -> Result<Decision, StorageError> {
        if request.resource.organization_id != request.organization_id {
            return Ok(Decision::NotVisible);
        }
        if !request.resource.resource_type.acl_allowed() {
            // wapp 等 ACL 表不承载的类型：只走角色基线（§6.1/§15.3）。
            let role = self.read_role(&request.organization_id, &request.principal_id)?;
            return Ok(acl::evaluate(role, &to_acl_request(request), &[]));
        }

        let conn = self.store.lock_for_adapter()?;
        let role = read_role_conn(&conn, &request.organization_id, &request.principal_id)?;
        let rows = conn
            .prepare(
                "SELECT subject_type, subject_id, action, deny FROM acl_entries
                 WHERE organization_id = ?1 AND resource_type = ?2 AND resource_id = ?3",
            )
            .map_err(Self::backend)?
            .query_map(
                params![
                    request.organization_id,
                    request.resource.resource_type.as_str(),
                    request.resource.resource_id,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)? != 0,
                    ))
                },
            )
            .map_err(Self::backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::backend)?;
        // 词表解码分两步：闭包内不能 `?`，先把 action 字面量收拢，
        // 再统一把越词汇值折成精确的存储错误。
        let decoded = rows
            .into_iter()
            .map(|(subject_type, subject_id, action, deny)| {
                Action::from_db(&action)
                    .map(|action| (subject_type, subject_id, action, deny))
                    .ok_or_else(|| {
                        Self::backend(format!("acl_entries has an unknown action `{action}`"))
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::backend)?;
        let entries = decoded
            .into_iter()
            .map(|(subject_type, subject_id, action, deny)| acl::AclEntry {
                subject_type,
                subject_id,
                action,
                deny,
            })
            .collect::<Vec<_>>();
        Ok(acl::evaluate(role, &to_acl_request(request), &entries))
    }

    fn read_role(
        &self,
        organization_id: &str,
        principal_id: &str,
    ) -> Result<Option<Role>, StorageError> {
        let conn = self.store.lock_for_adapter()?;
        read_role_conn(&conn, organization_id, principal_id)
    }

    /// True only for the built-in organization administrator role. Used by
    /// organization-level mutations (for example T25 memory publish/revoke)
    /// that are not addressed by the connection/session resource table.
    /// A missing membership is "not admin", not an error.
    pub fn is_org_admin(
        &self,
        organization_id: &str,
        principal_id: &str,
    ) -> Result<bool, StorageError> {
        Ok(self.read_role(organization_id, principal_id)? == Some(Role::OrgAdmin))
    }

    /// 当前权限版本（无行 = 初始 1）。会话建立时的快照来源之一。
    pub fn permission_version(&self, organization_id: &str) -> Result<i64, StorageError> {
        let mut conn = self.store.lock_for_adapter()?;
        let tx = conn.transaction().map_err(Self::backend)?;
        let version = version::current_tx(&tx, organization_id)?;
        tx.commit().map_err(Self::backend)?;
        Ok(version)
    }

    // ── 管理侧（同事务授权变更 + 版本递增，§6.3） ──

    /// 建立 membership（登录映射与角色基线的落库处；外键目标）。
    pub fn create_membership(
        &self,
        organization_id: &str,
        principal_id: &str,
        role: Role,
        external_subject: &str,
    ) -> Result<(), StorageError> {
        if external_subject.trim().is_empty() {
            return Err(StorageError::Query(
                "policy: external_subject must not be blank (ck_membership_subject)".into(),
            ));
        }
        let now = now_string();
        let mut conn = self.store.lock_for_adapter()?;
        let tx = conn.transaction().map_err(Self::backend)?;
        tx.execute(
            "INSERT INTO memberships (organization_id, principal_id, role_id, external_subject,
                                      enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
            params![
                organization_id,
                principal_id,
                role.as_str(),
                external_subject,
                now
            ],
        )
        .map_err(Self::backend)?;
        tx.commit().map_err(Self::backend)?;
        Ok(())
    }

    /// 启用/禁用 membership 的管理用例（§4.5 + §6.3 同事务组合）：
    ///
    /// `UPDATE memberships` + （禁用时）撤销该主体全部存活会话（reason
    /// 由调用方给定：管理员禁用 → `Admin`；权限撤销传播 →
    /// `PermissionRevoked`）+ 递增 `permission_version`，全部在**同一个
    /// 事务**里提交。启用不撤销会话、但同样递增版本（角色状态变化使
    /// 缓存分区键失效，§6.2）。
    pub fn set_membership_enabled(
        &self,
        organization_id: &str,
        principal_id: &str,
        enabled: bool,
        on_disable_revoke_reason: crate::storage::login_sessions::RevokeReason,
    ) -> Result<PolicyChange, StorageError> {
        let now = now_string();
        let mut conn = self.store.lock_for_adapter()?;
        let tx = conn.transaction().map_err(Self::backend)?;
        let changed = tx
            .execute(
                "UPDATE memberships SET enabled = ?3, updated_at = ?4
                 WHERE organization_id = ?1 AND principal_id = ?2",
                params![
                    organization_id,
                    principal_id,
                    i64::from(enabled),
                    now.clone(),
                ],
            )
            .map_err(Self::backend)?;
        if changed == 0 {
            return Err(Self::backend(format!(
                "membership ({organization_id}, {principal_id}) does not exist"
            )));
        }
        let revoked_sessions = if enabled {
            0
        } else {
            revoke_all_for_principal_tx(
                &tx,
                organization_id,
                principal_id,
                on_disable_revoke_reason,
                &now,
            )
            .map_err(Self::backend)?
        };
        let permission_version = version::bump_tx(&tx, organization_id, &now)?;
        tx.commit().map_err(Self::backend)?;
        Ok(PolicyChange {
            permission_version,
            revoked_sessions,
        })
    }

    /// ACL grant（deny=0）+ 版本递增，同事务。
    pub fn grant_acl(
        &self,
        organization_id: &str,
        resource: &ResourceRef,
        subject: SubjectType,
        subject_id: &str,
        action: Action,
    ) -> Result<PolicyChange, StorageError> {
        self.write_acl_entry(
            organization_id,
            resource,
            subject,
            subject_id,
            action,
            false,
        )
    }

    /// ACL deny（deny=1）+ 版本递增，同事务（§6.1：显式 deny 优先于一切
    /// grant；写入 deny 是撤销传播的合法手段之一）。
    pub fn deny_acl(
        &self,
        organization_id: &str,
        resource: &ResourceRef,
        subject: SubjectType,
        subject_id: &str,
        action: Action,
    ) -> Result<PolicyChange, StorageError> {
        self.write_acl_entry(organization_id, resource, subject, subject_id, action, true)
    }

    /// 移除 ACL 条目（§6.3：撤销必须先移除 grant）+ 版本递增，同事务。
    /// 条目不存在时仍是成功（幂等移除），但版本照常递增——调用方拿到新
    /// 版本即可使旧分区键失效。
    pub fn revoke_acl(
        &self,
        organization_id: &str,
        resource: &ResourceRef,
        subject: SubjectType,
        subject_id: &str,
        action: Action,
    ) -> Result<PolicyChange, StorageError> {
        validate_resource_ref(resource)?;
        let now = now_string();
        let mut conn = self.store.lock_for_adapter()?;
        let tx = conn.transaction().map_err(Self::backend)?;
        tx.execute(
            "DELETE FROM acl_entries
             WHERE organization_id = ?1 AND resource_type = ?2 AND resource_id = ?3
               AND subject_type = ?4 AND subject_id = ?5 AND action = ?6",
            params![
                organization_id,
                resource.resource_type.as_str(),
                resource.resource_id,
                subject.as_str(),
                subject_id,
                action.as_str(),
            ],
        )
        .map_err(Self::backend)?;
        let permission_version = version::bump_tx(&tx, organization_id, &now)?;
        tx.commit().map_err(Self::backend)?;
        Ok(PolicyChange {
            permission_version,
            revoked_sessions: 0,
        })
    }

    fn write_acl_entry(
        &self,
        organization_id: &str,
        resource: &ResourceRef,
        subject: SubjectType,
        subject_id: &str,
        action: Action,
        deny: bool,
    ) -> Result<PolicyChange, StorageError> {
        validate_resource_ref(resource)?;
        if subject_id.is_empty() {
            return Err(StorageError::Query(
                "policy: subject_id must not be blank".into(),
            ));
        }
        let now = now_string();
        let mut conn = self.store.lock_for_adapter()?;
        let tx = conn.transaction().map_err(Self::backend)?;
        tx.execute(
            "INSERT INTO acl_entries (organization_id, resource_type, resource_id,
                                      subject_type, subject_id, action, deny, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (organization_id, resource_type, resource_id,
                          subject_type, subject_id, action)
             DO UPDATE SET deny = excluded.deny",
            params![
                organization_id,
                resource.resource_type.as_str(),
                resource.resource_id,
                subject.as_str(),
                subject_id,
                action.as_str(),
                i64::from(deny),
                now,
            ],
        )
        .map_err(Self::backend)?;
        let permission_version = version::bump_tx(&tx, organization_id, &now)?;
        tx.commit().map_err(Self::backend)?;
        Ok(PolicyChange {
            permission_version,
            revoked_sessions: 0,
        })
    }

    // ── policyIsolationKey（§6.2） ──

    /// `HMAC(server_secret, organizationId | principalId | policyRef_revision
    /// | permissionVersion | readOnlyRestriction)`，base64url（无填充）。
    ///
    /// 用途是**隔离**而非授权（§6.2）：缓存与连接池 `PoolKey` 的分区键、
    /// SSE 授权快照的比对基准。客户端读到它只能原样回传做一致性检查，
    /// 服务端收到后一律重新求值。
    ///
    /// 分隔符 `|` 与 §6.2 公式逐字一致；组件歧义由调用方保证 ID 词表不含
    /// `|`（ID 词表校验随域切片落地，本函数的输入契约在此登记）。
    pub fn policy_isolation_key(
        &self,
        organization_id: &str,
        principal_id: &str,
        policy_ref_revision: u64,
        permission_version: i64,
        read_only: bool,
    ) -> Result<String, StorageError> {
        let message = format!(
            "{organization_id}|{principal_id}|{policy_ref_revision}|{permission_version}|{}",
            u8::from(read_only),
        );
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.server_secret)
            .map_err(|_| Self::backend("server_secret is not usable as an HMAC key"))?;
        mac.update(message.as_bytes());
        let digest = mac.finalize().into_bytes();
        Ok(URL_SAFE_NO_PAD.encode(digest))
    }
}

fn to_acl_request(request: &AuthorizeRequest) -> acl::Request<'_> {
    acl::Request {
        principal_id: &request.principal_id,
        resource_type: request.resource.resource_type,
        resource_id: &request.resource.resource_id,
        action: request.action,
    }
}

/// 模块级错误包装（自由函数用；`Self::backend` 只在 impl 内可用）。
fn backend(source: impl std::fmt::Display) -> StorageError {
    StorageError::Query(format!("policy: {source}"))
}

fn read_role_conn(
    conn: &rusqlite::Connection,
    organization_id: &str,
    principal_id: &str,
) -> Result<Option<Role>, StorageError> {
    let (role_raw, enabled): (Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT role_id, enabled FROM memberships
             WHERE organization_id = ?1 AND principal_id = ?2",
            params![organization_id, principal_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(backend)?
        .unwrap_or((None, None));
    let Some(role_raw) = role_raw else {
        return Ok(None);
    };
    // 禁用的 membership 一律不可见（§4.5：管理员禁用即撤权；会话撤销是
    // 同事务的组合动作，这里是不变量兜底）。
    if enabled.unwrap_or(0) == 0 {
        return Ok(None);
    }
    Role::from_db(&role_raw)
        .map(Some)
        .ok_or_else(|| backend(format!("memberships has an unknown role `{role_raw}`")))
}

fn validate_resource_ref(resource: &ResourceRef) -> Result<(), StorageError> {
    if !resource.resource_type.acl_allowed() {
        return Err(StorageError::Query(format!(
            "policy: acl_entries does not carry resource type `{}` (ck_acl_resource)",
            resource.resource_type.as_str()
        )));
    }
    Ok(())
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::login_sessions::{
        LoginSessionMode, LoginSessions, NewLoginSession, RevokeReason, SessionRejection,
    };
    use crate::storage::ValidateFailure;
    use datazen::shared::id::Timestamp;

    fn store() -> Arc<AdminStore> {
        let store = AdminStore::open_in_memory().expect("store");
        store.migrate().expect("migrate");
        Arc::new(store)
    }

    fn policy(store: Arc<AdminStore>) -> PolicyService {
        PolicyService::new(store, b"test-server-secret-32-bytes-minimum!!".to_vec())
            .expect("policy")
    }

    const NOW: &str = "2026-01-01T00:00:00.000Z";
    const ABSOLUTE: &str = "2026-01-02T00:00:00.000Z";
    const IDLE: &str = "2026-01-01T08:00:00.000Z";

    fn session_input(org: &str, principal: &str, session_id: &str) -> NewLoginSession {
        NewLoginSession {
            organization_id: org.into(),
            session_id: session_id.into(),
            principal_id: principal.into(),
            auth_time: NOW.into(),
            issued_at: NOW.into(),
            absolute_expires_at: ABSOLUTE.into(),
            idle_expires_at: IDLE.into(),
            permission_version: 1,
            client_binding: "ua|client-1".into(),
            mode: LoginSessionMode::Cookie,
            native_secret_hash: None,
        }
    }

    fn connection_resource(org: &str, id: &str) -> ResourceRef {
        ResourceRef {
            organization_id: org.into(),
            resource_type: ResourceType::Connection,
            resource_id: id.into(),
        }
    }

    fn authorize_request(
        org: &str,
        principal: &str,
        resource: &ResourceRef,
        action: Action,
    ) -> AuthorizeRequest {
        AuthorizeRequest {
            organization_id: org.into(),
            principal_id: principal.into(),
            resource: resource.clone(),
            action,
        }
    }

    #[test]
    fn cross_organization_resources_are_not_visible_before_any_query() {
        let store = store();
        let service = policy(store.clone());
        service
            .create_membership("org-a", "user-1", Role::OrgAdmin, "idp:u1")
            .expect("membership");

        // orgAdmin 对 org-b 的资源：组织边界先于一切 → NotVisible（404）。
        let decision = service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &connection_resource("org-b", "c-1"),
                Action::Read,
            ))
            .expect("authorize");
        assert_eq!(decision, Decision::NotVisible);
    }

    #[test]
    fn missing_or_disabled_membership_is_not_visible() {
        let store = store();
        let service = policy(store.clone());
        service
            .create_membership("org-a", "user-1", Role::OrgMember, "idp:u1")
            .expect("membership");

        service
            .set_membership_enabled("org-a", "user-1", false, RevokeReason::Admin)
            .expect("disable");

        let decision = service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &connection_resource("org-a", "c-1"),
                Action::Read,
            ))
            .expect("authorize");
        assert_eq!(
            decision,
            Decision::NotVisible,
            "禁用 membership 的主体对任何资源都不可见（不变量兜底）"
        );
    }

    #[test]
    fn acl_override_and_deny_precedence_hold_end_to_end() {
        let store = store();
        let service = policy(store.clone());
        service
            .create_membership("org-a", "member", Role::OrgMember, "idp:m")
            .expect("membership");
        let resource = connection_resource("org-a", "c-1");

        // 基线拒绝（orgMember 不可写）。
        assert_eq!(
            service
                .authorize(&authorize_request(
                    "org-a",
                    "member",
                    &resource,
                    Action::Write
                ))
                .expect("authorize"),
            Decision::Denied
        );
        // grant 越过基线缺口；版本随授权变更递增。
        let before = service.permission_version("org-a").expect("version");
        let change = service
            .grant_acl(
                "org-a",
                &resource,
                SubjectType::User,
                "member",
                Action::Write,
            )
            .expect("grant");
        assert_eq!(change.permission_version, before + 1);
        assert_eq!(
            service
                .authorize(&authorize_request(
                    "org-a",
                    "member",
                    &resource,
                    Action::Write
                ))
                .expect("authorize"),
            Decision::Allowed
        );
        // deny 压过 grant（§6.1 第 2 步）。
        service
            .deny_acl(
                "org-a",
                &resource,
                SubjectType::User,
                "member",
                Action::Write,
            )
            .expect("deny");
        assert_eq!(
            service
                .authorize(&authorize_request(
                    "org-a",
                    "member",
                    &resource,
                    Action::Write
                ))
                .expect("authorize"),
            Decision::Denied
        );
        // revoke 回到基线拒绝（先移除 grant 是撤销传播的合法路径）。
        service
            .revoke_acl(
                "org-a",
                &resource,
                SubjectType::User,
                "member",
                Action::Write,
            )
            .expect("revoke");
        assert_eq!(
            service
                .authorize(&authorize_request(
                    "org-a",
                    "member",
                    &resource,
                    Action::Write
                ))
                .expect("authorize"),
            Decision::Denied
        );
    }

    #[test]
    fn organization_level_action_denied_maps_for_the_403_branch() {
        let store = store();
        let service = policy(store.clone());
        service
            .create_membership("org-a", "member", Role::OrgMember, "idp:m")
            .expect("membership");
        service
            .create_membership("org-a", "admin", Role::OrgAdmin, "idp:a")
            .expect("membership");

        let org_level = ResourceRef::organization_level("org-a", ResourceType::Connection);
        // 组织级动作：orgAdmin 允许（403 分支的 Allowed 侧）。
        assert_eq!(
            service
                .authorize(&authorize_request(
                    "org-a",
                    "admin",
                    &org_level,
                    Action::Admin
                ))
                .expect("authorize"),
            Decision::Allowed
        );
        // 组织级动作：orgMember 拒绝 → host 映射 403（资源 id 为空串）。
        assert_eq!(
            service
                .authorize(&authorize_request(
                    "org-a",
                    "member",
                    &org_level,
                    Action::Admin
                ))
                .expect("authorize"),
            Decision::Denied
        );
        assert!(org_level.is_organization_level());
    }

    /// §6.3 的可执行形式：禁用 membership 与撤销全部会话、递增版本
    /// 同事务——中途失败不留下半套变更。
    #[test]
    fn membership_disable_revokes_sessions_and_bumps_the_version_in_one_transaction() {
        let store = store();
        let service = policy(store.clone());
        service
            .create_membership("org-a", "user-1", Role::OrgMember, "idp:u1")
            .expect("membership");
        let sessions = LoginSessions::new(store.clone());
        sessions
            .create(session_input("org-a", "user-1", "s-1"))
            .expect("session 1");
        sessions
            .create(session_input("org-a", "user-1", "s-2"))
            .expect("session 2");

        let before = service.permission_version("org-a").expect("version");
        let change = service
            .set_membership_enabled("org-a", "user-1", false, RevokeReason::PermissionRevoked)
            .expect("disable");
        assert_eq!(change.permission_version, before + 1);
        assert_eq!(change.revoked_sessions, 2);

        // 撤销立即可见：下一个入站请求必 401（§4.5 的撤销语义保证）。
        assert_eq!(
            sessions.validate("s-1", Timestamp::new(NOW), 3600),
            Err(ValidateFailure::Rejected(SessionRejection::Revoked(
                RevokeReason::PermissionRevoked
            ))),
        );
        assert!(matches!(
            sessions.validate("s-2", Timestamp::new(NOW), 3600),
            Err(ValidateFailure::Rejected(_))
        ));

        // 权限版本快照语义：旧会话行上的快照（1）已落后于当前版本（2），
        // 排队执行派发前比对即拒绝（§6.3 的 CM-39 路径，比对在派发层）。
        assert_eq!(
            service.permission_version("org-a").expect("version"),
            before + 1
        );
    }

    /// §6.2：隔离键对每个输入分量都敏感、对同输入确定。
    #[test]
    fn policy_isolation_key_is_deterministic_and_input_sensitive() {
        let store = store();
        let service = policy(store);

        let base = service
            .policy_isolation_key("org-a", "user-1", 3, 7, false)
            .expect("key");
        assert_eq!(
            base,
            service
                .policy_isolation_key("org-a", "user-1", 3, 7, false)
                .expect("key"),
            "same inputs → same partition key"
        );
        for variant in [
            service
                .policy_isolation_key("org-b", "user-1", 3, 7, false)
                .expect("key"),
            service
                .policy_isolation_key("org-a", "user-2", 3, 7, false)
                .expect("key"),
            service
                .policy_isolation_key("org-a", "user-1", 4, 7, false)
                .expect("key"),
            service
                .policy_isolation_key("org-a", "user-1", 3, 8, false)
                .expect("key"),
            service
                .policy_isolation_key("org-a", "user-1", 3, 7, true)
                .expect("key"),
        ] {
            assert_ne!(base, variant, "every input component must change the key");
        }
        // base64url 字母表（无 +/、无填充）。
        assert!(
            base.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "key must be base64url: {base}"
        );
        assert_eq!(
            base.len(),
            43,
            "HMAC-SHA256 → 32 bytes → 43 base64url chars"
        );
    }

    #[test]
    fn an_empty_server_secret_is_rejected_at_construction() {
        let error = PolicyService::new(store(), Vec::new()).expect_err("empty secret");
        assert!(error
            .to_string()
            .contains("server_secret must not be empty"));
    }
}
