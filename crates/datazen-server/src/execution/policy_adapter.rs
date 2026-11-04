//! WR-F7：modules [`PolicyService`](datazen::execution::application::policy::PolicyService)
//! 的服务端适配器 —— 真租约源（`ServerRuntimeLeaseSource`）的三件套之一。
//!
//! 职责分界（§6 的「查询侧与宿主侧」）：
//!
//! - **求值**委托给 [`crate::policy::PolicyService`]（rbac 基线 + acl 表 +
//!   组织边界先于 DB，CM-05），适配器只做词汇映射与 port 错误翻译；
//! - **委托**落在 v7 `execution_service_identity` 表（与
//!   [`crate::execution::identity::ServerIdentityResolver`] 同表同判据：
//!   行存在 / 未吊销 / 未过期 / scope 逐字相等 / 目标连接以行内事实为准，
//!   CM-06——委托主体不得凭委托继承调用者的全量权限）；
//! - **隔离键**沿用引擎的 `policyIsolationKey` 派生（HMAC，§6.2）；
//!   `policy_ref_revision=0` 与 `read_only=false` 是与
//!   `identity.rs` 同款的保守缺省（同一登记：随执行上下文事实源收口改取真值）；
//! - **版本订阅**用内存通道广播：管理面（ACL 管理端点）点亮后经
//!   [`ServerExecutionPolicy::notify_version_change`] 推送；订阅阻塞等推送
//!   （不轮询——端口契约硬约束），基线版本由订阅方按自身组织的
//!   `permission_version` 现查。
//!
//! ## 动作词表映射（登记）
//!
//! modules [`AuthorizationAction`] 是用例词汇，server 引擎是
//! (action, resource) 词汇。组织级映射如下（每行在测试中钉住）：
//!
//! | modules 动作 | server (Action, ResourceType) | 依据 |
//! | --- | --- | --- |
//! | Connect | (Execute, Connection) | 建立连接即执行资源 |
//! | Query | (Execute, Execution) | 查询即一次执行 |
//! | Write | (Write, Execution) | 上下文写入等执行面写 |
//! | Administer | (Admin, Connection) | 组织管理动作 |
//! | ViewCredential | (Read, Connection) | v1 与连接读同闸（凭据读的独立档位随连接管理 §4.1 切片） |
//! | ExportArtifact | (Download, Artifact) | 导出即产物下载 |
//! | ManageJob | (Cancel, Job) | v1 取「操作者」档位（读/取消）；写级管理走逐资源 ACL |
//!
//! 委托 scope → 动作集（v1 词表，前缀匹配）：
//! `workflow` → {Connect, Query, ManageJob}；`connection` → {Connect, Query}；
//! `query` → {Query}；`job` → {ManageJob}；未知前缀 → 空集（一切拒绝）。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::OptionalExtension;

use datazen::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
    PolicyChangeStream, PolicyService as ModulesPolicyService, PolicyVersionChange,
};
use datazen::shared::clock::Clock;
use datazen::shared::context::{DelegationRef, RequestContext};
use datazen::shared::error::PortError;
use datazen::shared::id::PolicyIsolationKey;

use crate::policy::{AuthorizeRequest, Decision, ResourceRef};
use crate::storage::AdminStore;

type EnginePolicy = crate::policy::PolicyService;

/// `execution_service_identity` 行（与 `identity.rs` 的私有行同形状；此处
/// 独立声明以保持两个适配器互不依赖）。
struct DelegationRow {
    scope: String,
    target_connection_id: String,
    expires_at: datazen::shared::id::Timestamp,
    revoked: bool,
}

fn load_delegation_row(
    store: &AdminStore,
    organization_id: &str,
    delegation_id: &str,
) -> Result<Option<DelegationRow>, PortError> {
    let conn = store
        .lock_for_adapter()
        .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
    conn.query_row(
        "SELECT scope, target_connection_id, expires_at, revoked
         FROM execution_service_identity
         WHERE organization_id = ?1 AND delegation_id = ?2",
        rusqlite::params![organization_id, delegation_id],
        |row| {
            Ok(DelegationRow {
                scope: row.get(0)?,
                target_connection_id: row.get(1)?,
                expires_at: datazen::shared::id::Timestamp::new(row.get::<_, String>(2)?),
                revoked: row.get::<_, i64>(3)? != 0,
            })
        },
    )
    .optional()
    .map_err(|error| PortError::BackendUnavailable(error.to_string()))
}

/// 委托 scope 前缀 → 授予动作集（v1 词表，见模块头登记）。
fn scope_actions(scope: &str) -> Vec<AuthorizationAction> {
    let prefix = scope.split(':').next().unwrap_or(scope);
    match prefix {
        "workflow" => vec![
            AuthorizationAction::Connect,
            AuthorizationAction::Query,
            AuthorizationAction::ManageJob,
        ],
        "connection" => vec![AuthorizationAction::Connect, AuthorizationAction::Query],
        "query" => vec![AuthorizationAction::Query],
        "job" => vec![AuthorizationAction::ManageJob],
        _ => Vec::new(),
    }
}

/// modules 动作 → server 组织级 (Action, ResourceType) 映射（模块头登记表）。
fn action_resource(
    action: &AuthorizationAction,
) -> (
    crate::policy::rbac::Action,
    crate::policy::rbac::ResourceType,
) {
    use crate::policy::rbac::{Action as A, ResourceType as R};
    match action {
        AuthorizationAction::Connect => (A::Execute, R::Connection),
        AuthorizationAction::Query => (A::Execute, R::Execution),
        AuthorizationAction::Write => (A::Write, R::Execution),
        AuthorizationAction::Administer => (A::Admin, R::Connection),
        AuthorizationAction::ViewCredential => (A::Read, R::Connection),
        AuthorizationAction::ExportArtifact => (A::Download, R::Artifact),
        AuthorizationAction::ManageJob => (A::Cancel, R::Job),
    }
}

/// 服务端执行策略适配器。构造即持有引擎与管理库；订阅面见模块头。
pub struct ServerExecutionPolicy {
    engine: Arc<EnginePolicy>,
    store: Arc<AdminStore>,
    clock: Arc<dyn Clock>,
    subscribers: Mutex<Vec<std::sync::mpsc::Sender<PolicyVersionChange>>>,
}

impl std::fmt::Debug for ServerExecutionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerExecutionPolicy")
            .field("engine", &"PolicyService")
            .field("store", &"AdminStore")
            .finish()
    }
}

impl ServerExecutionPolicy {
    /// 构造。`engine` 与管理库同源（bootstrap 装配）；时钟用于委托有效期。
    pub fn new(engine: Arc<EnginePolicy>, store: Arc<AdminStore>, clock: Arc<dyn Clock>) -> Self {
        Self {
            engine,
            store,
            clock,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// 管理面推送入口：ACL/成员管理端点点亮后，授权变更成功即调用。
    /// 死接收者就地剔除（不阻塞管理路径）。
    pub fn notify_version_change(&self, version: i64, isolation_key: PolicyIsolationKey) {
        let change = PolicyVersionChange {
            version: datazen::shared::id::Counter::new(version as u64),
            isolation_key,
        };
        let mut subscribers = match self.subscribers.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        subscribers.retain(|sender| sender.send(change.clone()).is_ok());
    }

    /// 当前组织权限版本（隔离键与订阅快照共用）。
    fn current_version(&self, organization_id: &str) -> Result<i64, PortError> {
        self.engine
            .permission_version(organization_id)
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))
    }

    /// 委托行的存活校验（行存在 / 未吊销 / 未过期 / scope 逐字相等）。
    /// 通过返回 `Some(行)`；不通过返回原因。行缺失 → `None`。
    fn live_row(
        &self,
        organization_id: &str,
        delegation_id: &str,
        scope: &str,
    ) -> Result<Result<DelegationRow, &'static str>, PortError> {
        let Some(row) = load_delegation_row(&self.store, organization_id, delegation_id)? else {
            return Ok(Err("delegationUnknown"));
        };
        if row.revoked {
            return Ok(Err("delegationRevoked"));
        }
        if row.expires_at <= self.clock.now() {
            return Ok(Err("delegationExpired"));
        }
        if row.scope != scope {
            return Ok(Err("delegationScope"));
        }
        Ok(Ok(row))
    }
}

#[async_trait]
impl ModulesPolicyService for ServerExecutionPolicy {
    async fn authorize(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        let decision: Decision = match subject {
            AuthorizationSubject::Principal { principal_id } => {
                let (engine_action, resource_type) = action_resource(&action);
                let request = AuthorizeRequest {
                    organization_id: ctx.organization_id.as_str().to_owned(),
                    principal_id: principal_id.as_str().to_owned(),
                    resource: ResourceRef::organization_level(
                        ctx.organization_id.as_str().to_owned(),
                        resource_type,
                    ),
                    action: engine_action,
                };
                self.engine
                    .authorize(&request)
                    .map_err(|error| PortError::BackendUnavailable(error.to_string()))?
            }
            AuthorizationSubject::Delegated(delegation) => {
                // CM-06：委托校验失败不得降级为调用者权限——这里直接按
                // 委托行事实裁决，不走引擎（引擎词表没有服务主体委托）。
                let row = match self.live_row(
                    delegation.organization_id.as_str(),
                    delegation.delegation_id.as_str(),
                    &delegation.scope,
                )? {
                    Ok(row) => row,
                    Err(reason_code) => {
                        return Ok(AuthorizationDecision::Deny { reason_code });
                    }
                };
                if scope_actions(&row.scope).contains(&action) {
                    Decision::Allowed
                } else {
                    Decision::Denied
                }
            }
        };
        Ok(match decision {
            Decision::Allowed => AuthorizationDecision::Allow,
            Decision::Denied => AuthorizationDecision::Deny {
                reason_code: "policyDenied",
            },
            Decision::NotVisible => AuthorizationDecision::Deny {
                reason_code: "notVisible",
            },
        })
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        delegation: DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        let row = self
            .live_row(
                delegation.organization_id.as_str(),
                delegation.delegation_id.as_str(),
                &delegation.scope,
            )?
            .map_err(|reason| {
                // 与 identity.rs 同口径：行缺失 → NotFound（防枚举的合并由
                // host 决定）；吊销/过期/scope 不符 → TokenInvalid。
                if reason == "delegationUnknown" {
                    PortError::NotFound("service identity".into())
                } else {
                    PortError::TokenInvalid
                }
            })?;
        Ok(DelegationGrant {
            delegation,
            actions: scope_actions(&row.scope),
        })
    }

    async fn resolve_delegation(&self, ctx: &RequestContext) -> Result<DelegationGrant, PortError> {
        let delegation_id = ctx
            .delegation_id
            .as_ref()
            .ok_or_else(|| PortError::TokenInvalid)?;
        let row = load_delegation_row(
            &self.store,
            ctx.organization_id.as_str(),
            delegation_id.as_str(),
        )?
        .ok_or_else(|| PortError::NotFound("service identity".into()))?;
        if row.revoked || row.expires_at <= self.clock.now() {
            return Err(PortError::TokenInvalid);
        }
        let grant = DelegationRef {
            delegation_id: delegation_id.clone(),
            organization_id: ctx.organization_id.clone(),
            principal_id: ctx.principal_id.clone(),
            client_instance_id: ctx.client_instance_id.clone(),
            scope: row.scope.clone(),
            expires_at: row.expires_at.clone(),
        };
        Ok(DelegationGrant {
            delegation: grant,
            actions: scope_actions(&row.scope),
        })
    }

    async fn authorize_target(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        action: AuthorizationAction,
        target: &datazen::execution::domain::ExecutionTarget,
        _command: &str,
        _input: &serde_json::Value,
    ) -> Result<AuthorizationDecision, PortError> {
        if let AuthorizationSubject::Delegated(delegation) = &subject {
            // CM-06 目标绑定：委托行事实里的 target_connection_id 是唯一
            // 合法目标——请求目标与行不符即拒（不查引擎）。
            let row = match self.live_row(
                delegation.organization_id.as_str(),
                delegation.delegation_id.as_str(),
                &delegation.scope,
            )? {
                Ok(row) => row,
                Err(reason_code) => {
                    return Ok(AuthorizationDecision::Deny { reason_code });
                }
            };
            if row.target_connection_id != target.connection_id.as_str() {
                return Ok(AuthorizationDecision::Deny {
                    reason_code: "delegationTarget",
                });
            }
            let granted = scope_actions(&row.scope);
            return Ok(if granted.contains(&action) {
                AuthorizationDecision::Allow
            } else {
                AuthorizationDecision::Deny {
                    reason_code: "delegationScope",
                }
            });
        }
        // 非委托：目标级求值——资源从组织级收窄到具体连接（逐连接 ACL 生效）。
        let AuthorizationSubject::Principal { principal_id } = &subject else {
            return Ok(AuthorizationDecision::Deny {
                reason_code: "policyDenied",
            });
        };
        let (engine_action, resource_type) = action_resource(&action);
        let request = AuthorizeRequest {
            organization_id: ctx.organization_id.as_str().to_owned(),
            principal_id: principal_id.as_str().to_owned(),
            resource: ResourceRef {
                organization_id: ctx.organization_id.as_str().to_owned(),
                resource_type,
                resource_id: target.connection_id.as_str().to_owned(),
            },
            action: engine_action,
        };
        let decision = self
            .engine
            .authorize(&request)
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
        Ok(match decision {
            Decision::Allowed => AuthorizationDecision::Allow,
            Decision::Denied => AuthorizationDecision::Deny {
                reason_code: "policyDenied",
            },
            Decision::NotVisible => AuthorizationDecision::Deny {
                reason_code: "notVisible",
            },
        })
    }

    fn isolation_key(&self, ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        let permission_version = self.current_version(ctx.organization_id.as_str())?;
        let key = self
            .engine
            .policy_isolation_key(
                ctx.organization_id.as_str(),
                ctx.principal_id.as_str(),
                0,
                permission_version,
                false,
            )
            .map_err(|error| PortError::BackendUnavailable(error.to_string()))?;
        Ok(PolicyIsolationKey::new(key))
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        // 端口签名不带组织——订阅是全实例广播；首项即下一次管理面变更
        // （基线版本由订阅方按自身组织的 permission_version 现查）。
        // 阻塞等推送，不轮询（端口契约硬约束）。
        let (sender, receiver) = std::sync::mpsc::channel::<PolicyVersionChange>();
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.push(sender);
        }
        Box::new(receiver.into_iter())
    }
}
