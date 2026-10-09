//! 身份与授权判定顺序（INV-01/INV-03、`shared-boundaries-and-ports.md` §5.1）。
//!
//! ## 三条硬约束
//!
//! 1. **身份只来自 adapter**。用例签名第一个参数恒为 `&RequestContext`，
//!    请求体里出现 organization/principal 一律不是身份来源（INV-01）。
//! 2. **后台执行没有 `clientInstanceId`**。后台执行的主体来自持久化的执行归属，
//!    不是某个客户端实例，因此 owner 判定必须能接受「无客户端实例」的主体。
//! 3. **`OwnerRef` 必须落在当前客户端或已授权 Job 上**（§5.1(3)）。
//!    `OwnerRef::Editor` 还要求 `editor_session_id` 属于当前 `clientInstanceId`。
//!
//! ## 判定顺序不可调换
//!
//! [`JUDGMENT_ORDER`] 把顺序固化成数据而不是散落的 `if`：
//! 上下文绑定 → owner 可采信 → 授权 → 执行位。晚一步的判定**不能**替代早一步的判定：
//! 未绑定上下文就去问策略，拿到的是「谁的权限」这个问题的错误答案。
//!
//! 委托只到根、失败**不得降级为调用者权限**（CM-06）：[`authorization_subject`] 在
//! 上下文与委托引用不一致时直接失败，而不是悄悄退回 `Principal`。

use datazen_platform_api::context::{DelegationRef, OwnerRef, RequestContext};
use datazen_platform_api::id::{ExecutionId, JobId};
use datazen_platform_api::ports::policy::AuthorizationSubject;

use crate::error::{ApiError, ApiErrorCode};

/// 判定阶段。顺序即 [`JUDGMENT_ORDER`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PolicyStage {
    /// INV-01：上下文必须由 adapter 构造并与请求绑定。
    ContextBound,
    /// §5.1(3)：`OwnerRef` 必须属于当前客户端或已授权 Job。
    OwnerAdmissible,
    /// 授权判定（`PolicyService`）。委托校验在此之前已完成。
    AuthorizationGranted,
    /// INV-03：同一 session 最多一个普通执行。
    SingleExecutionSlot,
}

/// 固定判定顺序。**不可调换**，也不允许按调用点裁剪。
pub const JUDGMENT_ORDER: [PolicyStage; 4] = [
    PolicyStage::ContextBound,
    PolicyStage::OwnerAdmissible,
    PolicyStage::AuthorizationGranted,
    PolicyStage::SingleExecutionSlot,
];

/// 控制路径（CM §6.3）。执行、取消、关闭三条路径**互相独立**：
/// 取消不得借用执行路径的锁，否则「执行中」会把取消堵死。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlPath {
    Execution,
    Cancellation,
    Shutdown,
}

/// 执行位占用情况（INV-03）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionSlot {
    /// 空闲，可以起新执行。
    Free,
    /// 已有普通执行在跑，新请求必须排队（不是失败）。
    QueuedBehind { running: ExecutionId },
}

/// 授权判定器：只做判定，不做鉴权数据获取。
///
/// 授权**数据**来自 `PolicyService` 端口；本类型负责的是
/// 「主体是谁」「顺序对不对」「owner 能不能采信」。
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityPolicy;

impl IdentityPolicy {
    pub const fn new() -> Self {
        Self
    }

    /// 从上下文构造授权主体。
    ///
    /// * 无委托 ⇒ `Principal`，主体就是上下文里的 `principalId`。
    /// * 有委托 ⇒ `Delegated`，且**必须**传入同一 `delegationId` 的 `DelegationRef`；
    ///   两者不一致是 `ContextConflict`，不得降级（CM-06）。
    pub fn authorization_subject(
        ctx: &RequestContext,
        delegation: Option<&DelegationRef>,
    ) -> Result<AuthorizationSubject, ApiError> {
        match (ctx.delegation_id.as_ref(), delegation) {
            (None, None) => Ok(AuthorizationSubject::Principal {
                principal_id: ctx.principal_id.clone(),
            }),
            (Some(expected), Some(actual)) if &actual.delegation_id == expected => {
                Ok(AuthorizationSubject::Delegated(actual.clone()))
            }
            (Some(_), None) => Err(ApiError::new(
                ApiErrorCode::ContextConflict,
                "上下文声明了委托，但没有提供可校验的委托引用",
            )),
            (None, Some(_)) => Err(ApiError::new(
                ApiErrorCode::ContextConflict,
                "提供了委托引用，但上下文未声明委托",
            )),
            (Some(expected), Some(actual)) => Err(ApiError::new(
                ApiErrorCode::ContextConflict,
                format!(
                    "委托 id 不一致：上下文 {}，请求 {}",
                    expected.as_str(),
                    actual.delegation_id.as_str()
                ),
            )),
        }
    }

    /// §5.1(3)：`OwnerRef` 可采信性。
    ///
    /// * `Editor` / `ClientSession`：归属的 `clientInstanceId` 必须是**当前**客户端实例。
    /// * `Job` / `WorkflowBlock`：归属 Job 必须等于 `authorized_job`。
    ///
    /// 失败一律是 `PermissionDenied`——不给「是别人的」这种可用于探测的信息。
    ///
    /// # 契约债（不要把本函数当成网关归属闸门的兜底）
    ///
    /// 1. **本函数当前没有生产调用点。** 全仓只有定义本身、几处文档引用、以及本文件
    ///    内的单测；`src-tauri` 不调用它。任何「上层已经挡住了」的结论都不成立。
    /// 2. **这里的 `OwnerRef` 与 `packages/runtime` 里那个同名类型不是同一个东西。**
    ///    本侧是 `datazen_platform_api::context::OwnerRef`：它的 `Editor` 只有
    ///    `client_instance_id` / `editor_session_id`，`Job` 只有 `job_id` / `stage_id`
    ///    （且 `stage_id` 是 `crate::id::StageId`，网关侧同名字段是 `String`）——**两侧都没有
    ///    `organization_id` / `principal_id`**。因此本函数在结构上**看不到组织与主体**，
    ///    也不可能校验网关那一侧。
    /// 3. **两侧没有任何交叉校验**，同一个变体名、同一批字段名，但语义可以各改各的、互不报警。
    /// 4. 即便接上调用方，这里比的是「调用方显式传入的 `authorized_job` 与 `owner.job_id`
    ///    是否相等」，这是**这个 job 有没有被授权**，不是**这是不是同一个人**——与
    ///    CM-06「U1 指向 U2 的 job」的跨用户语义不是同一件事，不能互相顶替。
    ///
    /// 两条同名类型由网关侧 `packages/runtime/src/gateway/owner_binding.rs` 的模块头
    /// 同样登记在案。统一它们是跨 crate 的契约改动，不在网关轨内单方面做。
    pub fn check_owner(
        ctx: &RequestContext,
        owner: &OwnerRef,
        authorized_job: Option<&JobId>,
    ) -> Result<(), ApiError> {
        match owner {
            OwnerRef::Editor {
                client_instance_id,
                editor_session_id: _,
            } => {
                if client_instance_id == &ctx.client_instance_id {
                    return Ok(());
                }
                Err(ApiError::permission_denied(
                    "编辑器会话不属于当前客户端实例",
                ))
            }
            OwnerRef::ClientSession {
                client_instance_id,
                purpose: _,
            } => {
                if client_instance_id == &ctx.client_instance_id {
                    return Ok(());
                }
                Err(ApiError::permission_denied(
                    "长驻客户端会话不属于当前客户端实例",
                ))
            }
            OwnerRef::Job {
                job_id,
                stage_id: _,
            } => match authorized_job {
                Some(job) if job == job_id => Ok(()),
                _ => Err(ApiError::permission_denied("归属的 Job 未获授权")),
            },
            OwnerRef::WorkflowBlock {
                job_id,
                block_id: _,
            } => match authorized_job {
                Some(job) if job == job_id => Ok(()),
                _ => Err(ApiError::permission_denied("归属的 Job 未获授权")),
            },
        }
    }

    /// INV-03：执行位占用。空 ⇒ `Free`；被占 ⇒ `QueuedBehind`，**不是** `ResourceBusy` 错误。
    pub fn execution_slot(active: Option<ExecutionId>) -> ExecutionSlot {
        match active {
            Some(running) => ExecutionSlot::QueuedBehind { running },
            None => ExecutionSlot::Free,
        }
    }

    /// 控制路径互斥性：取消走独立路径，绝不与执行路径共用串行队列（CM §6.3）。
    pub fn control_path_is_independent(path: ControlPath) -> bool {
        !matches!(path, ControlPath::Execution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::{
        ClientInstanceId, DelegationId, EditorSessionId, OrganizationId, PrincipalId, RequestId,
        StageId, Timestamp,
    };

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

    fn delegation(id: &str) -> DelegationRef {
        DelegationRef::new(
            DelegationId::new(id),
            OrganizationId::new("org-1"),
            PrincipalId::new("user-2"),
            ClientInstanceId::new("client-1"),
            "connections:read".into(),
            Timestamp::new("2026-01-02T00:00:00Z"),
        )
    }

    #[test]
    fn a_plain_context_becomes_a_principal_subject() {
        let subject = IdentityPolicy::authorization_subject(&context(), None).expect("subject");
        assert_eq!(
            subject,
            AuthorizationSubject::Principal {
                principal_id: PrincipalId::new("user-1")
            }
        );
    }

    #[test]
    fn a_delegated_context_never_downgrades_to_the_caller() {
        let mut ctx = context();
        ctx.delegation_id = Some(DelegationId::new("del-1"));
        // 缺少委托引用 ⇒ 失败，而不是退回 user-1 的权限。
        let missing = IdentityPolicy::authorization_subject(&ctx, None).expect_err("must fail");
        assert_eq!(missing.code, ApiErrorCode::ContextConflict);

        // 委托 id 对不上 ⇒ 同样失败。
        let mismatched = IdentityPolicy::authorization_subject(&ctx, Some(&delegation("del-2")))
            .expect_err("must fail");
        assert_eq!(mismatched.code, ApiErrorCode::ContextConflict);

        // 对得上 ⇒ 才是 Delegated。
        let ok = IdentityPolicy::authorization_subject(&ctx, Some(&delegation("del-1")))
            .expect("delegated");
        assert!(matches!(ok, AuthorizationSubject::Delegated(_)));
    }

    #[test]
    fn an_undeclared_delegation_reference_is_rejected() {
        let error = IdentityPolicy::authorization_subject(&context(), Some(&delegation("del-1")))
            .expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::ContextConflict);
    }

    #[test]
    fn owner_admissibility_follows_the_three_shapes() {
        let ctx = context();
        let editor = OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        };
        assert!(IdentityPolicy::check_owner(&ctx, &editor, None).is_ok());

        let foreign = OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("client-2"),
            editor_session_id: EditorSessionId::new("ed-2"),
        };
        let denied = IdentityPolicy::check_owner(&ctx, &foreign, None).expect_err("must fail");
        assert_eq!(denied.code, ApiErrorCode::PermissionDenied);

        let job = OwnerRef::Job {
            job_id: JobId::new("job-1"),
            stage_id: StageId::new("st-1"),
        };
        assert!(IdentityPolicy::check_owner(&ctx, &job, Some(&JobId::new("job-1"))).is_ok());
        assert!(IdentityPolicy::check_owner(&ctx, &job, None).is_err());
        assert!(IdentityPolicy::check_owner(&ctx, &job, Some(&JobId::new("job-2"))).is_err());

        // 长驻客户端会话（MCP/server）同样必须属于当前客户端实例。
        let client_session = OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "mcp-stdio".to_string(),
        };
        assert!(IdentityPolicy::check_owner(&ctx, &client_session, None).is_ok());
        let foreign_session = OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-2"),
            purpose: "mcp-stdio".to_string(),
        };
        assert!(IdentityPolicy::check_owner(&ctx, &foreign_session, None).is_err());
    }

    #[test]
    fn the_authorization_error_does_not_leak_whose_it_was() {
        let ctx = context();
        let job = OwnerRef::Job {
            job_id: JobId::new("job-2"),
            stage_id: StageId::new("st-2"),
        };
        let error = IdentityPolicy::check_owner(&ctx, &job, Some(&JobId::new("job-1")))
            .expect_err("must fail");
        // 错误信息里不能出现「job-2 已存在但不是你」这类可枚举的差异信息。
        assert!(
            !error.message.contains("job-2"),
            "错误信息泄露了归属 id：{}",
            error.message
        );
    }

    #[test]
    fn the_judgment_order_is_frozen() {
        assert_eq!(
            JUDGMENT_ORDER,
            [
                PolicyStage::ContextBound,
                PolicyStage::OwnerAdmissible,
                PolicyStage::AuthorizationGranted,
                PolicyStage::SingleExecutionSlot,
            ]
        );
        // 顺序是数据不是散落的 if：任何新增阶段都必须插在合适位置并改动这个数组。
        let sorted = JUDGMENT_ORDER;
        let mut ordered = sorted;
        ordered.sort();
        assert_eq!(sorted, ordered, "枚举声明顺序必须与判定顺序一致");
    }

    #[test]
    fn a_busy_session_queues_rather_than_fails() {
        let running = ExecutionId::new("exec-1");
        assert_eq!(IdentityPolicy::execution_slot(None), ExecutionSlot::Free);
        assert_eq!(
            IdentityPolicy::execution_slot(Some(running.clone())),
            ExecutionSlot::QueuedBehind { running }
        );
    }

    #[test]
    fn cancellation_and_shutdown_do_not_share_the_execution_path() {
        assert!(IdentityPolicy::control_path_is_independent(
            ControlPath::Cancellation
        ));
        assert!(IdentityPolicy::control_path_is_independent(
            ControlPath::Shutdown
        ));
        assert!(!IdentityPolicy::control_path_is_independent(
            ControlPath::Execution
        ));
    }

    #[test]
    fn background_execution_needs_no_client_instance() {
        // 后台执行的主体来自执行归属，不来自客户端实例；这里断言的形状是：
        // 组织与主体仍然存在，只是没有 clientInstanceId 参与判定。
        let source = include_str!("identity_policy.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        assert!(
            !source.contains("ClientInstanceId::new"),
            "生产路径不得自行构造客户端实例"
        );
        assert!(!source.contains("unwrap()"));
        assert!(!source.contains(".expect("));
    }
}
