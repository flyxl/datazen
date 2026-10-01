//! 桌面本地身份的组装（概要 §5 表格第一列 + §5.1 三条硬约束）。
//!
//! ## 字段来源逐项对照
//!
//! | 字段 | 桌面取值 | 本文件对应 |
//! | --- | --- | --- |
//! | `organizationId` | 固定本地组织 ID（进程常量） | [`LOCAL_ORGANIZATION_ID`] |
//! | `principalId` | 当前桌面登录用户 | [`local_principal`] |
//! | `authenticationSessionId` | `None`（本地模式无登录会话） | [`DesktopIdentity::request_context`] |
//! | `clientInstanceId` | 每次应用启动生成一个实例 ID | [`DesktopIdentity::new_launch`] |
//! | `requestId` | 每个 IPC 调用生成一个 | [`new_request_id`] |
//! | `delegationId` | `None` | [`DesktopIdentity::request_context`] |
//!
//! ## 三条硬约束在本文件里的落点
//!
//! 1. **身份只来自 adapter。** [`RequestContext`] 是 `datazen_platform_api` 的类型，本文件
//!    是 desktop 侧**唯一**构造它的地方（`request_context`）。请求 DTO 不含身份字段，
//!    本模块也不提供任何「从入参填身份」的入口。
//! 2. **后台执行不依赖 GUI 上下文。** 本模块只组装桌面本地身份，不给 `client_instance_id`
//!    之外的隐式兜底；`client_instance_id` 不是授权依据，归属校验在 [`OwnerIntent::admit`]。
//! 3. **OwnerRef 与身份绑定。** 归属不是自由文本：`admit_editor` 必须比对当前实例 ID，
//!    `admit_job` 在桌面形态下**失败关闭**（桌面没有「已授权 Job」记录源，接上去就是
//!    放行任意 job owner）。

use datazen_application::error::{ApiError, ApiErrorCode};
use datazen_platform_api::context::{OwnerRef, RequestContext};
use datazen_platform_api::id::{
    BlockId, ClientInstanceId, EditorSessionId, JobId, OrganizationId, PrincipalId, RequestId,
    StageId,
};

/// 桌面本地形态的固定组织 ID（§5 第一列：进程常量）。
///
/// 桌面不存在跨组织隔离：一份 app data 就是一个组织。这**不是**"伪共享"——组织维度上
/// 桌面本来就是单组织；但它也**不**意味着主体可以合并：`principalId` 仍然逐次解析。
pub const LOCAL_ORGANIZATION_ID: &str = "datazen-desktop-local";

/// 取当前桌面登录用户作为 `principalId`。
///
/// 桌面形态下 `{appData}` 本身即按 OS 账号隔离，因此"该用户"与"该 app data 目录的属主"
/// 是同一个人。取不到时返回 `None`，由调用方决定失败策略（默认失败关闭），
/// **绝不在此处伪造一个占位主体**。
pub fn local_principal() -> Option<PrincipalId> {
    // 优先显式注入（测试与「按配置文件切换账号」的打包形态走这条）。
    if let Ok(explicit) = std::env::var("DATAZEN_DESKTOP_USER") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PrincipalId::new(trimmed));
        }
    }
    #[cfg(windows)]
    let candidate = std::env::var("USERNAME").ok();
    #[cfg(not(windows))]
    let candidate = std::env::var("USER").ok();

    candidate
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PrincipalId::new)
}

/// 单次应用启动的桌面身份。进程内构造一次，逐次 IPC 调用只换 `requestId`。
///
/// 无 `Default`：概要 §6.1 明确身份不得有默认值兜底，空组织 / 空主体 / 空实例 ID 一律拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopIdentity {
    organization_id: OrganizationId,
    principal_id: PrincipalId,
    client_instance_id: ClientInstanceId,
}

impl DesktopIdentity {
    /// 应用启动时组装一次。三个字段任一为空 ⇒ `ApiErrorCode::Unauthenticated`。
    pub fn new_launch(
        organization_id: &str,
        principal_id: &str,
        client_instance_id: &str,
    ) -> Result<Self, ApiError> {
        let organization_id = organization_id.trim();
        let principal_id = principal_id.trim();
        let client_instance_id = client_instance_id.trim();
        if organization_id.is_empty() || principal_id.is_empty() || client_instance_id.is_empty() {
            return Err(ApiError::new(
                ApiErrorCode::Unauthenticated,
                "桌面本地身份不可用：组织 / 主体 / 客户端实例 ID 不得为空",
            ));
        }
        Ok(Self {
            organization_id: OrganizationId::new(organization_id),
            principal_id: PrincipalId::new(principal_id),
            client_instance_id: ClientInstanceId::new(client_instance_id),
        })
    }

    /// 用已解析好的桌面登录用户组装；取不到用户 ⇒ 失败关闭。
    pub fn from_desktop_user(client_instance_id: &str) -> Result<Self, ApiError> {
        let principal = local_principal().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::Unauthenticated,
                "无法确定当前桌面登录用户，拒绝以占位主体继续",
            )
        })?;
        Self::new_launch(
            LOCAL_ORGANIZATION_ID,
            principal.as_str(),
            client_instance_id,
        )
    }

    /// 生成本次 IPC 调用的身份上下文。§5 第一列：本地模式无登录会话、无委托。
    pub fn request_context(&self, request_id: RequestId) -> RequestContext {
        RequestContext::new(
            self.organization_id.clone(),
            self.principal_id.clone(),
            None,
            self.client_instance_id.clone(),
            request_id,
            None,
        )
    }

    /// 组装 + 生成 `requestId` 的便捷入口。
    pub fn next_request_context(&self) -> RequestContext {
        self.request_context(new_request_id())
    }

    pub fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }

    pub fn principal_id(&self) -> &PrincipalId {
        &self.principal_id
    }

    pub fn client_instance_id(&self) -> &ClientInstanceId {
        &self.client_instance_id
    }
}

/// 每个 IPC 调用一个 `requestId`。UUID v4，无序号、无时间戳语义。
pub fn new_request_id() -> RequestId {
    RequestId::new(uuid::Uuid::new_v4().to_string())
}

/// 每次应用启动一个 `clientInstanceId`（概要 §5 桌面形态字段来源）。
///
/// 与 `requestId` 的区别是**生命周期**：实例 ID 跨调用复用，`requestId` 每次新建。
/// 它**不是**授权依据（概要 §5.1(3)），只用于把 owner 归属钉在同一进程实例上。
pub fn new_client_instance_id() -> ClientInstanceId {
    ClientInstanceId::new(uuid::Uuid::new_v4().to_string())
}

/// 归属意图。描述"调用方想认领什么 owner"，**不代表**该归属成立。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerIntent {
    /// 编辑器页签。必须携带候选 `clientInstanceId` 以便 adapter 校验归属。
    Editor {
        claimed_client_instance_id: ClientInstanceId,
        editor_session_id: EditorSessionId,
    },
    /// 任务阶段。桌面形态没有"已授权 Job"记录源，一律失败关闭。
    Job { job_id: JobId, stage_id: StageId },
    /// 工作流块。与 [`OwnerIntent::Job`] 同理。
    WorkflowBlock { job_id: JobId, block_id: BlockId },
    /// 非编辑器长驻客户端会话（MCP stdio 等）。
    ClientSession { purpose: String },
}

impl OwnerIntent {
    /// adapter 层归属校验（概要 §5.1(3)）。
    ///
    /// * `Editor`：`claimed_client_instance_id` 必须**等于**本进程的实例 ID，
    ///   否则 `PermissionDenied`。这条是硬拒绝，不是降级。
    /// * `Job` / `WorkflowBlock`：需要"已授权 Job"记录源，桌面形态未接 ⇒
    ///   `CapabilityUnsupported`（失败关闭，绝不放行任意 job owner）。
    /// * `ClientSession`：绑当前实例 ID。
    pub fn admit(&self, identity: &DesktopIdentity) -> Result<OwnerRef, ApiError> {
        match self {
            Self::Editor {
                claimed_client_instance_id,
                editor_session_id,
            } => {
                if claimed_client_instance_id != identity.client_instance_id() {
                    return Err(ApiError::new(
                        ApiErrorCode::PermissionDenied,
                        format!(
                            "editor owner 属于客户端 {claimed_client_instance_id}，\
                             与当前实例 {current} 不符",
                            current = identity.client_instance_id()
                        ),
                    ));
                }
                Ok(OwnerRef::Editor {
                    client_instance_id: identity.client_instance_id().clone(),
                    editor_session_id: editor_session_id.clone(),
                })
            }
            Self::Job { job_id, stage_id } => Err(Self::unauthorized_job_owner(
                "Job",
                job_id,
                stage_id.as_str(),
            )),
            Self::WorkflowBlock { job_id, block_id } => Err(Self::unauthorized_job_owner(
                "WorkflowBlock",
                job_id,
                block_id.as_str(),
            )),
            Self::ClientSession { purpose } => {
                let purpose = purpose.trim();
                if purpose.is_empty() {
                    return Err(ApiError::new(
                        ApiErrorCode::InvalidArgument,
                        "client session owner 缺少 purpose",
                    ));
                }
                Ok(OwnerRef::ClientSession {
                    client_instance_id: identity.client_instance_id().clone(),
                    purpose: purpose.to_owned(),
                })
            }
        }
    }

    /// job / block 归属缺少授权记录源时的统一失败关闭。
    ///
    /// 选 `CapabilityUnsupported` 而不是 `PermissionDenied`：这里**不是**判定"无权"，
    /// 而是"桌面形态没有可判定的记录源"。若将来接上 Job 授权记录，代码会走真实校验分支。
    fn unauthorized_job_owner(kind: &str, job_id: &JobId, sub_id: &str) -> ApiError {
        tracing::warn!(
            kind,
            job_id = job_id.as_str(),
            sub_id,
            "拒绝 job/workflowBlock owner：桌面形态无已授权 Job 记录源"
        );
        ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            format!(
                "{kind} owner（job={job_id}, sub={sub_id}）需要已授权 Job 记录源；\
                 桌面形态尚未接入，拒绝放行"
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_application::identity_policy::IdentityPolicy;
    use datazen_platform_api::ports::policy::AuthorizationSubject;

    fn identity() -> DesktopIdentity {
        DesktopIdentity::new_launch(LOCAL_ORGANIZATION_ID, "alice", "client-1")
            .expect("固定输入应当组装成功")
    }

    #[test]
    fn launch_identity_fills_the_desktop_column_of_the_context_table() {
        let id = identity();
        let ctx = id.request_context(new_request_id());
        assert_eq!(ctx.organization_id.as_str(), LOCAL_ORGANIZATION_ID);
        assert_eq!(ctx.principal_id.as_str(), "alice");
        // 桌面形态没有登录会话、没有委托。
        assert!(ctx.authentication_session_id.is_none());
        assert!(ctx.delegation_id.is_none());
        assert!(!ctx.is_delegated());
        assert_eq!(ctx.client_instance_id.as_str(), "client-1");
        assert!(!ctx.request_id.as_str().is_empty());
    }

    #[test]
    fn each_call_gets_a_distinct_request_id() {
        let id = identity();
        let a = id.next_request_context();
        let b = id.next_request_context();
        assert_ne!(a.request_id, b.request_id, "requestId 必须逐次生成");
        assert_eq!(a.principal_id, b.principal_id);
        assert_eq!(a.client_instance_id, b.client_instance_id);
    }

    #[test]
    fn launch_identity_refuses_empty_fields_instead_of_defaulting() {
        for (org, user, client) in [
            ("", "alice", "client-1"),
            (LOCAL_ORGANIZATION_ID, "  ", "client-1"),
            (LOCAL_ORGANIZATION_ID, "alice", ""),
        ] {
            let err = DesktopIdentity::new_launch(org, user, client)
                .err()
                .unwrap_or_else(|| panic!("{org}/{user}/{client} 应当被拒绝"));
            assert_eq!(err.code, ApiErrorCode::Unauthenticated);
        }
    }

    #[test]
    fn assembled_context_is_accepted_by_the_identity_policy() {
        let ctx = identity().next_request_context();
        let subject = IdentityPolicy::authorization_subject(&ctx, None)
            .unwrap_or_else(|e| panic!("本地身份应当可授权：{}", e.message));
        match subject {
            AuthorizationSubject::Principal { principal_id } => {
                assert_eq!(principal_id.as_str(), "alice");
            }
            other => panic!("桌面无委托，不应得到 {other:?}"),
        }
    }

    #[test]
    fn editor_owner_must_belong_to_the_current_client_instance() {
        let id = identity();
        let ok = OwnerIntent::Editor {
            claimed_client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("tab-1"),
        }
        .admit(&id);
        assert!(matches!(ok, Ok(OwnerRef::Editor { .. })));

        let denied = OwnerIntent::Editor {
            claimed_client_instance_id: ClientInstanceId::new("client-other"),
            editor_session_id: EditorSessionId::new("tab-1"),
        }
        .admit(&id)
        .err()
        .unwrap_or_else(|| panic!("跨实例 editor owner 必须被拒"));
        assert_eq!(denied.code, ApiErrorCode::PermissionDenied);
    }

    #[test]
    fn job_owner_fails_closed_because_desktop_has_no_authorized_job_record() {
        let id = identity();
        let err = OwnerIntent::Job {
            job_id: JobId::new("job-1"),
            stage_id: StageId::new("stage-1"),
        }
        .admit(&id)
        .err()
        .unwrap_or_else(|| panic!("桌面不得放行任意 job owner"));
        assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
        // 失败关闭必须是错误，不是静默降级成 client session。
        assert!(!err.message.contains("clientSession"));
    }

    #[test]
    fn workflow_block_owner_also_fails_closed() {
        let err = OwnerIntent::WorkflowBlock {
            job_id: JobId::new("job-1"),
            block_id: BlockId::new("block-1"),
        }
        .admit(&identity())
        .err()
        .unwrap_or_else(|| panic!("桌面不得放行任意 workflowBlock owner"));
        assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
    }

    #[test]
    fn client_session_owner_requires_a_purpose() {
        let id = identity();
        assert!(OwnerIntent::ClientSession {
            purpose: "mcp-stdio".to_owned()
        }
        .admit(&id)
        .is_ok());
        let err = OwnerIntent::ClientSession {
            purpose: "   ".to_owned(),
        }
        .admit(&id)
        .err()
        .unwrap_or_else(|| panic!("空 purpose 必须被拒"));
        assert_eq!(err.code, ApiErrorCode::InvalidArgument);
    }

    /// INV-01 的形状断言：本模块**没有任何函数把身份当作参数接收**。
    ///
    /// 身份只能由本模块组装；一旦某个 `fn` 的参数列表里出现 `OrganizationId` /
    /// `PrincipalId` / `ClientInstanceId` / `RequestContext`，就说明有人打算"从调用方
    /// 拿身份"，那正是 CM §3 INV-01 禁止的"从请求体填身份"。
    #[test]
    fn identity_module_exposes_no_body_derived_identity_entry_point() {
        let production = crate::platform::production_source(include_str!("identity.rs"));
        let parameter_lists = crate::platform::fn_parameter_lists(&production);
        assert!(
            parameter_lists.len() >= 8,
            "参数列表取样器应当覆盖本模块全部函数，实际取到 {} 个",
            parameter_lists.len()
        );
        for list in &parameter_lists {
            for forbidden in [
                "OrganizationId",
                "PrincipalId",
                "ClientInstanceId",
                "AuthenticationSessionId",
                "DelegationId",
                "RequestContext",
            ] {
                assert!(
                    !list.contains(forbidden),
                    "identity.rs 不得把 {forbidden} 当作参数接收：fn({list})"
                );
            }
        }
    }

    /// 对照项：组装入口本身**不**接收任何身份参数，只接收"启动期的一次性字符串"。
    #[test]
    fn launch_takes_plain_strings_not_typed_identity() {
        let production = crate::platform::production_source(include_str!("identity.rs"));
        let lists = crate::platform::fn_parameter_lists(&production);
        let launch = lists
            .iter()
            .find(|list| list.contains("client_instance_id: &str"))
            .expect("应当存在按启动参数组装的入口");
        assert!(
            launch.contains("organization_id: &str") && launch.contains("principal_id: &str"),
            "启动组装只接受未加工字符串，fn({launch})"
        );
    }

    /// 身份字段集合与 §6.1 的六字段一一对应，不多不少。
    #[test]
    fn request_context_has_exactly_the_six_specified_fields() {
        let ctx = identity().next_request_context();
        let json = serde_json::to_value(&ctx).unwrap_or_else(|e| panic!("{e}"));
        let object = json.as_object().unwrap_or_else(|| panic!("应是对象"));
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "authenticationSessionId",
                "clientInstanceId",
                "delegationId",
                "organizationId",
                "principalId",
                "requestId",
            ]
        );
    }
}
